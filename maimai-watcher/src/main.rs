use std::{
    collections::hash_map::{Entry as HMEntry, HashMap},
    iter::once,
    path::PathBuf,
    time::Duration,
};

use actix_web::{middleware::Logger, web, App, HttpServer, Responder};
use anyhow::{bail, Context};
use clap::Parser;
use log::{error, info};
use maimai_watcher::{
    slack::webhook_send,
    watch::{self, TimeoutConfig, WatchHandler},
};
use serde::Deserialize;
use splitty::split_unquoted_whitespace;
use tokio::sync::Mutex;
use url::Url;

#[derive(Parser)]
struct Opts {
    #[clap(default_value = "ignore/maimai-watcher-config.toml")]
    config_path: PathBuf,
}

#[derive(Clone, Deserialize)]
struct Config {
    port: u16,
    webhook_endpoint: String,
    interval: Duration,
    levels_path: PathBuf,
    removed_songs_path: PathBuf,
    slack_post_webhook: Option<Url>,
    users: HashMap<UserId, UserConfig>,
    timeout_hours: f64,
}
// #[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
// struct UserId(String);
type UserId = String;
#[derive(Clone, Deserialize)]
struct UserConfig {
    slack_user_id: String,
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    user_data_path: PathBuf,
    estimate_internal_levels: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    let config: Config = toml::from_str(&fs_err::read_to_string(opts.config_path)?)?;
    let port = config.port;
    let route = config.webhook_endpoint.clone();

    let reqwest_client = reqwest::Client::new();
    let url = config.slack_post_webhook.clone();
    let webhook_send = |message: &'static str| webhook_send(&reqwest_client, &url, message);

    webhook_send("The server has started.").await;

    HttpServer::new(move || {
        let mut slack_id_to_user_id = HashMap::<_, Vec<_>>::new();
        for (id, config) in &config.users {
            slack_id_to_user_id
                .entry(config.slack_user_id.clone())
                .or_default()
                .push(id.clone());
        }
        App::new()
            .app_data(web::Data::new(State {
                slack_id_to_user_id,
                config: config.clone(),
                watch_handler: Mutex::new(HashMap::new()),
            }))
            .route(&route, web::post().to(webhook))
            .wrap(Logger::default())
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await?;

    webhook_send("The server is about to shut down.").await;

    Ok(())
}

struct State {
    config: Config,
    slack_id_to_user_id: HashMap<String, Vec<UserId>>,
    watch_handler: Mutex<HashMap<UserId, WatchHandler>>,
}

async fn webhook(state: web::Data<State>, info: web::Form<SlashCommand>) -> impl Responder {
    let client = reqwest::Client::new();
    let url = state.config.slack_post_webhook.clone();
    if let Err(e) = webhook_impl(state, info, &client).await {
        error!("{e}");
        webhook_send(&client, &url, e.to_string()).await;
    };
    "done"
}

mod slash_command {
    use super::UserId;
    use clap::{Args, Parser, Subcommand};

    #[derive(Parser)]
    pub struct Opts {
        #[clap(subcommand)]
        pub sub: Sub,
    }
    #[derive(Subcommand)]
    pub enum Sub {
        Stop(Stop),
        Start(Start),
        Single(Single),
    }
    #[derive(Args)]
    pub struct Stop {
        pub user_id: Option<UserId>,
    }
    #[derive(Args)]
    pub struct Start {
        pub user_id: Option<UserId>,
    }
    #[derive(Args)]
    pub struct Single {
        pub user_id: Option<UserId>,
    }
}

async fn webhook_impl(
    state: web::Data<State>,
    info: web::Form<SlashCommand>,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    info!("Slash command: {info:?}");

    let [user_id] = &state
        .slack_id_to_user_id
        .get(&info.user_id)
        .context("You are not authorized to run this command.")?[..]
    else {
        bail!("Multiple accounts are possible.  Choose an account.")
    };
    let user_config = &state.config.users[user_id];

    macro_rules! post {
        ($message: literal) => {
            let url = &state.config.slack_post_webhook;
            webhook_send(client, url, $message).await
        };
    }

    let args = slash_command::Opts::try_parse_from(
        once("maimai-watcher").chain(split_unquoted_whitespace(&info.text).unwrap_quotes(true)),
    )?;
    use HMEntry::*;
    match args.sub {
        slash_command::Sub::Stop(_args) => {
            let mut map = state.watch_handler.lock().await;
            drop_if_closed(map.entry(user_id.clone()));
            match map.entry(user_id.clone()) {
                Occupied(entry) => {
                    entry.remove().stop().await?;
                    post!("Stopped!");
                }
                Vacant(_) => {
                    post!("Watcher is not running!");
                }
            }
        }
        slash_command::Sub::Start(_args) => {
            let mut map = state.watch_handler.lock().await;
            drop_if_closed(map.entry(user_id.clone()));
            match map.entry(user_id.clone()) {
                Occupied(_) => {
                    post!("Watcher is already running!");
                }
                Vacant(entry) => {
                    let timeout = TimeoutConfig::hours(state.config.timeout_hours);
                    let config = watch_config(&state.config, user_config, timeout, false);
                    entry.insert(watch::watch(config).await?);
                    post!("Started!");
                }
            }
        }
        slash_command::Sub::Single(_args) => {
            let config = watch_config(&state.config, user_config, TimeoutConfig::single(), true);
            watch::watch(config).await?;
        }
    };
    Ok(())
}

fn watch_config(
    state_config: &Config,
    user_config: &UserConfig,
    timeout_config: TimeoutConfig,
    report_no_updates: bool,
) -> watch::Config {
    watch::Config {
        interval: state_config.interval,
        credentials_path: user_config.credentials_path.clone(),
        cookie_store_path: user_config.cookie_store_path.clone(),
        maimai_uesr_data_path: user_config.user_data_path.clone(),
        levels_path: state_config.levels_path.clone(),
        removed_songs_path: state_config.removed_songs_path.clone(),
        slack_post_webhook: state_config.slack_post_webhook.clone(),
        estimate_internal_levels: user_config.estimate_internal_levels,
        timeout_config,
        report_no_updates,
    }
}

fn drop_if_closed<K>(entry: HMEntry<K, WatchHandler>) {
    if let HMEntry::Occupied(entry) = entry {
        if entry.get().is_dropped() {
            entry.remove();
        }
    }
}

#[derive(Deserialize, Debug)]
#[allow(unused)]
struct SlashCommand {
    token: String,
    command: String,
    text: String,
    response_url: String,
    trigger_id: String,
    user_id: String,
    user_name: String,
    team_id: String,
    api_app_id: String,
}

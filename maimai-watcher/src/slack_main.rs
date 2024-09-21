use std::{
    collections::{
        hash_map::{Entry as HMEntry, HashMap},
        HashSet,
    },
    iter::once,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

use actix_web::{middleware::Logger, web, App, HttpServer, Responder};
use anyhow::{bail, Context};
use clap::Parser;
use log::{error, info};
use maimai_scraping::cookie_store::UserIdentifier;
use serde::Deserialize;
use splitty::split_unquoted_whitespace;
use tokio::sync::Mutex;
use url::Url;

use crate::{
    misc,
    slack::webhook_send,
    watch::{self, AimeSwitchConfig, ForcePaidConfig, TimeoutConfig, UserId, WatchHandler},
};

#[derive(Parser)]
struct Opts {
    #[arg(default_value = "ignore/maimai-watcher-config.toml")]
    config_path: PathBuf,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    port: u16,
    webhook_endpoint: String,
    interval: Duration,

    database_path: Option<PathBuf>,
    estimator_config_path: Option<PathBuf>,

    slack_post_webhook: Option<Url>,
    // TODO make it getter
    pub users: HashMap<UserId, UserConfig>,
    timeout_hours: f64,
    #[serde(default)]
    default_users: HashMap<String, UserId>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserConfig {
    slack_user_ids: Vec<String>,
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    user_data_path: PathBuf,
    estimate_internal_levels: bool,
    user_identifier: UserIdentifier,
    #[serde(default)]
    international: bool,
    #[serde(default)]
    force_paid_config: Option<ForcePaidConfig>,
    #[serde(default)]
    aime_switch_config: Option<AimeSwitchConfig>,
}

pub async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    let config: Config = toml::from_str(&fs_err::read_to_string(opts.config_path)?)?;
    let port = config.port;
    let route = config.webhook_endpoint.clone();

    let reqwest_client = reqwest::Client::new();
    let url = config.slack_post_webhook.clone();
    let webhook_send = |message: &'static str| webhook_send(&reqwest_client, &url, None, message);

    webhook_send("The server has started.").await;

    HttpServer::new(move || {
        let mut slack_id_to_user_id = HashMap::<_, Vec<_>>::new();
        let mut slack_id_user_id_pairs = HashSet::new();
        for (id, config) in &config.users {
            for slack_id in &config.slack_user_ids {
                slack_id_to_user_id
                    .entry(slack_id.clone())
                    .or_default()
                    .push(id.clone());
                slack_id_user_id_pairs.insert((slack_id, id));
            }
        }
        let mut invalid = false;
        for pair in &config.default_users {
            if !slack_id_user_id_pairs.contains(&pair) {
                error!("Invalid default user (not in permission list): {pair:?}");
                invalid = true;
            }
        }
        if invalid {
            panic!("One or more invalid default user pairs were found.");
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
        error!("{e:#}");
        webhook_send(&client, &url, None, format!("{e:#}")).await;
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
        Recent(Recent),
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
    #[derive(Args)]
    pub struct Recent {
        pub user_id: Option<UserId>,
        #[arg(default_value = "5")]
        pub count: usize,
    }
}

async fn webhook_impl(
    state: web::Data<State>,
    info: web::Form<SlashCommand>,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    info!("Slash command: {info:?}");

    macro_rules! post {
        ($user_id: expr, $message: literal) => {
            let url = &state.config.slack_post_webhook;
            webhook_send(client, url, $user_id, $message).await
        };
    }

    let args = slash_command::Opts::try_parse_from(
        once("maimai-watcher").chain(split_unquoted_whitespace(&info.text).unwrap_quotes(true)),
    )?;
    use HMEntry::*;
    match args.sub {
        slash_command::Sub::Stop(sub_args) => {
            let (user_id, _) = get_user_id(&state, &info, &sub_args.user_id)?;
            let mut map = state.watch_handler.lock().await;
            drop_if_closed(map.entry(user_id.clone()));
            match map.entry(user_id.clone()) {
                Occupied(entry) => {
                    entry.remove().stop().await?;
                    post!(user_id, "Stopped!");
                }
                Vacant(_) => {
                    post!(user_id, "Watcher is not running!");
                }
            }
        }
        slash_command::Sub::Start(sub_args) => {
            let (user_id, user_config) = get_user_id(&state, &info, &sub_args.user_id)?;
            let mut map = state.watch_handler.lock().await;
            drop_if_closed(map.entry(user_id.clone()));
            match map.entry(user_id.clone()) {
                Occupied(_) => {
                    post!(user_id, "Watcher is already running!");
                }
                Vacant(entry) => {
                    let timeout = TimeoutConfig::hours(state.config.timeout_hours);
                    let config = watch_config(
                        user_id.clone(),
                        &state.config,
                        user_config,
                        timeout,
                        false,
                        None,
                    );
                    entry.insert(watch::watch(config).await?);
                    post!(user_id, "Started!");
                }
            }
        }
        slash_command::Sub::Single(sub_args) => {
            let (user_id, user_config) = get_user_id(&state, &info, &sub_args.user_id)?;
            let config = watch_config(
                user_id.clone(),
                &state.config,
                user_config,
                TimeoutConfig::single(),
                true,
                None,
            );
            watch::watch(config).await?;
        }
        slash_command::Sub::Recent(sub_args) => {
            let config = state.config.clone();
            let (user_id, user_config) = get_user_id(&state, &info, &sub_args.user_id)?;
            let (user_id, user_config) = (user_id.to_owned(), user_config.to_owned());
            tokio::task::spawn(async move {
                let client = reqwest::Client::new();
                if let Err(e) = misc::recent(
                    &client,
                    &config.slack_post_webhook,
                    &user_id,
                    &user_config.user_data_path,
                    config.database_path.as_ref(),
                    config.estimator_config_path.as_ref(),
                    sub_args.count,
                )
                .await
                {
                    error!("{e:#}");
                    webhook_send(
                        &client,
                        &config.slack_post_webhook,
                        Some(&user_id),
                        format!("{e:#}"),
                    )
                    .await;
                }
            });
        }
    };
    Ok(())
}

fn get_user_id<'a>(
    state: &'a web::Data<State>,
    info: &web::Form<SlashCommand>,
    specified_user_id: &'a Option<UserId>,
) -> anyhow::Result<(&'a UserId, &'a UserConfig)> {
    let allowed_users = state
        .slack_id_to_user_id
        .get(&info.user_id)
        .map_or(&[][..], |s| &s[..]);
    let user_id = match specified_user_id.as_ref() {
        Some(id) => {
            if allowed_users.iter().any(|a| a == id) {
                id
            } else {
                bail!("You do not have a permission to this account.")
            }
        }
        None => match allowed_users {
            // No associated default user must be present in this phase, as checked on loading
            [] => bail!("No account is associated to your Slack account."),
            [id] => id,
            _ => match state.config.default_users.get(&info.user_id) {
                Some(id) => id,
                None => {
                    bail!(
                        "Multiple accounts are associated to your Slack account.  You must explicitly specify the account."
                    )
                }
            },
        },
    };
    let user_config = &state
        .config
        .users
        .get(user_id)
        .with_context(|| format!("Account not found: {user_id:?}"))?;
    Ok((user_id, user_config))
}

pub fn watch_config(
    user_id: UserId,
    state_config: &Config,
    user_config: &UserConfig,
    timeout_config: TimeoutConfig,
    report_no_updates: bool,
    finish_flag: Option<Arc<AtomicBool>>,
) -> watch::Config {
    // let levels_path = if user_config.international {
    //     &state_config.levels_intl_path
    // } else {
    //     &state_config.levels_path
    // };
    let database = (!user_config.international)
        .then_some(state_config.database_path.as_ref())
        .flatten();
    watch::Config {
        user_id,
        interval: state_config.interval,
        credentials_path: user_config.credentials_path.clone(),
        cookie_store_path: user_config.cookie_store_path.clone(),
        maimai_uesr_data_path: user_config.user_data_path.clone(),
        slack_post_webhook: state_config.slack_post_webhook.clone(),
        estimate_internal_levels: user_config.estimate_internal_levels,
        timeout_config,
        report_no_updates,
        user_identifier: user_config.user_identifier.clone(),
        international: user_config.international,
        force_paid_config: user_config.force_paid_config.clone(),
        aime_switch_config: user_config.aime_switch_config.clone(),

        database_path: database.cloned(),
        estimator_config_path: state_config.estimator_config_path.clone(),

        finish_flag,
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

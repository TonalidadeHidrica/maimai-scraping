use actix_web::{middleware::Logger, web, App, HttpServer, Responder};
use anyhow::bail;
use log::{error, info};
use maimai_watcher::{
    slack::webhook_send,
    watch::{self, WatchHandler},
};
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone, Deserialize)]
struct Config {
    port: u16,
    route: String,
    user_id: String,
    watch_config: watch::Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config: Config = toml::from_str(&fs_err::read_to_string("maimai-watcher/config.toml")?)?;
    let port = config.port;
    let route = config.route.clone();
    Ok(HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(State {
                config: config.clone(),
                watch_handler: Mutex::new(None),
            }))
            .route(&route, web::post().to(webhook))
            .wrap(Logger::default())
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await?)
}

struct State {
    config: Config,
    watch_handler: Mutex<Option<WatchHandler>>,
}

async fn webhook(state: web::Data<State>, info: web::Form<SlashCommand>) -> impl Responder {
    let client = reqwest::Client::new();
    if let Err(e) = webhook_impl(state, info, &client).await {
        error!("{e}");
    };
    "done"
}

async fn webhook_impl(
    state: web::Data<State>,
    info: web::Form<SlashCommand>,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    if state.config.user_id != info.user_id {
        bail!("You are not authorized to run this command.");
    }
    info!("Slash command: {info:?}");

    macro_rules! post {
        ($message: literal) => {
            if let Some(url) = &state.config.watch_config.slack_post_webhook {
                webhook_send(client, url, $message).await
            }
        };
    }
    if info.text.contains("stop") {
        let mut handler = state.watch_handler.lock().await;
        if let Some(handler) = handler.take() {
            handler.stop().await?;
            post!("Stopped!");
        } else {
            post!("Watcher is not running!");
        }
    } else if info.text.contains("start") {
        let mut handler = state.watch_handler.lock().await;
        if handler.is_some() {
            post!("Watcher is already running!");
        } else {
            *handler = Some(watch::watch(state.config.watch_config.clone()).await?);
            post!("Started!");
        }
    } else {
        bail!("Invalid command: {:?}", info.text)
    };
    Ok(())
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

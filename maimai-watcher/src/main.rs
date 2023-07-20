use actix_web::{middleware::Logger, web, App, HttpServer, Responder};
use anyhow::bail;
use log::info;
use maimai_watcher::watch::{self, WatchHandler};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use url::Url;

#[derive(Clone, Deserialize)]
struct Config {
    port: u16,
    route: String,
    user_id: String,
    #[allow(unused)]
    slack_post_webhook: Url,
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

async fn webhook(
    state: web::Data<State>,
    info: web::Form<SlashCommand>,
) -> actix_web::Result<impl Responder> {
    Ok(web::Json(
        webhook_impl(state, info)
            .await
            .map_or_else(|e| in_channel(format!("Error: {e}")), |x| x),
    ))
}

async fn webhook_impl(
    state: web::Data<State>,
    info: web::Form<SlashCommand>,
) -> anyhow::Result<SlashCommandResponse> {
    if state.config.user_id != info.user_id {
        bail!("You are not authorized to run this command.");
    }
    info!("Slash command: {info:?}");

    let res = if info.text.contains("stop") {
        let mut handler = state.watch_handler.lock().await;
        if let Some(handler) = handler.take() {
            handler.stop().await?;
            in_channel("Stopped!")
        } else {
            in_channel("Watcher is not running!")
        }
    } else if info.text.contains("start") {
        let mut handler = state.watch_handler.lock().await;
        if handler.is_some() {
            in_channel("Watcher is already running!")
        } else {
            *handler = Some(watch::watch(state.config.watch_config.clone()).await?);
            in_channel("Started!")
        }
    } else {
        bail!("Invalid command: {:?}", info.text)
    };
    Ok(res)
}

#[derive(Deserialize)]
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

#[derive(Serialize)]
struct SlashCommandResponse {
    response_type: SlashCommandResponseType,
    text: String,
}
fn in_channel(message: impl AsRef<str>) -> SlashCommandResponse {
    SlashCommandResponse {
        response_type: SlashCommandResponseType::InChannel,
        text: message.as_ref().to_owned(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SlashCommandResponseType {
    InChannel,
    #[allow(unused)]
    Ephermal,
}

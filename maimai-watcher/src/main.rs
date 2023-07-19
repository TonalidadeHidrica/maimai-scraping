use actix_web::{web, App, HttpServer, Responder};
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Config {
    port: u16,
    route: String,
    user_id: String,
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
            }))
            .route(&route, web::get().to(webhook))
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await?)
}

struct State {
    config: Config,
}

async fn webhook(state: web::Data<State>, info: web::Query<SlashCommand>) -> impl Responder {
    if state.config.user_id == info.user_id {
        "Hello world!"
    } else {
        "Go away!"
    }
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

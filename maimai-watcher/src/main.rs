use actix_web::{get, web, App, HttpServer, Responder};
use serde::Deserialize;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(webhook))
        .bind(("0.0.0.0", 15342))?
        .run()
        .await
}

#[get("/kiet4AeraeTaebooS9ekuequia0DuNgoodooquie7AZei4uovo")]
async fn webhook(info: web::Query<SlashCommand>) -> impl Responder {
    "Hello world!"
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

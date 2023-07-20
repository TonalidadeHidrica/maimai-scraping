use log::error;
use serde::Serialize;
use url::Url;

#[allow(unused)]
#[derive(Serialize)]
struct SlashCommandResponse {
    response_type: SlashCommandResponseType,
    text: String,
}
#[allow(unused)]
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SlashCommandResponseType {
    InChannel,
    Ephermal,
}
#[allow(unused)]
fn in_channel(message: impl AsRef<str>) -> SlashCommandResponse {
    SlashCommandResponse {
        response_type: SlashCommandResponseType::InChannel,
        text: message.as_ref().to_owned(),
    }
}

#[derive(Serialize)]
struct WebhookPost<'a> {
    text: &'a str,
}

pub async fn webhook_send(client: &reqwest::Client, url: &Option<Url>, message: impl AsRef<str>) {
    let Some(url) = url else { return };
    if let Err(e) = client
        .post(url.clone())
        .json(&WebhookPost {
            text: message.as_ref(),
        })
        .send()
        .await
    {
        error!("{e}")
    }
}

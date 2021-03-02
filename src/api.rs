use crate::cookie_store::CookieStore;
use crate::play_record_parser::parse;
use crate::schema::PlayRecord;
use reqwest::header;
use scraper::Html;

pub fn reqwest_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_store(true)
        .connection_verbose(true)
        .build()
}

/// # Panics
/// - If idx >= 50.
pub async fn download_page(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
    idx: u8,
) -> anyhow::Result<PlayRecord> {
    assert!(idx < 50);
    let url = format!(
        "https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx={}",
        idx
    );
    let response = client
        .get(&url)
        .header(header::COOKIE, format!("userId={}", cookie_store.user_id))
        .send()
        .await?;
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        cookie_store.user_id = cookie.value().to_owned();
        cookie_store.save()?;
    }
    let document = Html::parse_document(&response.text().await?);
    parse(document)
}

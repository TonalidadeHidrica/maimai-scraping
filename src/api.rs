use crate::cookie_store::CookieStore;
use crate::play_record_parser::parse;
use crate::schema::Idx;
use crate::schema::PlayRecord;
use anyhow::anyhow;
use reqwest::header;
use reqwest::redirect;
use reqwest::Url;
use scraper::Html;

pub fn reqwest_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_store(true)
        .connection_verbose(true)
        .redirect(redirect::Policy::none())
        .build()
}

pub async fn download_page(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
    idx: Idx,
) -> anyhow::Result<Option<PlayRecord>> {
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
    if let Some(location) = response.headers().get(header::LOCATION).map(|x| x.to_str()) {
        return match location
            .ok()
            .and_then(|x| Url::parse(x).ok())
            .as_ref()
            .map(|x| x.path())
        {
            Some("/maimai-mobile/error/") => {
                Err(anyhow!("Redirected to error page: {:?}", response))
            }
            Some("/maimai-mobile/record/") => Ok(None), // There were less than idx records
            _ => Err(anyhow!("Redirected to error unknown page: {:?}", response)),
        };
    }
    let document = Html::parse_document(&response.text().await?);
    parse(document, idx).map(Some)
}

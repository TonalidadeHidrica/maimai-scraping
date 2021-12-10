use crate::cookie_store::CredentialStore;
use crate::cookie_store::MaybeCredential;
use crate::play_record_parser::parse;
use crate::play_record_parser::parse_record_index;
use crate::play_record_parser::RecordIndexData;
use crate::schema::latest::Idx;
use crate::schema::latest::PlayRecord;
use anyhow::anyhow;
use reqwest::header;
use reqwest::redirect;
use reqwest::IntoUrl;
use reqwest::Url;
use scraper::Html;

pub fn reqwest_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .cookie_store(true)
        .connection_verbose(true)
        .redirect(redirect::Policy::none())
        .build()
}

pub async fn download_record_index(
    client: &reqwest::Client,
    cookie_store: &mut CredentialStore,
) -> anyhow::Result<Vec<RecordIndexData>> {
    let url = "https://maimaidx.jp/maimai-mobile/record/";
    let response = fetch_authenticated(client, url, cookie_store).await?.0;
    let document = Html::parse_document(&response.text().await?);
    parse_record_index(document)
}

pub async fn download_record(
    client: &reqwest::Client,
    cookie_store: &mut CredentialStore,
    idx: Idx,
) -> anyhow::Result<Option<PlayRecord>> {
    let url = format!(
        "https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx={}",
        idx
    );
    let (response, redirect_url) = fetch_authenticated(client, &url, cookie_store).await?;
    if let Some(location) = redirect_url {
        return match location.path() {
            "/maimai-mobile/record/" => Ok(None), // There were less than idx records
            _ => Err(anyhow!("Redirected to error unknown page: {:?}", response)),
        };
    }
    let document = Html::parse_document(&response.text().await?);
    parse(document, idx).map(Some)
}

async fn fetch_authenticated(
    client: &reqwest::Client,
    url: impl IntoUrl,
    cookie_store: &mut CredentialStore,
) -> anyhow::Result<(reqwest::Response, Option<Url>)> {
    let response = client
        .get(url)
        .header(header::COOKIE, format!("userId={}", cookie_store.user_id))
        .send()
        .await?;
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        cookie_store.user_id = cookie.value().to_owned().into();
        cookie_store.save()?;
    }
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|x| Url::parse(x.to_str().ok()?).ok());
    if let Some(location) = &location {
        if location.path() == "/maimai-mobile/error/" {
            return Err(anyhow!("Redirected to error page: {:?}", response));
        }
    }
    Ok((response, location))
}

pub async fn try_login(creds: MaybeCredential) -> anyhow::Result<CredentialStore> {
    let (user_name, password) = (|| Some((creds.user_name?, creds.password?)))()
        .ok_or_else(|| anyhow!("User ID or password is missing; cannot log in."))?;
    todo!();
}

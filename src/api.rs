use crate::cookie_store::CookieStore;
use crate::cookie_store::Credentials;
use crate::cookie_store::Password;
use crate::cookie_store::UserName;
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
use serde::Serialize;

pub fn reqwest_client() -> reqwest::Result<reqwest::Client> {
    // let jar = Arc::new(Jar::default());
    reqwest::Client::builder()
        .cookie_store(true)
        // .cookie_provider(jar.clone())
        .connection_verbose(true)
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.url().path() == "/maimai-mobile/error/" {
                attempt.error(anyhow!("Redirected to error page"))
            } else if attempt
                .previous()
                .last()
                .map_or(false, |x| x.path() == "/maimai-mobile/aimeList/submit/")
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
}

pub async fn download_record_index(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
) -> anyhow::Result<Vec<RecordIndexData>> {
    let url = "https://maimaidx.jp/maimai-mobile/record/";
    let response = fetch_authenticated(client, url, cookie_store).await?.0;
    let document = Html::parse_document(&response.text().await?);
    parse_record_index(document)
}

pub async fn download_record(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
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
    cookie_store: &mut CookieStore,
) -> anyhow::Result<(reqwest::Response, Option<Url>)> {
    let response = client
        .get(url)
        .header(header::COOKIE, format!("userId={}", cookie_store.user_id))
        .send()
        .await?;
    set_and_save_credentials(cookie_store, &response)?;
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|x| Url::parse(x.to_str().ok()?).ok());
    if let Some(location) = &location {
        if location.path() == "/maimai-mobile/errr/" {
            return Err(anyhow!("Redirected to error page: {:?}", response));
        }
    }
    Ok((response, location))
}

pub fn set_and_save_credentials(
    cookie_store: &mut CookieStore,
    response: &reqwest::Response,
) -> anyhow::Result<bool> {
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        cookie_store.user_id = cookie.value().to_owned().into();
        cookie_store.save()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Debug, Serialize)]
struct LoginForm<'a> {
    #[serde(rename = "segaId")]
    sega_id: &'a UserName,
    password: &'a Password,
    save_cookie: &'static str,
    token: &'a str,
}
impl<'a> LoginForm<'a> {
    fn new(credentials: &'a Credentials, token: &'a str) -> Self {
        Self {
            sega_id: &credentials.user_name,
            password: &credentials.password,
            save_cookie: "on",
            token,
        }
    }
}

pub async fn try_login(client: &reqwest::Client) -> anyhow::Result<CookieStore> {
    let credentials = Credentials::load()?;

    let login_form_url = "https://maimaidx.jp/maimai-mobile/";
    let login_form = client.get(login_form_url).send().await?;
    let login_form = Html::parse_document(&login_form.text().await?);
    let token = login_form
        .select(selector!(
            r#"form[action="https://maimaidx.jp/maimai-mobile/submit/"] input[name="token"]"#
        ))
        .next()
        .ok_or_else(|| anyhow!("The token was not found in the login form."))?
        .value()
        .attr("value")
        .ok_or_else(|| anyhow!("Value was not present in the token element."))?;

    let login_url = "https://maimaidx.jp/maimai-mobile/submit/";
    let response = client
        .post(login_url)
        .form(&LoginForm::new(&credentials, token))
        .send()
        .await?;

    let url = response.url().clone();
    if url.as_str() != "https://maimaidx.jp/maimai-mobile/aimeList/" {
        return Err(anyhow!(
            "Error: redirected to unexpected url while logging in: {}",
            url,
        ));
    }

    let url = format!(
        "https://maimaidx.jp/maimai-mobile/aimeList/submit/?idx={}",
        credentials.aime_idx.unwrap_or_default()
    );
    let response = client.get(&url).send().await?;

    let mut cookie_store = CookieStore::default();
    if !set_and_save_credentials(&mut cookie_store, &response)? {
        return Err(anyhow!("Desired cookie was not found."));
    }
    Ok(dbg!(cookie_store))
}

#[cfg(test)]
mod tests {
    use crate::cookie_store::Credentials;

    use super::LoginForm;

    #[test]
    fn test_login_form() {
        let user_name = "abc".to_owned().into();
        let password = "def".to_owned().into();
        let credentials = Credentials {
            user_name,
            password,
            aime_idx: None,
        };
        let form = LoginForm::new(&credentials, "ghi");
        let json = serde_json::to_string(&form).unwrap();
        assert_eq!(
            json,
            r#"{"segaId":"abc","password":"def","save_cookie":"on","token":"ghi"}"#
        );
    }
}

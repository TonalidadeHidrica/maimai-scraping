use std::marker::PhantomData;

use crate::cookie_store::CookieStore;
use crate::cookie_store::Credentials;
use crate::cookie_store::Password;
use crate::cookie_store::UserName;
use crate::sega_trait::SegaTrait;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use reqwest::header;
use reqwest::redirect;
use reqwest::IntoUrl;
use reqwest::Url;
use scraper::Html;
use serde::Serialize;

pub fn reqwest_client<T: SegaTrait>() -> reqwest::Result<reqwest::Client> {
    // let jar = Arc::new(Jar::default());
    reqwest::Client::builder()
        .cookie_store(true)
        // .cookie_provider(jar.clone())
        .connection_verbose(true)
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.url().path() == T::ERROR_PATH {
                attempt.error(anyhow!("Redirected to error page"))
            } else if attempt
                .previous()
                .last()
                .map_or(false, |x| x.path() == T::AIME_SUBMIT_PATH)
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
}

pub async fn download_record_index<T: SegaTrait>(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
) -> anyhow::Result<Vec<(NaiveDateTime, T::Idx)>> {
    let url = T::RECORD_URL;
    let response = fetch_authenticated(client, url, cookie_store).await?.0;
    let document = Html::parse_document(&response.text().await?);
    T::parse_record_index(&document)
}

pub async fn download_record<T: SegaTrait>(
    client: &reqwest::Client,
    cookie_store: &mut CookieStore,
    idx: T::Idx,
) -> anyhow::Result<Option<T::PlayRecord>> {
    let url = T::play_log_detail_url(idx);
    let (response, redirect_url) = fetch_authenticated(client, &url, cookie_store).await?;
    if let Some(location) = redirect_url {
        return if T::play_log_detail_not_found(&location) {
            Ok(None)
        } else {
            Err(anyhow!("Redirected to error unknown page: {:?}", response))
        };
    }
    let document = Html::parse_document(&response.text().await?);
    T::parse(&document, idx).map(Some)
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
struct LoginForm<'a, T> {
    #[serde(rename = "segaId")]
    sega_id: &'a UserName,
    password: &'a Password,
    save_cookie: &'static str,
    token: &'a str,
    #[serde(skip)]
    _phantom: PhantomData<fn() -> T>,
}
impl<'a, T> LoginForm<'a, T> {
    fn new(credentials: &'a Credentials<T>, token: &'a str) -> Self {
        Self {
            sega_id: &credentials.user_name,
            password: &credentials.password,
            save_cookie: "on",
            token,
            _phantom: Default::default(),
        }
    }
}

pub async fn try_login<T: SegaTrait>(client: &reqwest::Client) -> anyhow::Result<CookieStore> {
    let credentials = Credentials::<T>::load()?;

    let login_form = client.get(T::LOGIN_FORM_URL).send().await?;
    let login_form = Html::parse_document(&login_form.text().await?);
    let token = login_form
        .select(T::login_form_token_selector())
        .next()
        .ok_or_else(|| anyhow!("The token was not found in the login form."))?
        .value()
        .attr("value")
        .ok_or_else(|| anyhow!("Value was not present in the token element."))?;

    let login_url = T::LOGIN_URL;
    let response = client
        .post(login_url)
        .form(&LoginForm::new(&credentials, token))
        .send()
        .await?;

    let url = response.url().clone();
    if url.as_str() != T::AIME_LIST_URL {
        return Err(anyhow!(
            "Error: redirected to unexpected url while logging in: {}",
            url,
        ));
    }

    let url = T::select_aime_list_url(credentials.aime_idx.unwrap_or_default());
    let response = client.get(&url).send().await?;

    let mut cookie_store = CookieStore::default();
    if !set_and_save_credentials(&mut cookie_store, &response)? {
        return Err(anyhow!("Desired cookie was not found."));
    }
    Ok(dbg!(cookie_store))
}

#[cfg(test)]
mod tests {
    use crate::{cookie_store::Credentials, maimai::Maimai};

    use super::LoginForm;

    #[test]
    fn test_login_form() {
        let user_name = "abc".to_owned().into();
        let password = "def".to_owned().into();
        let credentials = Credentials::<Maimai>::builder()
            .user_name(user_name)
            .password(password)
            .aime_idx(None)
            .build();
        let form = LoginForm::new(&credentials, "ghi");
        let json = serde_json::to_string(&form).unwrap();
        assert_eq!(
            json,
            r#"{"segaId":"abc","password":"def","save_cookie":"on","token":"ghi"}"#
        );
    }
}

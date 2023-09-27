use std::borrow::Cow;
use std::marker::PhantomData;
use std::path::Path;

use crate::cookie_store::CookieStore;
use crate::cookie_store::CookieStoreLoadError;
use crate::cookie_store::Credentials;
use crate::cookie_store::Password;
use crate::cookie_store::PlayerName;
use crate::cookie_store::SegaId;
use crate::sega_trait::Idx;
use crate::sega_trait::PlayTime;
use crate::sega_trait::SegaTrait;
use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context;
use log::info;
use reqwest::header;
use reqwest::redirect;
use reqwest::IntoUrl;
use reqwest::Url;
use scraper::Html;
use serde::Serialize;

pub struct SegaClient<'p, T: SegaTrait> {
    client: reqwest::Client,
    credentials_path: Cow<'p, Path>,
    cookie_store: CookieStore,
    cookie_store_path: Cow<'p, Path>,
    _phantom: PhantomData<T>,
}

impl<'p, T: SegaTrait> SegaClient<'p, T> {
    pub async fn new_with_default_path(
        player_name: Option<&PlayerName>,
    ) -> anyhow::Result<(SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>)> {
        Self::new(
            Path::new(T::CREDENTIALS_PATH),
            Path::new(T::COOKIE_STORE_PATH),
            player_name,
        )
        .await
    }

    pub async fn new(
        credentials_path: &'p Path,
        cookie_store_path: &'p Path,
        player_name: Option<&PlayerName>,
    ) -> anyhow::Result<(SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>)> {
        let credentials_path = Cow::Borrowed(credentials_path);
        let cookie_store_path = Cow::Borrowed(cookie_store_path);

        let client = reqwest_client::<T>()?;

        let cookie_store = CookieStore::load(cookie_store_path.as_ref());
        let (mut client, index) = match cookie_store {
            Ok(cookie_store) => {
                let mut client = Self {
                    client,
                    credentials_path,
                    cookie_store,
                    cookie_store_path,
                    _phantom: PhantomData,
                };
                let index = client.download_record_index().await;
                (client, index.map_err(Some))
            }
            Err(CookieStoreLoadError::NotFound) => {
                info!("Cookie store was not found.  Trying to log in.");
                let cookie_store =
                    try_login::<T>(&client, &credentials_path, &cookie_store_path, player_name)
                        .await?;
                let client = Self {
                    client,
                    credentials_path,
                    cookie_store,
                    cookie_store_path,
                    _phantom: PhantomData,
                };
                (client, Err(None))
            }
            Err(e) => return Err(anyhow::Error::from(e)),
        };
        let index = match index {
            Ok(index) => index, // TODO: a bit redundant
            Err(err) => {
                if let Some(err) = err {
                    info!("The stored session seems to be expired.  Trying to log in.");
                    info!("    Detail: {:?}", err);
                }
                client.cookie_store = try_login::<T>(
                    &client.client,
                    &client.credentials_path,
                    &client.cookie_store_path,
                    player_name,
                )
                .await?;
                // return Ok(());
                client.download_record_index().await?
            }
        };
        info!("Successfully logged in.");

        Ok((client, index))
    }

    async fn download_record_index(&mut self) -> anyhow::Result<Vec<(PlayTime<T>, Idx<T>)>> {
        let url = T::RECORD_URL;
        let response = self.fetch_authenticated(url).await?.0;
        let document = Html::parse_document(&response.text().await?);
        T::parse_record_index(&document)
    }

    pub async fn download_record(&mut self, idx: Idx<T>) -> anyhow::Result<Option<T::PlayRecord>>
    where
        Idx<T>: Copy,
    {
        let url = T::play_log_detail_url(idx);
        let (response, redirect_url) = self.fetch_authenticated(&url).await?;
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

    pub async fn fetch_authenticated(
        &mut self,
        url: impl IntoUrl,
    ) -> anyhow::Result<(reqwest::Response, Option<Url>)> {
        self.request_authenticated(|client| Ok(client.get(url)), "")
            .await
    }

    pub async fn request_authenticated(
        &mut self,
        request_builder: impl FnOnce(&reqwest::Client) -> anyhow::Result<reqwest::RequestBuilder>,
        additional_cookie: &str,
    ) -> anyhow::Result<(reqwest::Response, Option<Url>)> {
        let response = request_builder(&self.client)?
            .header(
                header::COOKIE,
                format!("userId={}{}", self.cookie_store.user_id, additional_cookie),
            )
            .send()
            .await?;
        set_and_save_credentials(&mut self.cookie_store, &self.cookie_store_path, &response)?;
        // HACK: see comments in fn reqwest_client().
        // Once the issue is resolved, "is_redirection" clause should be removed.
        // We do not know what is the side effect on other operations by this addition.
        if !(response.status().is_success() || response.status().is_redirection()) {
            bail!(
                "Unexpected error code: server returned {:?}",
                response.status()
            );
        }
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|x| Url::parse(x.to_str().ok()?).ok());
        Ok((response, location))
    }

    pub fn reqwest(&self) -> &reqwest::Client {
        &self.client
    }
}

fn reqwest_client<T: SegaTrait>() -> reqwest::Result<reqwest::Client> {
    // let jar = Arc::new(Jar::default());
    reqwest::Client::builder()
        .cookie_store(true)
        // .cookie_provider(jar.clone())
        .connection_verbose(true)
        .redirect(redirect::Policy::custom(|attempt| {
            #[allow(clippy::if_same_then_else)]
            if attempt.url().path() == T::ERROR_PATH {
                attempt.error(anyhow!("Redirected to error page"))
            } else if attempt.url().path() == "/maimai-mobile/home/userOption/favorite/musicList" {
                // HACK: on redirect, the header may be replaced by the contents on the cookie store.
                // While we set `userId` cookie by manually editing the header,
                // this behavior may overwrite and remove the header that we set,
                // causing a redirect error.
                // Here, we intentionally stop redirecting on specific path
                // so that this will never happen.
                // In future, we must exploit the cookie jar (that is commented out)
                // so that we can extract the cookie,
                // or implement a custom cookie store that is capable to do so.
                attempt.stop()
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

pub fn set_and_save_credentials(
    cookie_store: &mut CookieStore,
    cookie_store_path: &Path,
    response: &reqwest::Response,
) -> anyhow::Result<bool> {
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        cookie_store.user_id = cookie.value().to_owned().into();
        cookie_store.save(cookie_store_path)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Debug, Serialize)]
struct LoginForm<'a, T> {
    #[serde(rename = "segaId")]
    sega_id: &'a SegaId,
    password: &'a Password,
    save_cookie: &'static str,
    token: &'a str,
    #[serde(skip)]
    _phantom: PhantomData<fn() -> T>,
}
impl<'a, T> LoginForm<'a, T> {
    fn new(credentials: &'a Credentials, token: &'a str) -> Self {
        Self {
            sega_id: &credentials.sega_id,
            password: &credentials.password,
            save_cookie: "on",
            token,
            _phantom: Default::default(),
        }
    }
}

async fn try_login<T: SegaTrait>(
    client: &reqwest::Client,
    credentials_path: &Path,
    cookie_store_path: &Path,
    player_name: Option<&PlayerName>,
) -> anyhow::Result<CookieStore> {
    let credentials = Credentials::load(credentials_path)?;

    let token = get_token::<T>(client).await?;

    let login_url = T::LOGIN_URL;
    let response = client
        .post(login_url)
        .form(&LoginForm::<T>::new(&credentials, &token))
        .send()
        .await?;

    if !response.status().is_success() {
        bail!("Failed to log in: server returned {:?}", response.status());
    }

    let url = response.url().clone();
    if url.as_str() != T::AIME_LIST_URL {
        bail!("Error: redirected to unexpected url while logging in: {url}",);
    }

    let aime_idx = match player_name {
        None => credentials.aime_idx.unwrap_or_default(),
        Some(player_name) => dbg!(T::parse_aime_selection_page(&Html::parse_document(
            &response.text().await?
        ))?)
        .into_iter()
        .find_map(|(aime_idx, name)| (player_name == &name).then_some(aime_idx))
        .with_context(|| {
            "No user with player name {player_name:?} was found in the aime selection page"
        })?,
    };
    let url = T::select_aime_list_url(aime_idx);
    let response = client.get(&url).send().await?;

    let mut cookie_store = CookieStore::default();
    if !set_and_save_credentials(&mut cookie_store, cookie_store_path, &response)? {
        bail!("Desired cookie was not found.");
    }
    Ok(cookie_store)
}

async fn get_token<T: SegaTrait>(client: &reqwest::Client) -> Result<String, anyhow::Error> {
    let login_form = client.get(T::LOGIN_FORM_URL).send().await?;
    let login_form = Html::parse_document(&login_form.text().await?);
    let token = login_form
        .select(T::login_form_token_selector())
        .next()
        .ok_or_else(|| anyhow!("The token was not found in the login form."))?
        .value()
        .attr("value")
        .ok_or_else(|| anyhow!("Value was not present in the token element."))?
        .to_owned();
    Ok(token)
}

#[cfg(test)]
mod tests {
    use crate::{cookie_store::Credentials, maimai::Maimai};

    use super::LoginForm;

    #[test]
    fn test_login_form() {
        let sega_id = "abc".to_owned().into();
        let password = "def".to_owned().into();
        let credentials = Credentials::builder()
            .sega_id(sega_id)
            .password(password)
            .aime_idx(None)
            .build();
        let form = LoginForm::<Maimai>::new(&credentials, "ghi");
        let json = serde_json::to_string(&form).unwrap();
        assert_eq!(
            json,
            r#"{"segaId":"abc","password":"def","save_cookie":"on","token":"ghi"}"#
        );
    }
}

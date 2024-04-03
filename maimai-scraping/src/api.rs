use std::borrow::Cow;
use std::marker::PhantomData;
use std::path::Path;

use crate::cookie_store::AimeIdx;
use crate::cookie_store::CookieStore;
use crate::cookie_store::CookieStoreLoadError;
use crate::cookie_store::Credentials;
use crate::cookie_store::Password;
use crate::cookie_store::PlayerName;
use crate::cookie_store::SegaId;
use crate::cookie_store::UserIdentifier;
use crate::maimai::MaimaiIntl;
use crate::sega_trait::Idx;
use crate::sega_trait::PlayTime;
use crate::sega_trait::SegaJapaneseAuth;
use crate::sega_trait::SegaTrait;
use anyhow::anyhow;
use anyhow::bail;
use itertools::Itertools;
use log::debug;
use log::info;
use reqwest::header;
use reqwest::redirect;
use reqwest::IntoUrl;
use reqwest::Url;
use scraper::Html;
use serde::Serialize;

pub struct SegaClientInitializer<'p, 'q> {
    pub credentials_path: &'p Path,
    pub cookie_store_path: &'p Path,
    pub user_identifier: &'q UserIdentifier,
}

pub struct SegaClient<'p, T: SegaTrait> {
    client: reqwest::Client,
    // credentials_path: Cow<'p, Path>,
    cookie_store: CookieStore,
    cookie_store_path: Cow<'p, Path>,
    _phantom: PhantomData<T>,
}

impl<'p, T: SegaTrait> SegaClient<'p, T> {
    pub async fn new_with_default_path(
        user_identifier: &UserIdentifier,
    ) -> anyhow::Result<(SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>)>
    where
        T: SegaJapaneseAuth,
    {
        Self::new(SegaClientInitializer {
            credentials_path: Path::new(T::CREDENTIALS_PATH),
            cookie_store_path: Path::new(T::COOKIE_STORE_PATH),
            user_identifier,
        })
        .await
    }

    pub async fn new(
        args: SegaClientInitializer<'p, '_>,
    ) -> anyhow::Result<(SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>)>
    where
        T: SegaJapaneseAuth,
    {
        let credentials = Credentials::load(args.credentials_path)?;
        let cookie_store_path = Cow::Borrowed(args.cookie_store_path);
        let cookie_store = CookieStore::load(cookie_store_path.as_ref());

        let client = reqwest_client::<T>(Some(T::AIME_SUBMIT_PATH))?;
        let make_client = |cookie_store| Self {
            client,
            // credentials_path,
            cookie_store,
            cookie_store_path,
            _phantom: PhantomData,
        };

        // Try to log in
        let mut client = match cookie_store {
            Ok(cookie_store) => {
                // Why can't we directly access AIME_LIST_URL to determine log-in state?
                // This is because, even if the cookie is implicitly(*) expired,
                // we can still access AIME_LIST_URL.
                // However, unlike normal situation, the request trying to select Aime
                // does not return new `userId` cookie,
                // resulting in a wired error, where the cookie is not expired by this operation.
                // (*) Implicit expiration includes logging in from another account or timeout,
                // but as already mentioned, the wired error does not seem to count.
                info!("Cookie store was found.  Trying to use this cookie.");
                let mut client = make_client(cookie_store);
                // Check if the cookie is valid and ...
                if let Ok((response, redirect)) =
                    client.fetch_authenticated(T::FRIEND_CODE_URL).await
                {
                    // if friend code is specified, then we can determine if this is the correct account for sure.
                    if let Some(expected_friend_code) = args.user_identifier.friend_code.as_ref() {
                        if redirect.is_none() {
                            let friend_code = T::parse_friend_code_page(&Html::parse_document(
                                &response.text().await?,
                            ))?;
                            debug!("Expected {expected_friend_code:?}, found {friend_code:?}");
                            if &friend_code == expected_friend_code {
                                let res = client.download_record_index().await?;
                                return Ok((client, res));
                            }
                        } else {
                            info!("Redirect occurred, so the session has expired.")
                        }
                    }
                }
                // In other cases, we try to log in from scratch.
                // Although there's a bit chance of improvement by skipping logging in here,
                // but it's normal to start over when switching Aime, so we just don't care.
                client
            }
            Err(CookieStoreLoadError::NotFound) => {
                info!("Cookie store was not found.");
                make_client(Default::default())
            }
            Err(e) => return Err(e.into()),
        };
        let aime_list = client.try_login(&credentials).await?;
        info!("Successfully logged in.");
        debug!("Available Aimes: {aime_list:?}");

        // Determine which Aime to use
        let candidates = aime_list
            .into_iter()
            .filter_map(|(aime_idx, player_name)| {
                args.user_identifier
                    .player_name
                    .as_ref()
                    .map_or(true, |expected| &player_name == expected)
                    .then_some(aime_idx)
            })
            .collect_vec();
        if candidates.len() != 1 {
            bail!(
                "The Aime matching {:?} cannot be uniquely determined",
                args.user_identifier
            );
        }
        let aime_idx = candidates[0];

        // Select Aime
        let url = T::select_aime_list_url(aime_idx);
        // let (_, location) = client.fetch_authenticated(&url).await?;
        let response = client.client.get(&url).send().await?;
        let location = Self::get_location(&response);
        if !location.as_ref().is_some_and(|x| x.as_str() == T::HOME_URL) {
            bail!(
                "Redirected to unexpected url: {:?}",
                location.as_ref().map(|x| x.as_str())
            );
        }
        // Save the current cookie (cookie is always renewed after redirecting to home)
        if !set_and_save_credentials(
            &mut client.cookie_store,
            &client.cookie_store_path,
            &response,
        )? {
            bail!("Desired cookie was not found.");
        }

        // Make sure that we are in the correct account.
        if let Some(expected_friend_code) = args.user_identifier.friend_code.as_ref() {
            let (response, _) = client.fetch_authenticated(T::FRIEND_CODE_URL).await?;
            let friend_code =
                T::parse_friend_code_page(&Html::parse_document(&response.text().await?))?;
            if &friend_code != expected_friend_code {
                bail!("Friend code does not match: expected {expected_friend_code:?}, found {friend_code:?}")
            }
        }
        info!("Successfully chose Aime.");

        let res = client.download_record_index().await?;
        Ok((client, res))
    }

    async fn try_login(
        &mut self,
        credentials: &Credentials,
    ) -> anyhow::Result<Vec<(AimeIdx, PlayerName)>>
    where
        T: SegaJapaneseAuth,
    {
        info!("Trying to log in.");
        let token = get_token::<T>(&self.client).await?;

        // Submit login form
        let login_url = T::LOGIN_URL;
        let response = self
            .client
            .post(login_url)
            .form(&LoginForm::<T>::new(credentials, &token))
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("Failed to log in: server returned {:?}", response.status());
        }

        // Make sure that it redirects to aime list
        let url = response.url().clone();
        if url.as_str() != T::AIME_LIST_URL {
            bail!("Error: redirected to unexpected url while logging in: {url}",);
        }

        T::parse_aime_selection_page(&Html::parse_document(&response.text().await?))
    }

    async fn download_record_index(&mut self) -> anyhow::Result<Vec<(PlayTime<T>, Idx<T>)>> {
        let url = T::RECORD_URL;
        let response = self.fetch_authenticated(url).await?.0;
        let document = Html::parse_document(&response.text().await?);
        let res = T::parse_record_index(&document)?;
        debug!("Records: {:?}", res.iter().map(|x| &x.1).collect_vec());
        Ok(res)
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
        let location = Self::get_location(&response);
        Ok((response, location))
    }

    fn get_location(response: &reqwest::Response) -> Option<Url> {
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|x| Url::parse(x.to_str().ok()?).ok())
    }

    pub fn reqwest(&self) -> &reqwest::Client {
        &self.client
    }
}

impl<'p> SegaClient<'p, MaimaiIntl> {
    pub async fn new_maimai_intl(
        args: SegaClientInitializer<'p, '_>,
    ) -> anyhow::Result<(Self, Vec<(PlayTime<MaimaiIntl>, Idx<MaimaiIntl>)>)> {
        // TODO: duplicate code, can be refactored!
        let credentials = Credentials::load(args.credentials_path)?;
        let cookie_store_path = Cow::Borrowed(args.cookie_store_path);
        let cookie_store = CookieStore::load(cookie_store_path.as_ref());

        let client = reqwest_client::<MaimaiIntl>(None)?;
        let make_client = |cookie_store| Self {
            client,
            // credentials_path,
            cookie_store,
            cookie_store_path,
            _phantom: PhantomData,
        };

        // Try to log in
        let mut client = match cookie_store {
            Ok(cookie_store) => {
                info!("Cookie store was found.  Trying to use this cookie.");
                let mut client = make_client(cookie_store);
                if let Ok(res) = client.download_record_index().await {
                    return Ok((client, res));
                }
                client
            }
            Err(CookieStoreLoadError::NotFound) => {
                info!("Cookie store was not found.");
                make_client(Default::default())
            }
            Err(e) => return Err(e.into()),
        };

        // We just want the cookie (JSESSIONID).  Actual HTML does not matter.
        let url = "https://lng-tgk-aime-gw.am-all.net/common_auth/login?site_id=maimaidxex&redirect_url=https://maimaidx-eng.com/maimai-mobile/&back_url=https://maimai.sega.com/";
        let _ = client
            .request_authenticated(|client| Ok(client.get(url)), "")
            .await?;

        #[derive(Debug, Serialize)]
        struct LoginForm<'a> {
            #[serde(rename = "sid")]
            sega_id: &'a SegaId,
            password: &'a Password,
            retention: u8,
        }
        let response = client
            .reqwest()
            .post("https://lng-tgk-aime-gw.am-all.net/common_auth/login/sid/")
            .form(&LoginForm {
                sega_id: &credentials.sega_id,
                password: &credentials.password,
                retention: 1,
            })
            .send()
            .await?;
        set_and_save_credentials(
            &mut client.cookie_store,
            &client.cookie_store_path,
            &response,
        )?;

        let res = client.download_record_index().await?;
        Ok((client, res))
    }
}

fn reqwest_client<T: SegaTrait>(
    aime_submit_path: Option<&'static str>,
) -> reqwest::Result<reqwest::Client> {
    // let jar = Arc::new(Jar::default());
    reqwest::Client::builder()
        .cookie_store(true)
        // .cookie_provider(jar.clone())
        .connection_verbose(true)
        .redirect(redirect::Policy::custom(move |attempt| {
            #[allow(clippy::if_same_then_else)]
            if attempt.url().path() == T::ERROR_PATH {
                return attempt.error(anyhow!("Redirected to error page"));
            }
            if attempt.url().path() == "/maimai-mobile/home/userOption/favorite/musicList" {
                // HACK: on redirect, the header may be replaced by the contents on the cookie store.
                // While we set `userId` cookie by manually editing the header,
                // this behavior may overwrite and remove the header that we set,
                // causing a redirect error.
                // Here, we intentionally stop redirecting on specific path
                // so that this will never happen.
                // In future, we must exploit the cookie jar (that is commented out)
                // so that we can extract the cookie,
                // or implement a custom cookie store that is capable to do so.
                return attempt.stop();
            }
            if attempt.url().as_str() == "https://maimaidx-eng.com/maimai-mobile/home/" {
                // HACK:
                // Redirect is intentionally stopped to capture `Set-Cookie` header for `userId`.
                // Side effect is unknown, and it is better to implement a custom cookie store.
                // Moreover, redirect occurs even when it fails to log in.
                // This means that redirecting to this URL does not necessarily imply success.
                return attempt.stop();
            }
            if let (Some(last), Some(aime_submit_path)) =
                (attempt.previous().last(), aime_submit_path)
            {
                if last.path() == aime_submit_path {
                    // TODO: Why do we have to stop here?
                    return attempt.stop();
                }
            }
            attempt.follow()
        }))
        .build()
}

pub fn set_and_save_credentials(
    cookie_store: &mut CookieStore,
    cookie_store_path: &Path,
    response: &reqwest::Response,
) -> anyhow::Result<bool> {
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        debug!("Stored `userId`: {:?}", cookie.value());
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

async fn get_token<T: SegaJapaneseAuth>(client: &reqwest::Client) -> Result<String, anyhow::Error> {
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

use std::borrow::Cow;
use std::future::Future;
use std::marker::PhantomData;
use std::path::Path;

use crate::cookie_store::CookieStore;
use crate::cookie_store::CookieStoreLoadError;
use crate::cookie_store::PlayerName;
use crate::cookie_store::UserIdentifier;
use crate::maimai::MaimaiIntl;
use crate::sega_trait::AimeEntry;
use crate::sega_trait::Idx;
use crate::sega_trait::PlayTime;
use crate::sega_trait::SegaJapaneseAuth;
use crate::sega_trait::SegaTrait;
use anyhow::anyhow;
use anyhow::bail;
use itertools::Itertools;
use log::debug;
use log::info;
use log::warn;
use maimai_scraping_utils::fs_json_util::read_json;
use maimai_scraping_utils::sega_id::Credentials;
use maimai_scraping_utils::sega_id::Password;
use maimai_scraping_utils::sega_id::SegaId;
use reqwest::header;
use reqwest::redirect;
use reqwest::IntoUrl;
use reqwest::Url;
use scraper::Html;
use serde::Serialize;

#[derive(Clone, Copy)]
pub struct SegaClientInitializer<'p, 'q, T: SegaTrait> {
    pub credentials_path: &'p Path,
    pub cookie_store_path: &'p Path,
    pub user_identifier: &'q UserIdentifier,
    pub force_paid: T::ForcePaidFlag,
}

pub struct SegaClient<'p, T: SegaTrait> {
    client: reqwest::Client,
    // credentials_path: Cow<'p, Path>,
    cookie_store: CookieStore,
    cookie_store_path: Cow<'p, Path>,
    _phantom: PhantomData<T>,
}

pub type SegaClientAndRecordList<'p, T> = (SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>);

impl<'p, T: SegaTrait> SegaClient<'p, T> {
    pub async fn new_with_default_path(
        user_identifier: &UserIdentifier,
        force_paid: bool,
    ) -> anyhow::Result<(SegaClient<'p, T>, Vec<(PlayTime<T>, Idx<T>)>)>
    where
        T: SegaJapaneseAuth,
        T: SegaTrait<ForcePaidFlag = bool>,
    {
        Self::new(SegaClientInitializer {
            credentials_path: Path::new(T::CREDENTIALS_PATH),
            cookie_store_path: Path::new(T::COOKIE_STORE_PATH),
            user_identifier,
            force_paid,
        })
        .await
    }

    pub async fn new(
        args: SegaClientInitializer<'p, '_, T>,
    ) -> anyhow::Result<SegaClientAndRecordList<'p, T>>
    where
        T: SegaJapaneseAuth,
        T: SegaTrait<ForcePaidFlag = bool>,
    {
        let mut client =
            match Self::make_client(&args, Some(T::AIME_SUBMIT_PATH), |mut client| async {
                // Why can't we directly access AIME_LIST_URL to determine log-in state?
                // This is because, even if the cookie is implicitly(*) expired,
                // we can still access AIME_LIST_URL.
                // However, unlike normal situation, the request trying to select Aime
                // does not return new `userId` cookie,
                // resulting in a wired error, where the cookie is not expired by this operation.
                // (*) Implicit expiration includes logging in from another account or timeout,
                // but as already mentioned, the wired error does not seem to count.
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
                                return Ok(Ok((client, res)));
                            }
                        } else {
                            info!("Redirect occurred, so the session has expired.")
                        }
                    } else {
                        info!("We cannot be sure if we are in the correct account.  Logging in from scratch.")
                    }
                }
                // In other cases, we try to log in from scratch.
                // Although there's a bit chance of improvement by skipping logging in here,
                // but it's normal to start over when switching Aime, so we just don't care.
                Ok(Err(client))
            })
            .await?
            {
                Ok(res) => return Ok(res),
                Err(client) => client,
            };

        let credentials = read_json(args.credentials_path)?;
        let aime_list = client.try_login(&credentials).await?;
        info!("Successfully logged in.");
        debug!("Available Aimes: {aime_list:?}");

        // Determine which Aime to use
        let aime_entry = find_aime_idx(&aime_list, args.user_identifier.player_name.as_ref())?;

        if args.force_paid && !aime_entry.paid {
            info!("This account is not paid!  Switching to paid account.");
            if !aime_list.iter().any(|x| x.paid) {
                warn!("No paid aime was found in the retrieved aime list!  The following operations is likely to fail.");
            }
            let url = T::switch_to_paid_url(aime_entry.idx);
            let response = client.client.get(&url).send().await?;
            let form = T::parse_paid_confirmation(&Html::parse_document(&response.text().await?))?;
            let url = T::SWITCH_PAID_CONFIRMATION_URL;
            let response = client.client.post(url).form(&form).send().await?;
            if response.url().as_str() != T::AIME_LIST_URL {
                bail!("Error: redirected to unexpected url while logging in: {url}");
            }
            let aime_list =
                T::parse_aime_selection_page(&Html::parse_document(&response.text().await?))?;
            if !aime_list.iter().any(|x| x.idx == aime_entry.idx && x.paid) {
                bail!("Failed to switch to paid")
            }
            info!("Successfully switched to paid account.")
        }

        // Select Aime
        let url = T::select_aime_list_url(aime_entry.idx);
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

    async fn make_client<U, UFut, R>(
        args: &SegaClientInitializer<'p, '_, T>,
        aime_submit_path: Option<&'static str>,
        runner: R,
    ) -> anyhow::Result<Result<(Self, U), Self>>
    where
        R: FnOnce(Self) -> UFut,
        // Error returned by runner is immediately thrown
        UFut: Future<Output = anyhow::Result<Result<(Self, U), Self>>>,
    {
        let cookie_store_path = Cow::Borrowed(args.cookie_store_path);
        let cookie_store = CookieStore::load(cookie_store_path.as_ref());

        let client = reqwest_client::<T>(aime_submit_path)?;
        let make_client = |cookie_store| Self {
            client,
            // credentials_path,
            cookie_store,
            cookie_store_path,
            _phantom: PhantomData,
        };

        // Try to log in
        match cookie_store {
            Ok(cookie_store) => {
                info!("Cookie store was found.  Trying to use this cookie.");
                Ok(runner(make_client(cookie_store)).await?)
            }
            Err(CookieStoreLoadError::NotFound) => {
                info!("Cookie store was not found.");
                Ok(Err(make_client(Default::default())))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn try_login(&mut self, credentials: &Credentials) -> anyhow::Result<Vec<AimeEntry>>
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
        args: SegaClientInitializer<'p, '_, MaimaiIntl>,
    ) -> anyhow::Result<SegaClientAndRecordList<'p, MaimaiIntl>> {
        if args.user_identifier.friend_code.is_some() || args.user_identifier.player_name.is_some()
        {
            bail!("Maimai international does not support multi user");
        }

        let mut client = match Self::make_client(&args, None, |mut client| async {
            match client.download_record_index().await {
                Ok(res) => Ok(Ok((client, res))),
                Err(_) => Ok(Err(client)),
            }
        })
        .await?
        {
            Ok(res) => return Ok(res),
            Err(client) => client,
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
        let credentials: Credentials = read_json(args.credentials_path)?;
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

pub fn find_aime_idx<'p>(
    aime_list: &[AimeEntry],
    player_name: impl Into<Option<&'p PlayerName>>,
) -> anyhow::Result<&AimeEntry> {
    let expected = player_name.into();
    match aime_list
        .iter()
        .filter(|entry| expected.is_none_or(|expected| &entry.player_name == expected))
        .collect_vec()[..]
    {
        [aime] => Ok(aime),
        _ => bail!("The Aime with player name {expected:?} cannot be uniquely determined"),
    }
}

#[cfg(test)]
mod tests {
    use crate::maimai::Maimai;
    use maimai_scraping_utils::sega_id::Credentials;

    use super::LoginForm;

    #[test]
    fn test_login_form() {
        let sega_id = "abc".to_owned().into();
        let password = "def".to_owned().into();
        let credentials = Credentials::builder()
            .sega_id(sega_id)
            .password(password)
            .build();
        let form = LoginForm::<Maimai>::new(&credentials, "ghi");
        let json = serde_json::to_string(&form).unwrap();
        assert_eq!(
            json,
            r#"{"segaId":"abc","password":"def","save_cookie":"on","token":"ghi"}"#
        );
    }
}

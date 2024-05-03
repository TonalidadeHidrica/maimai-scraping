use std::{
    io::{BufReader, BufWriter, ErrorKind},
    marker::PhantomData,
    path::PathBuf,
    sync::Arc,
};

use anyhow::{anyhow, bail};
use log::{error, info, warn};
use maimai_scraping_utils::sega_id::{Credentials, Password, SegaId};
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};
use scraper::Html;
use serde::Serialize;

use crate::parser::{parse_aime_index, AimeIndex};

pub struct MayNotBeLoggedIn;
pub struct LoggedIn;

pub struct AimeApi<T> {
    _phantom: PhantomData<fn() -> T>,
    cookie_store_path: PathBuf,
    cookie_store: Arc<CookieStoreMutex>,
    reqwest: reqwest::Client,
}

impl AimeApi<MayNotBeLoggedIn> {
    pub fn new(cookie_store_path: PathBuf) -> anyhow::Result<Self> {
        let cookie_store = match fs_err::File::open(&cookie_store_path) {
            Ok(file) => CookieStore::load_json(BufReader::new(file))
                .map_err(|e| anyhow!("Failed to load cookie store: {e:#}"))?,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                warn!("Cookie store was not found at {cookie_store_path:?}.  Creating a new one.");
                CookieStore::new(None)
            }
            Err(e) => Err(e)?,
        };
        let cookie_store = Arc::new(CookieStoreMutex::new(cookie_store));
        let reqwest = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .cookie_provider(Arc::clone(&cookie_store))
            .connection_verbose(true)
            .build()?;
        Ok(Self {
            _phantom: PhantomData,
            cookie_store_path,
            cookie_store,
            reqwest,
        })
    }

    pub async fn login(
        self,
        credentials: &Credentials,
    ) -> anyhow::Result<(AimeApi<LoggedIn>, AimeIndex)> {
        // Using this to avoid Format issue
        macro_rules! login_url {
            () => {
                "https://tgk-aime-gw.sega.jp/common_auth/login?site_id=aimess&redirect_url=https%3A%2F%2Fmy-aime.net%2Flogin%2Fauth%2Fcauth"
            }
        }

        let client = AimeApi {
            _phantom: PhantomData,
            cookie_store_path: self.cookie_store_path,
            cookie_store: self.cookie_store,
            reqwest: self.reqwest,
        };

        // First attempt
        let response = client
            .reqwest
            .get("https://my-aime.net/login")
            .send()
            .await?;
        client.save_cookie();
        match response.url().as_str() {
            "https://my-aime.net/" => {
                info!("Already logged in.");
                let html = Html::parse_document(&response.text().await?);
                return Ok((client, parse_aime_index(&html)?));
            }
            login_url!() => {
                info!("Not logged in yet.  Trying to log in.");
                // Go on outside match block
            }
            url => bail!("Redirected to unexpected url: {url}"),
        };

        // Log in.
        // If successful, it redirects to the top page with desired aime data.
        #[derive(Debug, Serialize)]
        struct LoginForm<'a> {
            #[serde(rename = "sid")]
            sega_id: &'a SegaId,
            password: &'a Password,
            retention: u8,
        }
        let response = client
            .reqwest
            .post("https://tgk-aime-gw.sega.jp/common_auth/login/sid/")
            .form(&LoginForm {
                sega_id: &credentials.sega_id,
                password: &credentials.password,
                retention: 1,
            })
            .send()
            .await?;
        client.save_cookie();
        match response.url().as_str() {
            "https://my-aime.net/" => {
                info!("Successfully logged in.");
                let html = Html::parse_document(&response.text().await?);
                Ok((client, parse_aime_index(&html)?))
            }
            url => bail!("Redirected to unexpected url: {url}"),
        }
    }
}

impl AimeApi<LoggedIn> {}

impl<T> AimeApi<T> {
    fn save_cookie(&self) {
        let run = || {
            self.cookie_store
                .lock()
                .expect("Cookie store was poisoned")
                .save_json(&mut BufWriter::new(fs_err::File::create(
                    &self.cookie_store_path,
                )?))
                .map_err(|e| anyhow!("{e:#}"))?;
            anyhow::Ok(())
        };
        if let Err(e) = run() {
            error!("Failed to save cookie: {e:#}")
        }
    }
}

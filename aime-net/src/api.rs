use std::{
    io::{BufReader, BufWriter, ErrorKind},
    path::PathBuf,
    sync::Arc,
};

use anyhow::anyhow;
use log::{error, warn};
use maimai_scraping_utils::sega_id::Credentials;
use reqwest_cookie_store::{CookieStore, CookieStoreMutex};

pub struct AimeApi {
    cookie_store_path: PathBuf,
    cookie_store: Arc<CookieStoreMutex>,
    reqwest: reqwest::Client,
}

impl AimeApi {
    pub fn login(cookie_store_path: PathBuf, credentials: &Credentials) -> anyhow::Result<Self> {
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
            .cookie_provider(Arc::clone(&cookie_store))
            .build()?;
        let mut client = Self {
            cookie_store_path,
            cookie_store,
            reqwest,
        };
        Ok(client)
    }

    fn save_cookie(&mut self) -> anyhow::Result<()> {
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
        Ok(())
    }
}

use std::io::{self, BufReader, BufWriter};

use fs_err::File;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CookieStore {
    pub user_id: UserId,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub user_name: UserName,
    pub password: Password,
}

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserId(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserName(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct Password(String);

const COOKIE_STORE_PATH: &str = "./ignore/cookie_store.json";

impl CookieStore {
    pub fn load() -> Result<CookieStore, CookieStoreLoadError> {
        Ok(serde_json::from_reader(BufReader::new(File::open(
            COOKIE_STORE_PATH,
        )?))?)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let writer = BufWriter::new(File::create(COOKIE_STORE_PATH)?);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CookieStoreLoadError {
    #[error("Cookie store was not found.")]
    NotFound,
    #[error("An I/O error occurred when loading the cookie store: {0:?}")]
    IOError(io::Error),
    #[error("The cookie store json file is corrupted and could not be loaded: {0:?}")]
    JsonError(#[from] serde_json::Error),
}
impl From<io::Error> for CookieStoreLoadError {
    fn from(e: io::Error) -> Self {
        match e.kind() {
            io::ErrorKind::NotFound => Self::NotFound,
            _ => Self::IOError(e),
        }
    }
}

const CREDENTIALS_PATH: &str = "./ignore/credentials.json";

impl Credentials {
    pub fn load() -> anyhow::Result<Credentials> {
        Ok(serde_json::from_reader(BufReader::new(File::open(
            CREDENTIALS_PATH,
        )?))?)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let writer = BufWriter::new(File::create(CREDENTIALS_PATH)?);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }
}

use std::{
    io::{self, BufReader, BufWriter},
    marker::PhantomData,
};

use fs_err::File;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::sega_trait::SegaTrait;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct CookieStore {
    pub user_id: UserId,
}

#[derive(Debug, TypedBuilder, Serialize, Deserialize)]
pub struct Credentials<T> {
    pub user_name: UserName,
    pub password: Password,
    pub aime_idx: Option<AimeIdx>,
    #[serde(skip)]
    #[builder(default)]
    _phantom: PhantomData<fn() -> T>,
}

#[derive(Default, Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserId(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserName(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct Password(String);

#[derive(
    Clone, Copy, Default, Debug, derive_more::From, derive_more::Display, Serialize, Deserialize,
)]
pub struct AimeIdx(u8);

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

impl<T: SegaTrait> Credentials<T> {
    pub fn load() -> anyhow::Result<Self> {
        Ok(serde_json::from_reader(BufReader::new(File::open(
            T::CREDENTIALS_PATH,
        )?))?)
    }

    pub fn save(&self) -> std::io::Result<()> {
        let writer = BufWriter::new(File::create(T::CREDENTIALS_PATH)?);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }
}

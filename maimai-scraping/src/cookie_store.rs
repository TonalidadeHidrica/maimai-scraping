use std::{
    fmt::Debug,
    io::{self, BufReader, BufWriter},
    path::PathBuf,
};

use derive_more::{AsRef, Display, From};
use fs_err::File;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

#[derive(Default, Serialize, Deserialize)]
pub struct CookieStore {
    pub user_id: UserIdCookie,
}

/// Represents the value of `user_id` cookie, which is used to track user's identity
#[derive(Default, Debug, From, Display, Serialize, Deserialize)]
pub struct UserIdCookie(String);

impl Debug for CookieStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CookieStore")
            .field("user_id", &self.user_id)
            .finish()
    }
}

#[derive(Clone, Debug, TypedBuilder, Serialize, Deserialize, clap::Args)]
pub struct UserIdentifier {
    #[arg(long)]
    pub friend_code: Option<FriendCode>,
    #[arg(long)]
    pub player_name: Option<PlayerName>,
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
pub struct FriendCode(String);

#[derive(Clone, PartialEq, Eq, Debug, From, Display, Serialize, Deserialize)]
pub struct PlayerName(String);

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, From, Display, Serialize, Deserialize)]
pub struct AimeIdx(u8);

impl CookieStore {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, CookieStoreLoadError> {
        Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn save(&self, path: impl Into<PathBuf>) -> std::io::Result<()> {
        let writer = BufWriter::new(File::create(path)?);
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

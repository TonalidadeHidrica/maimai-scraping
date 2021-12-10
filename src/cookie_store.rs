use std::io::{BufReader, BufWriter};

use fs_err::File;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CredentialStore<UserId = UserIdCookie> {
    pub user_id: UserId,
    pub user_name: Option<UserName>,
    pub password: Option<Password>,
}

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserIdCookie(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct UserName(String);

#[derive(Debug, derive_more::From, derive_more::Display, Serialize, Deserialize)]
pub struct Password(String);

const COOKIE_STORE_PATH: &str = "./ignore/credentials.json";

pub type MaybeCredential = CredentialStore<Option<UserIdCookie>>;

impl CredentialStore {
    pub fn load() -> anyhow::Result<MaybeCredential> {
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

impl MaybeCredential {
    pub fn transpose(self) -> Result<CredentialStore, Self> {
        match self.user_id {
            Some(user_id) => Ok(CredentialStore {
                user_id,
                user_name: self.user_name,
                password: self.password,
            }),
            None => Err(self),
        }
    }
}

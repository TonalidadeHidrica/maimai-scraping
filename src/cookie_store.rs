use std::io::{BufReader, BufWriter};

use fs_err::File;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct CookieStore {
    pub user_id: String,
}

const COOKIE_STORE_PATH: &str = "./ignore/cookies.json";

impl CookieStore {
    pub fn load() -> anyhow::Result<Self> {
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

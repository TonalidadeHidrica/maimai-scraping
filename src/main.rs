use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
};

use reqwest::header;
use scraper::Html;
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cookie_store = CookieStore::load()?;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .connection_verbose(true)
        .build()?;
    let response = client
        .get("https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx=0")
        .header(header::COOKIE, format!("userId={}", cookie_store.user_id))
        .send()
        .await?;
    if let Some(cookie) = response.cookies().find(|x| x.name() == "userId") {
        cookie_store.user_id = cookie.value().to_owned();
        cookie_store.save()?;
    }
    let document = Html::parse_document(&response.text().await?);

    BufWriter::new(File::create("ignore/test.html")?)
        .write_all(document.root_element().html().as_bytes())?;

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct CookieStore {
    user_id: String,
}

const COOKIE_STORE_PATH: &'static str = "./ignore/cookies.json";

impl CookieStore {
    fn load() -> anyhow::Result<Self> {
        Ok(serde_json::from_reader(BufReader::new(File::open(
            COOKIE_STORE_PATH,
        )?))?)
    }

    fn save(&self) -> std::io::Result<()> {
        let writer = BufWriter::new(File::create(COOKIE_STORE_PATH)?);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }
}

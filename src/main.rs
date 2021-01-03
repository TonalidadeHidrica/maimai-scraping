use std::{fs::File, io::BufReader};

use once_cell::sync::Lazy;
use reqwest::header;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cookie_store = CookieStore::load()?;

    let client = reqwest::Client::builder()
        .cookie_store(true)
        .connection_verbose(true)
        .build()?;
    let response = client
        .get("https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx=0")
        .header(header::COOKIE, format!("userId={}", cookie_store.user_id))
        .send()
        .await?;
    println!("{:?}", response.cookies().find(|x| x.name() == "userId"));
    let document = Html::parse_document(&response.text().await?);
    static CELL: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".playlog_achievement_txt").unwrap());
    println!(
        "{:?}",
        document
            .select(&*CELL)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join(""))
    );

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
}

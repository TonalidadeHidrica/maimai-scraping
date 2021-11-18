use std::collections::BTreeMap;
use std::io;
use std::io::BufReader;
use std::io::BufWriter;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use chrono::NaiveDateTime;
use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::api::download_page;
use maimai_scraping::api::reqwest_client;
use maimai_scraping::cookie_store::CookieStore;
use maimai_scraping::schema::PlayRecord;

#[derive(Parser)]
struct Opts {
    json_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let path = &opts.json_file;

    let mut cookie_store = CookieStore::load()?;
    let client = reqwest_client()?;

    let mut records = load_from_file(path)?;

    // TODO: the order of loading should match to that of
    // https://maimaidx.jp/maimai-mobile/record/
    for i in (0..50).rev() {
        println!("Downloading idx={}...", i);
        if let Some(record) = download_page(&client, &mut cookie_store, i).await? {
            println!("  Downloaded record {:?}", record.played_at());
            if records.insert(*record.played_at().time(), record).is_some() {
                println!("The record above was already found in previous data; stopping.");
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    let file = BufWriter::new(File::create(path)?);
    serde_json::to_writer(file, &records.values().collect_vec())?;
    println!("Successfully saved data to {:?}.", path);

    Ok(())
}

fn load_from_file<P>(path: P) -> anyhow::Result<BTreeMap<NaiveDateTime, PlayRecord>>
where
    P: Into<PathBuf> + std::fmt::Debug,
{
    let path = path.into();
    match File::open(&path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            println!("Successfully loaded data from {:?}.", &path);
            let records: Vec<PlayRecord> = serde_json::from_reader(reader)?;
            Ok(records
                .into_iter()
                .map(|record| (*record.played_at().time(), record))
                .collect())
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                println!("The file was not found.");
                println!("We weill create a new file for you and save the data there.");
                Ok(BTreeMap::new())
            }
            _ => Err(anyhow!("Unexpected I/O Error: {:?}", e)),
        },
    }
}

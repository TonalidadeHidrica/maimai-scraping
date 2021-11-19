use std::collections::btree_map::Entry;
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
use maimai_scraping::api::download_record;
use maimai_scraping::api::download_record_index;
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

    let index = download_record_index(&client, &mut cookie_store).await?;
    // In `index`, newer result is stored first.
    // Since we want to fetch older result as fast as possible,
    // we inspect them in the reverse order.
    for (played_at, idx) in index.into_iter().rev() {
        println!("Checking idx={}...", idx);
        match records.entry(played_at) {
            Entry::Vacant(entry) => {
                let record = download_record(&client, &mut cookie_store, idx)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "  Once found record has been disappeared: played_at={}, idx={}",
                            played_at,
                            idx
                        )
                    })?;
                println!("  Downloaded record {:?}", record.played_at());
                if &played_at != record.played_at().time() {
                    println!(
                        "  Record has been updated at idx={}.  Probably there was a data loss.  Expected: {}, found: {}", 
                        idx, played_at, record.played_at().time());
                }
                entry.insert(record);
                std::thread::sleep(Duration::from_secs(2));
            }
            Entry::Occupied(entry) => {
                if *entry.get().played_at().idx() != idx {
                    println!(
                        "  The currently obtained idx is different from recorded: got {}",
                        idx,
                    );
                    println!("  Played at: {:?}", entry.get().played_at());
                }
            }
        }
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
                println!("We will create a new file for you and save the data there.");
                Ok(BTreeMap::new())
            }
            _ => Err(anyhow!("Unexpected I/O Error: {:?}", e)),
        },
    }
}

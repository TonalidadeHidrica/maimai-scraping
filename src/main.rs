use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::io;
use std::io::BufReader;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use chrono::NaiveDateTime;
use clap::ArgEnum;
use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::api::download_record;
use maimai_scraping::api::download_record_index;
use maimai_scraping::api::reqwest_client;
use maimai_scraping::api::try_login;
use maimai_scraping::cookie_store::CookieStore;
use maimai_scraping::cookie_store::CookieStoreLoadError;
use maimai_scraping::maimai::Maimai;
use maimai_scraping::ongeki::Ongeki;
use maimai_scraping::sega_trait::PlayRecordTrait;
use maimai_scraping::sega_trait::SegaTrait;
use serde::Deserialize;
use serde::Serialize;

#[derive(Parser)]
struct Opts {
    #[clap(arg_enum)]
    game: Game,
    json_file: PathBuf,
}
#[derive(Clone, ArgEnum)]
enum Game {
    Ongeki,
    Maimai,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let path = &opts.json_file;

    match opts.game {
        Game::Maimai => run::<Maimai>(path).await,
        Game::Ongeki => run::<Ongeki>(path).await,
    }
}

async fn run<T>(path: &Path) -> anyhow::Result<()>
where
    T: SegaTrait,
    T::Idx: Display + PartialEq,
    T::PlayRecord: Serialize,
    for<'a> T::PlayRecord: Deserialize<'a>,
    <T::PlayRecord as PlayRecordTrait>::PlayedAt: Debug,
{
    let mut records = load_from_file::<T, _>(path)?;

    let client = reqwest_client::<T>()?;

    let cookie_store = CookieStore::load();
    let (mut cookie_store, index) = match cookie_store {
        Ok(mut cookie_store) => {
            let index = download_record_index::<T>(&client, &mut cookie_store).await;
            (cookie_store, index.map_err(Some))
        }
        Err(CookieStoreLoadError::NotFound) => {
            println!("Cookie store was not found.  Trying to log in.");
            let cookie_store = try_login::<T>(&client).await?;
            (cookie_store, Err(None))
        }
        Err(e) => return Err(anyhow::Error::from(e)),
    };
    let index = match index {
        Ok(index) => index, // TODO: a bit redundant
        Err(err) => {
            if let Some(err) = err {
                println!("The stored session seems to be expired.  Trying to log in.");
                println!("    Detail: {:?}", err);
            }
            cookie_store = try_login::<T>(&client).await?;
            // return Ok(());
            download_record_index::<T>(&client, &mut cookie_store).await?
        }
    };
    println!("Successfully logged in.");

    // In `index`, newer result is stored first.
    // Since we want to fetch older result as fast as possible,
    // we inspect them in the reverse order.
    for (played_at, idx) in index.into_iter().rev() {
        println!("Checking idx={}...", idx);
        match records.entry(played_at) {
            Entry::Vacant(entry) => {
                let record = download_record::<T>(&client, &mut cookie_store, idx)
                    .await?
                    .ok_or_else(|| {
                        anyhow!(
                            "  Once found record has been disappeared: played_at={}, idx={}",
                            played_at,
                            idx
                        )
                    })?;
                println!("  Downloaded record {:?}", record.played_at());
                if played_at != record.time() {
                    println!(
                        "  Record has been updated at idx={}.  Probably there was a data loss.  Expected: {}, found: {}", 
                        idx, played_at, record.time());
                }
                entry.insert(record);
                std::thread::sleep(Duration::from_secs(2));
            }
            Entry::Occupied(entry) => {
                if entry.get().idx() != idx {
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

fn load_from_file<T, P>(path: P) -> anyhow::Result<BTreeMap<NaiveDateTime, T::PlayRecord>>
where
    T: SegaTrait,
    for<'a> T::PlayRecord: Deserialize<'a>,
    P: Into<PathBuf> + std::fmt::Debug,
{
    let path = path.into();
    match File::open(&path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            println!("Successfully loaded data from {:?}.", &path);
            let records: Vec<T::PlayRecord> = serde_json::from_reader(reader)?;
            Ok(records
                .into_iter()
                .map(|record| (record.time(), record))
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

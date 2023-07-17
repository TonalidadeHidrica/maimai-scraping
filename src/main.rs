use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::fmt::Display;
use std::io;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::anyhow;
use clap::ArgEnum;
use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::api::SegaClient;
use maimai_scraping::fs_json_util::write_json;
use maimai_scraping::maimai::Maimai;
use maimai_scraping::ongeki::Ongeki;
use maimai_scraping::sega_trait::Idx;
use maimai_scraping::sega_trait::PlayRecordTrait;
use maimai_scraping::sega_trait::PlayTime;
use maimai_scraping::sega_trait::PlayedAt;
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
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
    T::PlayRecord: Serialize,
    for<'a> T::PlayRecord: Deserialize<'a>,
{
    let mut records = load_from_file::<T, _>(path)?;
    let (mut client, index) = SegaClient::<T>::new().await?;

    // In `index`, newer result is stored first.
    // Since we want to fetch older result as fast as possible,
    // we inspect them in the reverse order.
    for (played_at, idx) in index.into_iter().rev() {
        println!("Checking idx={}...", idx);
        match records.entry(played_at) {
            Entry::Vacant(entry) => {
                let record = client.download_record(idx).await?.ok_or_else(|| {
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

    write_json(path, &records.values().collect_vec())?;
    println!("Successfully saved data to {:?}.", path);

    Ok(())
}

fn load_from_file<T, P>(path: P) -> anyhow::Result<BTreeMap<PlayTime<T>, T::PlayRecord>>
where
    T: SegaTrait,
    PlayTime<T>: Copy + Ord + Display,
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

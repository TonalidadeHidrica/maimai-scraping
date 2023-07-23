use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use clap::ArgEnum;
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::api::SegaClient;
use maimai_scraping::data_collector::load_records_from_file;
use maimai_scraping::data_collector::update_records;
use maimai_scraping::fs_json_util::write_json;
use maimai_scraping::maimai::Maimai;
use maimai_scraping::ongeki::Ongeki;
use maimai_scraping::sega_trait::Idx;
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
    let mut records = load_records_from_file::<T, _>(path)?;
    let (mut client, index) = SegaClient::<T>::new_with_default_path().await?;
    update_records(&mut client, &mut records, index).await?;
    write_json(path, &records.values().collect_vec())?;
    println!("Successfully saved data to {:?}.", path);
    Ok(())
}

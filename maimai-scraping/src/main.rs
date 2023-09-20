use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use clap::ArgEnum;
use clap::Parser;
use maimai_scraping::api::SegaClient;
use maimai_scraping::data_collector::load_or_create_user_data;
use maimai_scraping::data_collector::update_records;
use maimai_scraping::fs_json_util::write_json;
use maimai_scraping::maimai::Maimai;
use maimai_scraping::ongeki::Ongeki;
use maimai_scraping::sega_trait::Idx;
use maimai_scraping::sega_trait::PlayTime;
use maimai_scraping::sega_trait::PlayedAt;
use maimai_scraping::sega_trait::SegaTrait;
use maimai_scraping::sega_trait::SegaUserData;
use serde::Deserialize;
use serde::Serialize;

#[derive(Parser)]
struct Opts {
    #[clap(arg_enum)]
    game: Game,
    user_data_path: PathBuf,
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
    let path = &opts.user_data_path;

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
    T::UserData: Serialize,
    for<'a> T::UserData: Default + Deserialize<'a>,
{
    let mut data = load_or_create_user_data::<T, _>(path)?;
    let (mut client, index) = SegaClient::<T>::new_with_default_path().await?;
    update_records(&mut client, data.records_mut(), index).await?;
    write_json(path, &data)?;
    println!("Successfully saved data to {:?}.", path);
    Ok(())
}

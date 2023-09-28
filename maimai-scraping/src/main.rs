use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use log::info;
use maimai_scraping::api::SegaClient;
use maimai_scraping::cookie_store::PlayerName;
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
    #[arg(value_enum)]
    game: Game,
    user_data_path: PathBuf,
    #[arg(long)]
    credentials_path: Option<PathBuf>,
    #[arg(long)]
    cookie_store_path: Option<PathBuf>,
    #[arg(long)]
    player_name: Option<PlayerName>,
}
#[derive(Clone, ValueEnum)]
enum Game {
    Ongeki,
    Maimai,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    match opts.game {
        Game::Maimai => run::<Maimai>(&opts).await,
        Game::Ongeki => run::<Ongeki>(&opts).await,
    }
}

async fn run<T>(opts: &Opts) -> anyhow::Result<()>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
    T::UserData: Serialize,
    for<'a> T::UserData: Default + Deserialize<'a>,
{
    let mut data = load_or_create_user_data::<T, _>(&opts.user_data_path)?;
    let (mut client, index) = SegaClient::<T>::new(
        opts.credentials_path
            .as_deref()
            .unwrap_or_else(|| Path::new(T::CREDENTIALS_PATH)),
        opts.cookie_store_path
            .as_deref()
            .unwrap_or_else(|| Path::new(T::COOKIE_STORE_PATH)),
        opts.player_name.as_ref(),
    )
    .await?;
    update_records(&mut client, data.records_mut(), index).await?;
    write_json(&opts.user_data_path, &data)?;
    info!("Successfully saved data to {:?}.", opts.user_data_path);
    Ok(())
}

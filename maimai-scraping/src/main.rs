use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use clap::ValueEnum;
use log::info;
use maimai_scraping::api::SegaClient;
use maimai_scraping::api::SegaClientAndRecordList;
use maimai_scraping::api::SegaClientInitializer;
use maimai_scraping::cookie_store::UserIdentifier;
use maimai_scraping::data_collector::load_or_create_user_data;
use maimai_scraping::data_collector::update_records;
use maimai_scraping::fs_json_util::write_json;
use maimai_scraping::maimai::Maimai;
use maimai_scraping::maimai::MaimaiIntl;
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
    #[clap(flatten)]
    user_identifier: UserIdentifier,
}
#[derive(Clone, ValueEnum)]
enum Game {
    Ongeki,
    Maimai,
    MaimaiIntl,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    match opts.game {
        Game::Maimai => {
            let client = SegaClient::<Maimai>::new(make_initializer::<Maimai>(&opts)).await?;
            run(&opts, client).await
        }
        Game::Ongeki => {
            let client = SegaClient::<Ongeki>::new(make_initializer::<Maimai>(&opts)).await?;
            run(&opts, client).await
        }
        Game::MaimaiIntl => {
            let client = SegaClient::new_maimai_intl(make_initializer::<MaimaiIntl>(&opts)).await?;
            run(&opts, client).await
        }
    }
}

fn make_initializer<T: SegaTrait>(opts: &Opts) -> SegaClientInitializer<'_, '_> {
    SegaClientInitializer {
        credentials_path: opts
            .credentials_path
            .as_deref()
            .unwrap_or_else(|| Path::new(T::CREDENTIALS_PATH)),
        cookie_store_path: opts
            .cookie_store_path
            .as_deref()
            .unwrap_or_else(|| Path::new(T::COOKIE_STORE_PATH)),
        user_identifier: &opts.user_identifier,
    }
}

async fn run<T>(
    opts: &Opts,
    (mut client, index): SegaClientAndRecordList<'_, T>,
) -> anyhow::Result<()>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
    T::UserData: Serialize,
    for<'a> T::UserData: Default + Deserialize<'a>,
{
    let mut data = load_or_create_user_data::<T, _>(&opts.user_data_path)?;
    update_records(&mut client, data.records_mut(), index).await?;
    write_json(&opts.user_data_path, &data)?;
    info!("Successfully saved data to {:?}.", opts.user_data_path);
    Ok(())
}

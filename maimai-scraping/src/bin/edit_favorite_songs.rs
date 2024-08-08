use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        favorite_songs::{fetch_favorite_songs_form, SetFavoriteSong},
        Maimai,
    },
};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    #[clap(flatten)]
    user_identifier: UserIdentifier,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &opts.credentials_path,
        cookie_store_path: &opts.cookie_store_path,
        user_identifier: &opts.user_identifier,
        // There is no need to be Standard member to edit favorite songs
        force_paid: false,
    })
    .await?;
    let page = fetch_favorite_songs_form(&mut client).await?;
    SetFavoriteSong::builder()
        .token(&page.token)
        .music(vec![&page.songs[39].idx])
        .build()
        .send(&mut client)
        .await?;
    Ok(())
}

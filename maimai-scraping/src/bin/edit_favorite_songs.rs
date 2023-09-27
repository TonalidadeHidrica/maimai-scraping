use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    api::SegaClient,
    cookie_store::PlayerName,
    maimai::{
        favorite_songs::{fetch_favorite_songs_form, SetFavoriteSong},
        Maimai,
    },
};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    #[arg(long)]
    player_name: Option<PlayerName>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let (mut client, _) = SegaClient::<Maimai>::new(
        &opts.credentials_path,
        &opts.cookie_store_path,
        opts.player_name.as_ref(),
    )
    .await?;
    let page = fetch_favorite_songs_form(&mut client).await?;
    SetFavoriteSong::builder()
        .token(&page.token)
        .music(vec![&page.genres[0].songs[39].idx])
        .build()
        .send(&mut client)
        .await?;
    Ok(())
}

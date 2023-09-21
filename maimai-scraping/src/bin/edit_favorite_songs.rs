use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    api::SegaClient,
    maimai::{favorite_songs::SetFavoriteSong, parser::favorite_songs, Maimai},
};
use scraper::Html;
use url::Url;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let (mut client, _) =
        SegaClient::<Maimai>::new(&opts.credentials_path, &opts.cookie_store_path).await?;
    let page = favorite_songs::parse(&Html::parse_document(
        &client
            .fetch_authenticated(Url::parse(
                "https://maimaidx.jp/maimai-mobile/home/userOption/favorite/updateMusic",
            )?)
            .await?
            .0
            .text()
            .await?,
    ))?;

    SetFavoriteSong::builder()
        .token("token".to_owned().into())
        .music(vec![page.genres[0].songs[42].idx.clone()])
        .build()
        .send(&mut client)
        .await?;
    Ok(())
}

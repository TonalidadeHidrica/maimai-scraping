use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;
use fs_err::read_to_string;
use hashbrown::{HashMap, HashSet};
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        favorite_songs::{fetch_favorite_songs_form, SetFavoriteSong},
        parser, Maimai,
    },
};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    songs_path: PathBuf,
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

    let mut song_name_to_idx = HashMap::<&str, Vec<_>>::new();
    for song in &page.songs {
        song_name_to_idx
            .entry(song.name.as_ref())
            .or_default()
            .push(&song.idx);
    }

    let mut queries = HashSet::<&parser::favorite_songs::Idx>::new();
    for name in read_to_string(opts.songs_path)?.lines() {
        match song_name_to_idx.get(name) {
            None => bail!("Song not found: {name:?}"),
            Some(idxs) => queries.extend(idxs),
        }
    }

    if queries.len() >= 30 {
        bail!("Too many songs! ({} songs)", queries.len());
    }

    SetFavoriteSong::builder()
        .token(&page.token)
        .music(Vec::from_iter(queries))
        .build()
        .send(&mut client)
        .await?;
    Ok(())
}

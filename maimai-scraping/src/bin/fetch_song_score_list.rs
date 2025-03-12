use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use clap::Parser;
use hashbrown::HashMap;
use log::info;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        data_collector::get_icon_for_idx, parser::song_score, schema::latest::ScoreDifficulty,
        song_list::song_score::SongScoreList, Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::write_json;
use scraper::Html;
use tokio::time::sleep;
use url::Url;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    output_json: PathBuf,

    #[clap(flatten)]
    user_identifier: UserIdentifier,
}

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();
    let opts = Opts::parse();

    let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &opts.credentials_path,
        cookie_store_path: &opts.cookie_store_path,
        user_identifier: &opts.user_identifier,
        // There is no need to be Standard member to fetch song score page
        force_paid: false,
    })
    .await?;

    let mut result = SongScoreList::default();

    use ScoreDifficulty::*;
    let difficulties = [Basic, Advanced, Expert, Master, ReMaster];
    for (i, &difficulty) in difficulties.iter().enumerate() {
        info!("Fetching {difficulty:?}");
        let html = Html::parse_document(
            // TODO international ver
            &client
                .fetch_authenticated(Url::parse(&format!(
                    "https://maimaidx.jp/maimai-mobile/record/musicGenre/search/?genre=99&diff={i}"
                ))?)
                .await?
                .0
                .text()
                .await?,
        );
        result.entries[difficulty] = song_score::parse(&html, difficulty)?;
        sleep(Duration::from_secs(1)).await;
    }

    let duplicate_idx = difficulties.iter().flat_map(|&d| {
        let mut map = HashMap::<_, Vec<_>>::new();
        for entry in &result.entries[d] {
            map.entry((entry.song_name(), entry.metadata().generation()))
                .or_default()
                .push(entry.idx());
        }
        map.into_values().filter(|x| x.len() >= 2).flatten()
    });

    for idx in duplicate_idx {
        info!("Fetching {idx:?}");
        let icon = get_icon_for_idx(&mut client, idx).await?;
        result.idx_to_icon_map.insert(idx.clone(), icon);
        sleep(Duration::from_secs(1)).await;
    }

    write_json(&opts.output_json, &result)?;

    Ok(())
}

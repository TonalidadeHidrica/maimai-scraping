use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use clap::Parser;
use hashbrown::HashSet;
use log::info;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        data_collector::get_icon_for_idx, parser::song_score, rating::ScoreLevel,
        schema::latest::ScoreDifficulty, song_list::song_score::SongScoreList, Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::write_json;
use scraper::Html;
use tokio::time::sleep;

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
        let url = format!(
            "https://maimaidx.jp/maimai-mobile/record/musicGenre/search/?genre=99&diff={i}"
        );
        let html = Html::parse_document(&client.fetch_authenticated(url).await?.0.text().await?);
        result.by_difficulty[difficulty] = song_score::parse(&html)?;
        sleep(Duration::from_secs(1)).await;
    }

    for (level, i) in ScoreLevel::all().zip(1..) {
        info!("Fetching {level:?}");
        let url = format!("https://maimaidx.jp/maimai-mobile/record/musicLevel/search/?level={i}");
        let html = Html::parse_document(&client.fetch_authenticated(url).await?.0.text().await?);
        result.by_level.push((level, song_score::parse(&html)?));
        sleep(Duration::from_secs(1)).await;
    }

    let idxs = (result.by_difficulty.values())
        .chain(result.by_level.iter().map(|x| &x.1))
        .flatten()
        .flat_map(|x| &x.entries);
    let link_idx = idxs.filter_map(|entry| {
        let name: &str = entry.song_name().as_ref();
        (name == "Link" || name.starts_with("Help me, ERINNNNNN!!")).then_some(entry.idx())
    });
    let idx_list = link_idx.collect::<HashSet<_>>();

    for idx in idx_list {
        info!("Fetching {idx:?}");
        let icon = get_icon_for_idx(&mut client, idx).await?;
        result.idx_to_icon_map.insert(idx.clone(), icon);
        sleep(Duration::from_secs(1)).await;
    }

    write_json(&opts.output_json, &result)?;

    Ok(())
}

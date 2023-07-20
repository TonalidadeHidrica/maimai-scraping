use std::{iter::successors, path::PathBuf, thread::sleep, time::Duration};

use anyhow::Context;
use itertools::Itertools;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{
        load_records_from_file, load_targets_from_file, update_records, update_targets, RecordMap,
    },
    fs_json_util::write_json,
    maimai::{rating_target_parser::RatingTargetFile, schema::latest::PlayRecord, Maimai},
};
use serde::Deserialize;
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
};
use url::Url;

use crate::slack::webhook_send;

#[derive(Clone, Deserialize)]
pub struct Config {
    pub interval: Duration,
    pub records_path: PathBuf,
    pub rating_target_path: PathBuf,
    pub slack_post_webhook: Option<Url>,
}

pub async fn watch(config: Config) -> anyhow::Result<WatchHandler> {
    let (tx, mut rx) = mpsc::channel(100);
    let mut records = load_records_from_file::<Maimai, _>(&config.records_path)?;
    let mut rating_targets = load_targets_from_file(&config.rating_target_path)?;
    spawn(async move {
        'outer: while let Err(TryRecvError::Empty) = rx.try_recv() {
            if let Err(e) = run(&config, &mut records, &mut rating_targets).await {
                println!("{e}");
            }
            let chunk = Duration::from_millis(250);
            for remaining in successors(Some(config.interval), |x| x.checked_sub(chunk)) {
                sleep(remaining.min(chunk));
                if rx.try_recv().is_ok() {
                    break 'outer;
                }
            }
        }
    });
    Ok(WatchHandler(tx))
}

async fn run(
    config: &Config,
    records: &mut RecordMap<Maimai>,
    rating_targets: &mut RatingTargetFile,
) -> anyhow::Result<()> {
    let (mut client, index) = SegaClient::<Maimai>::new().await?;
    let last_played = index.first().context("There is no play yet.")?.0;
    let inserted_records = update_records(&mut client, records, index).await?;
    if inserted_records.is_empty() {
        return Ok(());
    }
    for record in inserted_records {
        webhook_send(
            client.reqwest(),
            &config.slack_post_webhook,
            make_message(record),
        )
        .await;
    }
    write_json(&config.records_path, &records.values().collect_vec())?;
    update_targets(&mut client, rating_targets, last_played).await?;
    write_json(&config.rating_target_path, &rating_targets)?;
    webhook_send(
        client.reqwest(),
        &config.slack_post_webhook,
        "Rating target updated",
    )
    .await;
    Ok(())
}

fn make_message(record: &PlayRecord) -> String {
    use maimai_scraping::maimai::schema::latest::{
        AchievementRank::*, FullComboKind::*, ScoreDifficulty::*, ScoreGeneration::*,
    };
    let title = record.song_metadata().name();
    let gen = match record.score_metadata().generation() {
        Standard => "STD",
        Deluxe => "DX",
    };
    let dif = match record.score_metadata().difficulty() {
        Basic => "Bas",
        Advanced => "Adv",
        Expert => "Exp",
        Master => "Mas",
        ReMaster => "ReMas",
    };
    let rank = match record.achievement_result().rank() {
        D => "D",
        C => "C",
        BBB => "BBB",
        BB => "BB",
        B => "B",
        A => "A",
        AA => "AA",
        AAA => "AAA",
        S => "S",
        SPlus => "S+",
        SS => "SS",
        SSPlus => "SS+",
        SSS => "SSS",
        SSSPlus => "SSS+",
    };
    let fc = match record.combo_result().full_combo_kind() {
        Nothing => "",
        FullCombo => "FC",
        FullComboPlus => "FC+",
        AllPerfect => "AP",
        AllPerfectPlus => "AP+",
    };
    format!(
        "{time}　{title} ({gen} {dif})　{rank}({ach}) {fc}",
        time = record.played_at().time(),
        ach = record.achievement_result().value(),
    )
}

pub struct WatchHandler(mpsc::Sender<()>);
impl WatchHandler {
    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(()).await
    }
}

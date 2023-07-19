use std::{iter::successors, path::PathBuf, thread::sleep, time::Duration};

use anyhow::Context;
use itertools::Itertools;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{
        load_records_from_file, load_targets_from_file, update_records, update_targets, RecordMap,
    },
    fs_json_util::write_json,
    maimai::{rating_target_parser::RatingTargetFile, Maimai},
};
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
};

pub struct Config {
    pub interval: Duration,
    pub records_path: PathBuf,
    pub rating_target_path: PathBuf,
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
    let data_downloaded = update_records(&mut client, records, index).await?;
    if !data_downloaded {
        return Ok(());
    }
    write_json(&config.records_path, &records.values().collect_vec())?;
    update_targets(&mut client, rating_targets, last_played).await?;
    write_json(&config.rating_target_path, &rating_targets)?;
    Ok(())
}

pub struct WatchHandler(mpsc::Sender<()>);
impl WatchHandler {
    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(()).await
    }
}

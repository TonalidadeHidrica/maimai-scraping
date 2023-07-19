use std::{
    fmt::{Debug, Display},
    path::PathBuf,
    thread::sleep,
    time::Duration,
};

use itertools::Itertools;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{load_records_from_file, update_records, RecordMap},
    fs_json_util::write_json,
    sega_trait::{Idx, PlayTime, PlayedAt, SegaTrait},
};
use serde::{Deserialize, Serialize};
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
};

pub struct Config {
    duration: Duration,
    records_path: PathBuf,
}

pub async fn watch<T>(config: Config) -> anyhow::Result<WatchHandler>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display + Send,
    PlayTime<T>: Copy + Ord + Display + Send + 'static,
    PlayedAt<T>: Debug + Send,
    T::PlayRecord: Serialize + Send + 'static,
    for<'a> T::PlayRecord: Deserialize<'a>,
{
    let (tx, mut rx) = mpsc::channel(100);
    let mut records = load_records_from_file::<T, _>(&config.records_path)?;
    spawn(async move {
        while let Err(TryRecvError::Empty) = rx.try_recv() {
            if let Err(e) = run::<T>(&config, &mut records).await {
                println!("{e}");
            }
            sleep(config.duration);
        }
    });
    Ok(WatchHandler(tx))
}

async fn run<T>(config: &Config, records: &mut RecordMap<T>) -> anyhow::Result<()>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
    T::PlayRecord: Serialize,
    for<'a> T::PlayRecord: Deserialize<'a>,
{
    let (mut client, index) = SegaClient::<T>::new().await?;
    update_records(&mut client, records, index).await?;
    write_json(&config.records_path, &records.values().collect_vec())?;
    Ok(())
}

pub struct WatchHandler(mpsc::Sender<()>);
impl WatchHandler {
    pub async fn stop(self) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(()).await
    }
}

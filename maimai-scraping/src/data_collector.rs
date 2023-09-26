use std::{
    collections::btree_map::Entry,
    fmt::{Debug, Display},
    io::BufReader,
    path::PathBuf,
    time::Duration,
};

use crate::{
    api::SegaClient,
    sega_trait::{Idx, PlayRecordTrait, PlayTime, PlayedAt, RecordMap, SegaTrait},
};
use anyhow::{anyhow, bail};
use fs_err::File;
use log::{info, trace, warn};
use serde::Deserialize;
use tokio::time::sleep;

pub fn load_or_create_user_data<T, P>(path: P) -> anyhow::Result<T::UserData>
where
    T: SegaTrait,
    for<'a> T::UserData: Default + Deserialize<'a>,
    P: Into<PathBuf> + Debug,
{
    let path = path.into();
    match File::open(&path) {
        Ok(file) => {
            let res = serde_json::from_reader(BufReader::new(file))?;
            info!("Successfully loaded data from {:?}.", &path);
            Ok(res)
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                info!("The file was not found.");
                info!("We will create a new file for you and save the data there.");
                Ok(T::UserData::default())
            }
            _ => bail!("Unexpected I/O Error: {:?}", e),
        },
    }
}

pub async fn update_records<'m, T>(
    client: &mut SegaClient<'_, T>,
    records: &'m mut RecordMap<T>,
    index: Vec<(PlayTime<T>, Idx<T>)>,
) -> anyhow::Result<Vec<PlayTime<T>>>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
{
    let mut inserted = vec![];
    // In `index`, newer result is stored first.
    // Since we want to fetch older result as fast as possible,
    // we inspect them in the reverse order.
    for (played_at, idx) in index.into_iter().rev() {
        trace!("Checking idx={}...", idx);
        match records.entry(played_at) {
            Entry::Vacant(entry) => {
                inserted.push(played_at);
                let record = client.download_record(idx).await?.ok_or_else(|| {
                    anyhow!(
                        "  Once found record has been disappeared: played_at={}, idx={}",
                        played_at,
                        idx
                    )
                })?;
                info!("  Downloaded record {:?}", record.played_at());
                if played_at != record.time() {
                    warn!(
                        "  Record has been updated at idx={}.  Probably there was a data loss.  Expected: {}, found: {}", 
                        idx, played_at, record.time());
                }
                entry.insert(record);
                sleep(Duration::from_secs(2)).await;
            }
            Entry::Occupied(entry) => {
                if entry.get().idx() != idx {
                    warn!("  The currently obtained idx is different from recorded: got {idx}",);
                    warn!("  Played at: {:?}", entry.get().played_at());
                }
            }
        }
    }
    Ok(inserted)
}

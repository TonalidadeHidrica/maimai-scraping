use std::{
    cmp::Ordering,
    collections::{btree_map::Entry, BTreeMap},
    fmt::{Debug, Display},
    io::{self, BufReader},
    path::PathBuf,
    time::Duration,
};

use crate::{
    api::SegaClient,
    maimai::{
        rating_target_parser::{self, RatingTargetFile},
        schema::latest::PlayTime as MaimaiPlayTime,
        Maimai,
    },
    sega_trait::{Idx, PlayRecordTrait, PlayTime, PlayedAt, SegaTrait},
};
use anyhow::{anyhow, bail};
use fs_err::File;
use log::{info, trace, warn};
use scraper::Html;
use serde::Deserialize;
use tokio::time::sleep;
use url::Url;

pub fn load_data_from_file<T, P>(path: P) -> anyhow::Result<T::UserData>
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

#[deprecated]
pub type RecordMap<T> = BTreeMap<PlayTime<T>, <T as SegaTrait>::PlayRecord>;
#[deprecated]
pub fn load_records_from_file<T, P>(path: P) -> anyhow::Result<RecordMap<T>>
where
    T: SegaTrait,
    PlayTime<T>: Ord,
    for<'a> T::PlayRecord: Deserialize<'a>,
    P: Into<PathBuf> + Debug,
{
    let path = path.into();
    match File::open(&path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            info!("Successfully loaded data from {:?}.", &path);
            let records: Vec<T::PlayRecord> = serde_json::from_reader(reader)?;
            Ok(records
                .into_iter()
                .map(|record| (record.time(), record))
                .collect())
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                info!("The file was not found.");
                info!("We will create a new file for you and save the data there.");
                Ok(BTreeMap::new())
            }
            _ => Err(anyhow!("Unexpected I/O Error: {:?}", e)),
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

#[deprecated]
pub fn load_targets_from_file(path: impl Into<PathBuf>) -> anyhow::Result<RatingTargetFile> {
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
                Ok(BTreeMap::new())
            }
            _ => bail!("Unexpected I/O Error: {:?}", e),
        },
    }
}

pub async fn update_targets(
    client: &mut SegaClient<'_, Maimai>,
    rating_targets: &mut RatingTargetFile,
    last_played: MaimaiPlayTime,
) -> anyhow::Result<()> {
    let last_saved = rating_targets.last_key_value().map(|x| *x.0);
    if let Some(date) = last_saved {
        println!("Rating target saved at: {date}");
    } else {
        println!("Rating target: not saved");
    }
    println!("Latest play at: {last_played}");
    match last_saved.cmp(&Some(last_played)) {
        Ordering::Less => println!("Updates needed."),
        Ordering::Equal => {
            println!("Already up to date.");
            return Ok(());
        }
        Ordering::Greater => {
            bail!("What?!  Inconsistent newest records between play records and rating targets!")
        }
    };

    let res = client
        .fetch_authenticated(Url::parse(
            "https://maimaidx.jp/maimai-mobile/home/ratingTargetMusic/",
        )?)
        .await?;
    let res = rating_target_parser::parse(&Html::parse_document(&res.0.text().await?))?;
    rating_targets.insert(last_played, res);
    Ok(())
}

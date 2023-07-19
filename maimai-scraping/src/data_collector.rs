use std::{
    collections::{btree_map::Entry, BTreeMap},
    fmt::{Debug, Display},
    io::{self, BufReader},
    path::PathBuf,
    time::Duration,
};

use crate::api::SegaClient;
use anyhow::anyhow;
use fs_err::File;
use serde::Deserialize;

use crate::sega_trait::{Idx, PlayRecordTrait, PlayTime, PlayedAt, SegaTrait};

type RecordMap<T> = BTreeMap<PlayTime<T>, <T as SegaTrait>::PlayRecord>;
pub fn load_from_file<T, P>(path: P) -> anyhow::Result<RecordMap<T>>
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
            println!("Successfully loaded data from {:?}.", &path);
            let records: Vec<T::PlayRecord> = serde_json::from_reader(reader)?;
            Ok(records
                .into_iter()
                .map(|record| (record.time(), record))
                .collect())
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                println!("The file was not found.");
                println!("We will create a new file for you and save the data there.");
                Ok(BTreeMap::new())
            }
            _ => Err(anyhow!("Unexpected I/O Error: {:?}", e)),
        },
    }
}

pub async fn update_records<T>(
    client: &mut SegaClient<T>,
    records: &mut RecordMap<T>,
    index: Vec<(PlayTime<T>, Idx<T>)>,
) -> anyhow::Result<()>
where
    T: SegaTrait,
    Idx<T>: Copy + PartialEq + Display,
    PlayTime<T>: Copy + Ord + Display,
    PlayedAt<T>: Debug,
{
    // In `index`, newer result is stored first.
    // Since we want to fetch older result as fast as possible,
    // we inspect them in the reverse order.
    for (played_at, idx) in index.into_iter().rev() {
        println!("Checking idx={}...", idx);
        match records.entry(played_at) {
            Entry::Vacant(entry) => {
                let record = client.download_record(idx).await?.ok_or_else(|| {
                    anyhow!(
                        "  Once found record has been disappeared: played_at={}, idx={}",
                        played_at,
                        idx
                    )
                })?;
                println!("  Downloaded record {:?}", record.played_at());
                if played_at != record.time() {
                    println!(
                        "  Record has been updated at idx={}.  Probably there was a data loss.  Expected: {}, found: {}", 
                        idx, played_at, record.time());
                }
                entry.insert(record);
                std::thread::sleep(Duration::from_secs(2));
            }
            Entry::Occupied(entry) => {
                if entry.get().idx() != idx {
                    println!(
                        "  The currently obtained idx is different from recorded: got {}",
                        idx,
                    );
                    println!("  Played at: {:?}", entry.get().played_at());
                }
            }
        }
    }
    Ok(())
}

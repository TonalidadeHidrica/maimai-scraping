use std::path::PathBuf;

use chrono::NaiveDateTime;
use clap::Parser;
use hashbrown::HashSet;
use itertools::Itertools;
use maimai_scraping::maimai::{
    estimate_rating::ScoreKey, load_score_level::MaimaiVersion, schema::latest::PlayTime,
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    input_files: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let versions = enum_iterator::all::<MaimaiVersion>()
        .map(|v| (v, v.start_time()))
        .zip_eq(
            enum_iterator::all::<MaimaiVersion>()
                .skip(1)
                .map(MaimaiVersion::start_time)
                .chain([NaiveDateTime::MAX]),
        )
        .collect_vec();
    for file in opts.input_files {
        let data: MaimaiUserData = read_json(&file)?;
        println!("{file:?}");
        let mut set = HashSet::new();
        for &((version, start), end) in &versions {
            let keys = || {
                data.records
                    .range(PlayTime::from(start)..PlayTime::from(end))
                    .map(|x| ScoreKey::from(x.1))
            };
            set.extend(keys());
            if keys().next().is_some() {
                println!("{version:?} => {}", set.len());
            }
        }
    }

    Ok(())
}

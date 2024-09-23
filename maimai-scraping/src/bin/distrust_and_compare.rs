use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use maimai_scraping::maimai::{
    internal_lv_estimator::{self, multi_user::update_all, Estimator},
    song_list::{self, database::SongDatabase},
    version::MaimaiVersion,
};
use maimai_scraping_utils::fs_json_util::{read_json, read_toml};

#[derive(Parser)]
struct Opts {
    database: PathBuf,
    estimator_config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let songs: Vec<song_list::Song> = read_json(opts.database)?;
    let database = SongDatabase::new(&songs)?;

    let config: internal_lv_estimator::multi_user::Config = read_toml(opts.estimator_config)?;
    let datas = config.read_all()?;

    let mut estimator = Estimator::new_distrust_all(&database, MaimaiVersion::latest())?;
    let before_len = estimator.event_len();
    update_all(&database, &datas, &mut estimator)?;
    for event in &estimator.events()[before_len..] {
        let stored = event
            .score()
            .for_version(MaimaiVersion::latest())
            .with_context(|| format!("Not found: {}", event.score()))?
            .level()
            .with_context(|| format!("Level not found: {}", event.score()))?;
        if event.candidates().intersection(stored).is_empty() {
            println!(
                "Contradiction detected: {}: estimated {}, found {}",
                event.score(),
                event.candidates(),
                stored
            );
        }
    }

    Ok(())
}

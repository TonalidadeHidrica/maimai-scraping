use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use maimai_scraping::{maimai::MaimaiUserData, sega_trait::PlayRecordTrait};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    user_data_path: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(opts.user_data_path)?;
    for (_, record) in data.records {
        let date = (record.idx().timestamp_jst())
            .unwrap_or_else(|| record.played_at().time())
            .get()
            .format("%Y-%m-%d %H:%M:%S");
        let title = record.song_metadata().name();
        let achievement = record.achievement_result().value();
        println!("{date} {title} {achievement}");
    }

    Ok(())
}

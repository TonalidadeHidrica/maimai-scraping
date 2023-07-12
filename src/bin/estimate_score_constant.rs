use std::{io::BufReader, path::PathBuf};

use anyhow::Context;
use chrono::NaiveTime;
use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::{
    load_score_level::{self, MaimaiVersion},
    schema::latest::PlayRecord,
};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    level_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    // TODO: use rank coeffieicnts for appropriate versions
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.input_file)?))?;

    let levels = load_score_level::load(opts.level_file)?;
    let levels = load_score_level::make_map(&levels)?;

    let version = MaimaiVersion::latest();
    let start_time = version.start_date().and_time(NaiveTime::from_hms(5, 0, 0));

    for record in records
        .iter()
        .filter(|r| start_time <= *r.played_at().time())
    {
        let song = levels
            .get(&(
                record.song_metadata().cover_art(),
                *record.score_metadata().generation(),
            ))
            .with_context(|| {
                format!(
                    "Song not found: {:?} {:?}",
                    record.song_metadata(),
                    record.score_metadata()
                )
            })?;
        println!("{song:?}");
    }

    Ok(())
}

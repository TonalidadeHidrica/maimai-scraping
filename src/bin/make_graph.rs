use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::schema::latest::{PlayRecord, ScoreDifficulty};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    song_name: String,
    difficulty: ScoreDifficulty,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.input_file)?))?;
    let filtered = records
        .iter()
        .filter(|x| {
            x.song_metadata().name() == &opts.song_name
                && *x.score_metadata().difficulty() == opts.difficulty
        })
        .collect_vec();
    println!("Found {} record(s)", filtered.len());

    Ok(())
}

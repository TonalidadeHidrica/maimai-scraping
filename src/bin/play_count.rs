use std::path::PathBuf;

use clap::Parser;
use indexmap::IndexMap;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::schema::latest::{PlayRecord, ScoreGeneration},
};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> = read_json(opts.input_file)?;

    let mut map = IndexMap::<_, Vec<_>>::new();
    for record in &records {
        map.entry((record.song_metadata().name(), record.score_metadata()))
            .or_default()
            .push(record);
    }
    for ((song_name, score_meta), records) in &map {
        let first_date = records.first().unwrap().played_at().time();
        let gen = match score_meta.generation() {
            ScoreGeneration::Standard => "S",
            ScoreGeneration::Deluxe => "D",
        };
        println!(
            "{first_date} {:3} {song_name} {:?}({gen})",
            records.len(),
            score_meta.difficulty()
        );
    }

    Ok(())
}

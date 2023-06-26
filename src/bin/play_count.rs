use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use indexmap::IndexMap;
use maimai_scraping::maimai::schema::latest::{PlayRecord, ScoreGeneration};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.input_file)?))?;

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
        println!("{first_date} {:3} {song_name} {:?}({gen})", records.len(), score_meta.difficulty());
    }

    Ok(())
}

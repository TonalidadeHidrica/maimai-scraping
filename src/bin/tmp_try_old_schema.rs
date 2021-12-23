use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::schema::ver_20210316_2338::PlayRecord;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.input_file)?))?;
    for record in records {
        let achievement = record.achievement_result();
        let rating = record.rating_result();
        println!(
            "{}\t{}\t{:?}\t{}",
            record.song_metadata().name(),
            achievement.value(),
            achievement.rank(),
            rating.rating()
        );
    }

    Ok(())
}

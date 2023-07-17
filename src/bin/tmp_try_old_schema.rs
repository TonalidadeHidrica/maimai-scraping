use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{fs_json_util::read_json, maimai::schema::ver_20210316_2338::PlayRecord};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let records: Vec<PlayRecord> = read_json(opts.input_file)?;
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

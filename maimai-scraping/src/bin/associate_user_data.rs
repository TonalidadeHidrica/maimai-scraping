use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::{
    associated_user_data,
    song_list::{
        database::{ScoreForVersionRef, SongDatabase},
        Song,
    },
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_file: PathBuf,
    user_data: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let database: Vec<Song> = read_json(&opts.database_file)?;
    let database = SongDatabase::new(&database)?;
    let user_data: MaimaiUserData = read_json(&opts.user_data)?;
    let data = associated_user_data::UserData::annotate(&database, &user_data)?;
    for record in data.records().values() {
        match record.score() {
            Ok(ScoreForVersionRef::Ordinary(score)) => {
                println!(
                    "{:?} {} ({:?} {:?} Lv.{}) {}",
                    record.record().played_at().time(),
                    record.record().song_metadata().name(),
                    score.version(),
                    score.score().difficulty(),
                    score
                        .level()
                        .map_or("?".to_owned(), |level| level.to_string()),
                    record.record().achievement_result().value(),
                );
            }
            Err(e) => {
                println!("Error: {e:#}");
            }
            _ => (),
        }
    }
    Ok(())
}

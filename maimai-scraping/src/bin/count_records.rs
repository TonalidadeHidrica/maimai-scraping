use std::path::PathBuf;

use chrono::NaiveDateTime;
use clap::Parser;
use either::Either;
use hashbrown::HashSet;
use itertools::Itertools;
use maimai_scraping::maimai::{
    associated_user_data,
    schema::latest::PlayTime,
    song_list::{
        database::{ScoreForVersionRef, SongDatabase},
        Song,
    },
    version::MaimaiVersion,
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database: PathBuf,
    input_files: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database)?;
    let database = SongDatabase::new(&songs)?;

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
        let associated = associated_user_data::UserData::annotate(&database, &data)?;

        println!("{file:?}");
        let mut set = HashSet::new();
        for &((version, start), end) in &versions {
            let mut exist = false;
            for (_, score) in associated
                .records()
                .range(PlayTime::from(start)..PlayTime::from(end))
            {
                let score = match score.score() {
                    Ok(score) => score,
                    Err(_) => {
                        // Report error?
                        continue;
                    }
                };
                let score = match score {
                    ScoreForVersionRef::Ordinary(x) => Either::Left(x.score()),
                    ScoreForVersionRef::Utage(x) => Either::Right(x),
                };
                set.insert(score);
                exist = true;
            }
            if exist {
                println!("{version:?} => {}", set.len());
            }
        }
    }

    Ok(())
}

use std::{cmp::Reverse, path::PathBuf};

use clap::Parser;
use enum_map::EnumMap;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating},
    schema::latest::AchievementValue,
    song_list::{database::SongDatabase, Song},
    version::MaimaiVersion,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let mut constants = EnumMap::<bool, Vec<_>>::default();
    let version = MaimaiVersion::latest();
    for score in database.all_scores_for_version(version) {
        let new = score.score().scores().scores().version == Some(version);
        if let Some(constant) = score.level().and_then(|x| x.get_if_unique()) {
            constants[new].push(constant);
        }
    }

    let ans = constants
        .into_iter()
        .map(|(new, mut constants)| {
            constants.sort_by_key(|&x| Reverse(x));
            let count = if new { 15 } else { 35 };
            let a = AchievementValue::try_from(101_0000).unwrap();
            println!("====");
            constants
                .iter()
                .take(count)
                .map(|&x| {
                    let ret = single_song_rating(x, a, rank_coef(a)).get();
                    println!("{x} {ret}");
                    ret
                })
                .sum::<u16>()
        })
        .sum::<u16>();
    println!("{ans}");

    Ok(())
}

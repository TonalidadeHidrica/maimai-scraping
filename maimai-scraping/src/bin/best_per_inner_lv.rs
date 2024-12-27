use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use hashbrown::HashMap;
use itertools::Itertools;
use maimai_scraping::maimai::{
    associated_user_data,
    rating::{InternalScoreLevel, ScoreConstant},
    schema::latest::AchievementValue,
    song_list::{database::SongDatabase, Song},
    version::MaimaiVersion,
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    user_data_path: PathBuf,
    inner_lv: u8,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let user_data: MaimaiUserData = read_json(opts.user_data_path)?;
    let data = associated_user_data::UserData::annotate(&database, &user_data)?;

    let zero = AchievementValue::try_from(0).unwrap();
    let mut best = HashMap::<_, AchievementValue>::new();
    for score in data.ordinary_data_associated()?.ordinary_records() {
        let a = score.record().achievement_result().value();
        let x = best.entry(score.score().score()).or_insert(zero);
        *x = (*x).max(a);
    }

    let lv: ScoreConstant = opts
        .inner_lv
        .try_into()
        .map_err(|_| anyhow!("Invalid inner lv"))?;
    let version = MaimaiVersion::latest();
    for (a, s) in database
        .all_scores_for_version(version)
        .map(|score| score.score())
        .filter(|x| x.score().levels[version].unwrap() == InternalScoreLevel::known(lv))
        .map(|score| (best.get(&score).copied().unwrap_or(zero), score))
        .sorted()
    {
        println!("{a:8} {s}");
    }

    Ok(())
}

use std::{cmp::Reverse, path::PathBuf};

use anyhow::{bail, Context};
use clap::Parser;
use hashbrown::HashMap;
use itertools::Itertools;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    associated_user_data::OrdinaryPlayRecordAssociated,
    internal_lv_estimator::{multi_user, Estimator},
    load_score_level::MaimaiVersion,
    rating::{rank_coef, single_song_rating, InternalScoreLevel},
    song_list::{database::SongDatabase, Song},
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    config_path: PathBuf,
    user_name: multi_user::UserName,

    // levels_json: PathBuf,
    // config: PathBuf,
    // user_name: estimator_config_multiuser::UserName,
    version: Option<MaimaiVersion>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let config: multi_user::Config = toml::from_str(&fs_err::read_to_string(opts.config_path)?)?;
    let datas = config.read_all()?;
    let datas = multi_user::associate_all(&database, &datas)?;

    let version = opts.version.unwrap_or_else(MaimaiVersion::latest);
    let mut estimator = Estimator::new(&database, version)?;
    multi_user::estimate_all(&datas, &mut estimator)?;

    let (_, data) = datas
        .iter()
        .find(|x| x.0.name() == &opts.user_name)
        .with_context(|| {
            format!(
                "User name {:?} not found in estimator config",
                opts.user_name
            )
        })?;

    let mut best = HashMap::new();
    for record in data.ordinary_records() {
        // The record is played before the version ends
        let record_within_version = record.record().played_at().time().get() < version.end_time();
        // The score is not removed as of this version
        let score_exists = record.score().score().for_version(version).is_some();
        // This is new record (record should be updated)
        let new_record = record.record().achievement_result().new_record();
        if record_within_version && score_exists && new_record {
            best.insert(record.score().score(), record);
        }
    }

    let current_version = opts.version.unwrap_or(MaimaiVersion::latest());
    let mut new_songs = vec![];
    let mut old_songs = vec![];
    for (score, record) in best {
        let levels = estimator
            .get(score)
            .with_context(|| format!("Score not found: {score}"))?;

        let a = record.record().achievement_result().value();
        let ratings = levels
            .candidates()
            .candidates()
            .map(|c| single_song_rating(c, a, rank_coef(a)).get())
            .collect_vec();
        if ratings.is_empty() {
            bail!("Empty candidates for {score}");
        }

        let songs = if score.scores().scores().version == Some(current_version) {
            &mut new_songs
        } else {
            &mut old_songs
        };
        songs.push((record, ratings, levels.candidates()));
    }

    let sort = |songs: &mut Vec<(&OrdinaryPlayRecordAssociated, Vec<u16>, InternalScoreLevel)>| {
        songs.sort_by_key(|(record, ratings, _)| {
            Reverse((
                *ratings.last().unwrap(),
                record.record().achievement_result().value(),
            ))
        });
    };
    sort(&mut new_songs);
    sort(&mut old_songs);

    for (label, songs, boundary) in [
        ("New", &new_songs, 15.min(new_songs.len())),
        ("Old", &old_songs, 35.min(old_songs.len())),
    ] {
        println!("{label} songs");
        for (i, (record, ratings, levels)) in songs.iter().enumerate() {
            if i == boundary {
                println!("=========");
            }
            let achievement = record.record().achievement_result().value();
            let score = record.score().score();
            let rating = {
                let min = ratings[0];
                let &max = ratings.last().unwrap();
                lazy_format!(
                    if min == max => "{min:3}    "
                    else => "{max:3}-{min:<3}"
                )
            };
            println!("{i:4} {achievement:>10} {rating} {levels:10} {score}");
        }
    }

    Ok(())
}

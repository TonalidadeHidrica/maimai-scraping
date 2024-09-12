use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Parser;
use fs_err::read_to_string;
use hashbrown::HashMap;
use itertools::Itertools;
use maimai_scraping::maimai::{
    estimate_rating::{visualize_rating_targets, ScoreConstantsStore, ScoreKey},
    estimator_config_multiuser::{self, update_all},
    load_score_level::{self, MaimaiVersion},
    parser::rating_target::RatingTargetEntry,
    rating::{rank_coef, single_song_rating},
};

#[derive(Parser)]
struct Opts {
    levels_json: PathBuf,
    config: PathBuf,
    user_name: estimator_config_multiuser::UserName,
    version: Option<MaimaiVersion>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let args = Opts::parse();
    let levels = load_score_level::load(&args.levels_json)?;
    let mut store = ScoreConstantsStore::new(&levels, &[])?;

    let config: estimator_config_multiuser::Root = toml::from_str(&read_to_string(&args.config)?)?;
    let datas = config.read_all()?;
    let data = datas
        .iter()
        .find_map(|(user, data)| (user.name() == &args.user_name).then_some(data))
        .with_context(|| {
            format!(
                "User name {:?} is not defined in the config file {:?}",
                args.user_name, args.config
            )
        })?;
    update_all(&datas, &mut store)?;

    let mut best = HashMap::new();
    for record in data.records.values() {
        if record.achievement_result().new_record() {
            best.insert(ScoreKey::from(record), record);
        }
    }

    let current_version = args.version.unwrap_or(MaimaiVersion::latest());
    let mut new_songs = vec![];
    let mut old_songs = vec![];
    for (key, record) in best {
        let (song, levels) = store.get(key)?.context("Song removed")?;
        let a = record.achievement_result().value();
        let entry = RatingTargetEntry::builder()
            .score_metadata(record.score_metadata())
            .song_name(record.song_metadata().name().clone())
            .level(levels[0].into())
            .achievement(a)
            .idx("dummy".to_owned().into())
            .build();
        let scores = levels
            .iter()
            .map(|&c| single_song_rating(c, a, rank_coef(a)).get())
            .collect_vec();
        let score = scores[0];
        if scores.iter().any(|&x| x != score) {
            bail!("Not same");
        }
        let songs = if song.version() == current_version {
            &mut new_songs
        } else {
            &mut old_songs
        };
        songs.push((score, entry));
    }

    let sort = |songs: &mut Vec<(u16, RatingTargetEntry)>| {
        songs.sort_by_key(|(score, entry)| (*score, entry.achievement()));
        songs.reverse();
    };
    sort(&mut new_songs);
    sort(&mut old_songs);

    let boundary = 15.min(new_songs.len());
    println!("  New songs");
    visualize_rating_targets(
        &store,
        new_songs[..boundary].iter().map(|x| &x.1),
        &data.idx_to_icon_map,
        0,
    )?;
    println!("  =========");
    visualize_rating_targets(
        &store,
        new_songs[boundary..].iter().map(|x| &x.1),
        &data.idx_to_icon_map,
        boundary,
    )?;

    let boundary = 35.min(old_songs.len());
    println!("  Old songs");
    visualize_rating_targets(
        &store,
        old_songs[..boundary].iter().map(|x| &x.1),
        &data.idx_to_icon_map,
        0,
    )?;
    println!("  =========");
    visualize_rating_targets(
        &store,
        old_songs[boundary..].iter().map(|x| &x.1),
        &data.idx_to_icon_map,
        boundary,
    )?;

    Ok(())
}

use std::{collections::BTreeSet, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use fs_err::read_to_string;
use inquire::CustomType;
use joinery::JoinableIterator;
use maimai_scraping::{
    chrono_util::jst_now,
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{KeyFromTargetEntry, PrintResult, ScoreConstantsStore, ScoreKey},
        estimator_config_multiuser,
        load_score_level::{self, MaimaiVersion},
        rating::ScoreConstant,
        schema::latest::{AchievementValue, PlayTime, ScoreDifficulty, ScoreGeneration, SongName},
        MaimaiUserData,
    },
};
use ordered_float::OrderedFloat;

#[derive(Parser)]
struct Opts {
    old_levels_json: PathBuf,
    levels_json: PathBuf,
    config: PathBuf,

    #[clap(default_value = "10")]
    level_update_factor: f64,

    #[clap(long, value_enum, default_value = "quiet")]
    estimator_detail: PrintResult,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();
    let config: estimator_config_multiuser::Root = toml::from_str(&read_to_string(args.config)?)?;
    let datas = (config.users().iter())
        .map(|config| anyhow::Ok((config, read_json::<_, MaimaiUserData>(config.data_path())?)))
        .collect::<Result<Vec<_>, _>>()?;

    let old_levels = load_score_level::load(&args.old_levels_json)?;
    let old_store = ScoreConstantsStore::new(&old_levels, &[])?;

    let levels = load_score_level::load(&args.levels_json)?;
    let mut store = ScoreConstantsStore::new(&levels, &[])?;
    store.show_details = args.estimator_detail;

    update_all(&datas, &mut store)?;
    let count_initial = store.num_determined_songs();

    let initial_rating = read_i16("Initial rating");
    let mut history: Vec<HistoryEntry> = vec![];
    struct HistoryEntry<'s> {
        key: ScoreKey<'s>,
        name: &'s SongName,
        achievement: AchievementValue,
        rating: i16,
        time: PlayTime,
    }
    'outer_loop: loop {
        let mut store = store.clone();
        let res = match (|| {
            // store.show_details = PrintResult::Detailed;
            for (i, entry) in history.iter().enumerate() {
                let rating_before = history
                    .get(i.wrapping_sub(1))
                    .map_or(initial_rating, |x| x.rating);
                let rating_delta = entry.rating - rating_before;
                store
                    .register_single_song_rating(
                        entry.key,
                        entry.achievement,
                        rating_delta,
                        entry.time,
                    )
                    .context("While registering single song rating")?;
            }
            update_all(&datas, &mut store).context("While updating under assumptions")?;
            // store.show_details = PrintResult::Quiet;
            get_optimal_song(&datas, &store, &old_store, args.level_update_factor)
                .context("While getting optimal song")
        })() {
            Err(e) => {
                println!("Error: {e:#}");
                None
            }
            Ok(None) => {
                println!("No more candidates!");
                break;
            }
            Ok(Some(res)) => {
                println!(
                    "{} songs has been determined so far",
                    store.num_determined_songs() - count_initial
                );
                println!(
                    "{} {:?} {:?}",
                    res.song.song_name(),
                    res.key.generation,
                    res.key.difficulty,
                );
                println!(
                    "{:.3} more songs is expected to be determined",
                    res.expected_count
                );
                println!(
                    "Old constants: {}",
                    res.old_constants.iter().join_with(", ")
                );
                println!("New constants: {}", res.constants.iter().join_with(", "));
                Some(res)
            }
        };

        enum Command {
            List,
            Undo,
            Add,
        }
        let command = loop {
            let res = CustomType::<String>::new("Command")
                .prompt()
                .map(|e| e.to_lowercase());
            match res.as_ref().map(|e| &e[..]) {
                Ok("undo") => break Command::Undo,
                Ok("add") => break Command::Add,
                Ok("list") => break Command::List,
                Err(inquire::InquireError::OperationInterrupted) => {
                    println!("Bye");
                    break 'outer_loop;
                }
                v => println!("Invalid command: {v:?}"),
            }
        };
        match command {
            Command::List => {
                println!();
                println!("=== Start of list ===");
                println!("Initial rating: {initial_rating}");
                for entry in &history {
                    println!(
                        "{} {:?} {:?} {} {}",
                        entry.name,
                        entry.key.generation,
                        entry.key.difficulty,
                        entry.achievement,
                        entry.rating
                    );
                }
                println!("=== End of list ===");
                println!();
            }
            Command::Undo => {
                if history.pop().is_none() {
                    println!("No more entry to remove!")
                }
            }
            Command::Add => {
                let Some(res) = res else {
                    println!("Resolve error before advancing!");
                    continue;
                };
                let achievement = loop {
                    let achievement = CustomType::<u32>::new("Achievement")
                        .prompt()
                        .map(AchievementValue::try_from);
                    match achievement {
                        Ok(Ok(v)) => break v,
                        e => println!("Invalid achievement: {e:?}"),
                    }
                };
                let rating = read_i16("Rating after play");
                history.push(HistoryEntry {
                    key: res.key,
                    name: res.song.song_name(),
                    achievement,
                    rating,
                    time: jst_now().into(),
                });
            }
        }
    }
    Ok(())
}

fn read_i16(message: &'static str) -> i16 {
    loop {
        match CustomType::<i16>::new(message).prompt() {
            Ok(v) => break v,
            Err(v) => println!("Invalid rating value: {v}"),
        }
    }
}

fn get_optimal_song<'s, 'o>(
    datas: &[(&estimator_config_multiuser::User, MaimaiUserData)],
    store: &ScoreConstantsStore<'s>,
    old_store: &'o ScoreConstantsStore<'s>,
    level_update_factor: f64,
) -> Result<Option<OptimalSongEntry<'s, 'o>>, anyhow::Error> {
    let undetermined_song_in_list = datas
        .iter()
        .flat_map(|(_, data)| &data.rating_targets)
        .filter_map(|(k, v)| (MaimaiVersion::latest().start_time() <= k.get()).then_some(v))
        .flat_map(|r| {
            [
                r.target_new(),
                r.target_old(),
                r.candidates_new(),
                r.candidates_old(),
            ]
        })
        .flatten()
        .filter_map(|entry| match store.key_from_target_entry(entry) {
            KeyFromTargetEntry::Unique(key) => Some(key),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut candidates = vec![];
    for key in undetermined_song_in_list {
        let Ok(Some((song, constants))) = store.get(key) else {
            continue;
        };
        if constants.len() <= 1 {
            continue;
        }
        let old_constants = if let Ok(Some((_, old_constants))) = old_store.get(key) {
            old_constants
        } else {
            println!("Warning: constans cannot be rertrieved: {key:?}");
            &[]
        };
        let mut factor_sum = 0f64;
        let mut weighted_count_sum = 0f64;
        for &constant in constants {
            let factor = if old_constants.is_empty() {
                1.
            } else {
                old_constants
                    .iter()
                    .map(|&c| {
                        level_update_factor
                            .powi(-((u8::from(c)).abs_diff(u8::from(constant)) as i32))
                    })
                    .sum()
            };

            let mut store = store.clone();
            let count_before = store.num_determined_songs();
            // Error shuold not occur at this stage
            store.set(key, [constant], "assumption").with_context(|| {
                format!(
                    "When assuming {} {:?} {:?} to be {constant}",
                    song.song_name(),
                    key.generation,
                    key.difficulty,
                )
            })?;
            update_all(datas, &mut store)?;
            let count_determined_anew = store.num_determined_songs() - count_before;
            factor_sum += factor;
            weighted_count_sum += factor * count_determined_anew as f64;
            // println!(
            //     "If {} {:?} {:?} is {constant} (prob. {factor:.3}) => {count_determined_anew:.3} more songs",
            //     song.song_name(),
            //     key.generation,
            //     key.difficulty,
            // );
        }
        let expected_count = weighted_count_sum / factor_sum;
        if let (ScoreGeneration::Deluxe, ScoreDifficulty::Master | ScoreDifficulty::ReMaster) =
            (key.generation, key.difficulty)
        {
            // Skip
        } else {
            candidates.push(OptimalSongEntry {
                expected_count,
                key,
                song,
                old_constants,
                constants: constants.to_owned(),
            });
        }
    }
    candidates.sort_by_key(|x| OrderedFloat(-x.expected_count));
    let res = candidates.into_iter().next();
    Ok(res)
}

#[derive(Clone)]
struct OptimalSongEntry<'s, 'o> {
    expected_count: f64,
    key: ScoreKey<'s>,
    song: &'s load_score_level::Song,
    old_constants: &'o [ScoreConstant],
    constants: Vec<ScoreConstant>,
}

fn update_all(
    datas: &[(&estimator_config_multiuser::User, MaimaiUserData)],
    constants: &mut ScoreConstantsStore,
) -> anyhow::Result<()> {
    while {
        let mut changed = false;
        for (config, data) in datas {
            changed |= constants.do_everything(
                config.estimator_config(),
                data.records.values(),
                &data.rating_targets,
            )?;
        }
        changed
    } {}
    Ok(())
}

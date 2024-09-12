use std::{collections::BTreeSet, path::PathBuf};

use anyhow::Context;
use chrono::Local;
use clap::Parser;
use fs_err::read_to_string;
use inquire::CustomType;
use itertools::Itertools;
use joinery::JoinableIterator;
use maimai_scraping::{
    chrono_util::jst_now,
    maimai::{
        estimate_rating::{KeyFromTargetEntry, PrintResult, ScoreConstantsStore, ScoreKey},
        estimator_config_multiuser::{self, update_all},
        load_score_level::{self, MaimaiVersion, RemovedSong},
        rating::ScoreConstant,
        schema::latest::{
            AchievementValue, PlayTime, ScoreDifficulty, ScoreGeneration, SongIcon, SongName,
        },
        song_list::{database::SongDatabase, Song},
        MaimaiUserData,
    },
};
use maimai_scraping_utils::fs_json_util::{read_json, write_json};
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
struct Opts {
    old_levels_json: PathBuf,
    levels_json: PathBuf,
    config: PathBuf,
    database: PathBuf,

    #[clap(long)]
    backup_dir: Option<PathBuf>,

    #[clap(long)]
    removed_songs: Option<PathBuf>,

    #[clap(default_value = "10")]
    level_update_factor: f64,

    #[clap(long, value_enum, default_value = "quiet")]
    estimator_detail: PrintResult,

    #[clap(long)]
    only_estimate: bool,

    #[clap(long)]
    restore_output: Option<PathBuf>,

    #[clap(long)]
    no_dx_master: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();
    let config: estimator_config_multiuser::Root = toml::from_str(&read_to_string(args.config)?)?;
    let datas = config.read_all()?;

    let old_levels = load_score_level::load(&args.old_levels_json)?;
    let old_store = ScoreConstantsStore::new(&old_levels, &[])?;

    let levels = load_score_level::load(&args.levels_json)?;
    let removed_songs: Vec<RemovedSong> = args
        .removed_songs
        .as_ref()
        .map_or_else(|| Ok(Vec::new()), read_json)?;
    let mut store = ScoreConstantsStore::new(&levels, &removed_songs)?;
    store.show_details = args.estimator_detail;

    let songs: Vec<Song> = read_json(args.database)?;
    let database = SongDatabase::new(&songs)?;

    update_all(&datas, &mut store)?;
    let count_initial = store.num_determined_songs();

    if args.only_estimate {
        return Ok(());
    }

    if let Some(path) = &args.restore_output {
        let data: BackupOwned = read_json(path)?;
        println!("{}", data.initial_rating);
        for entry in data.history {
            println!(
                "# {} {:?} {:?}",
                entry.name, entry.key.generation, entry.key.difficulty
            );
            println!("{}", entry.achievement);
            println!("{}", entry.rating);
        }
        return Ok(());
    }

    let initial_rating = read_i16("Initial rating");
    let mut history: Vec<HistoryEntry> = vec![];
    let mut candidate_len: usize = 1;
    'outer_loop: loop {
        let mut store = store.clone();
        let optimal_songs = (|| {
            for (i, entry) in history.iter().enumerate() {
                let rating_before = history
                    .get(i.wrapping_sub(1))
                    .map_or(initial_rating, |x| x.rating);
                let rating_delta = entry.rating - rating_before;
                store
                    .register_single_song_rating(
                        entry.key,
                        None,
                        entry.achievement,
                        rating_delta,
                        entry.time,
                    )
                    .context("While registering single song rating")?;
            }
            update_all(&datas, &mut store).context("While updating under assumptions")?;
            println!(
                "{} songs has been determined so far",
                store.num_determined_songs() - count_initial
            );
            for (user, data) in &datas {
                let (mut got, mut all) = (0, 0);
                for key in covered_scores(data, &store) {
                    if args.no_dx_master && is_dx_master(key) {
                        continue;
                    }
                    let Ok(Some((_song, constants))) = store.get(key) else {
                        continue;
                    };
                    all += 1;
                    if constants.len() == 1 {
                        got += 1;
                    }
                }
                print!("{} {got}/{all}  ", user.name());
            }
            println!();
            let ret = get_optimal_song(
                &datas,
                &store,
                &old_store,
                args.level_update_factor,
                args.no_dx_master,
            )
            .context("While getting optimal song");
            ret
        })();
        let res = match optimal_songs {
            Err(e) => {
                println!("Error: {e:#}");
                vec![]
            }
            Ok(v) if v.is_empty() => {
                println!("No more candidates!");
                break;
            }
            Ok(v) => {
                for (i, res) in v.iter().enumerate().take(candidate_len) {
                    let locked = match database.song_from_name(res.song.song_name()).collect_vec()[..]
                    {
                        [song] => match song.song().locked_history.values().last() {
                            Some(true) => '!',
                            Some(false) => ' ',
                            None => '?',
                        },
                        _ => '?',
                    };
                    println!(
                        "{i:3}: {locked} {} {:?} {:?}",
                        res.song.song_name(),
                        res.key.generation,
                        res.key.difficulty,
                    );
                    println!(
                        "       {:.3} more songs is expected to be determined",
                        res.expected_count
                    );
                    println!(
                        "       Old constants: {}",
                        res.old_constants.iter().join_with(", ")
                    );
                    println!(
                        "       New constants: {}",
                        res.constants.iter().join_with(", ")
                    );
                }
                v
            }
        };

        enum Command {
            List,
            Undo,
            Add,
            Len,
        }
        let command = loop {
            let res = CustomType::<String>::new("Command")
                .prompt()
                .map(|e| e.to_lowercase());
            match res.as_ref().map(|e| &e[..]) {
                Ok("undo") => break Command::Undo,
                Ok("add") => break Command::Add,
                Ok("list") => break Command::List,
                Ok("len") => break Command::Len,
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
                if res.is_empty() {
                    println!("Resolve error before advancing!");
                    continue;
                };
                let res = loop {
                    let len = res.len().min(candidate_len);
                    let index = read_usize(&format!("Candidate length (length: {})", len));
                    if index < len {
                        break &res[index];
                    }
                    println!("Index out of range");
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
            Command::Len => {
                candidate_len = loop {
                    let res = read_usize(&format!("Candidate length (current: {candidate_len})"));
                    if res > 0 {
                        break res;
                    }
                    println!("Value must be positive");
                }
            }
        }
        if let Some(backup_dir) = &args.backup_dir {
            let path =
                backup_dir.join(format!("{}.json", Local::now().format("%Y-%m-%d_%H-%M-%S")));
            if let Err(e) = write_json(
                path,
                &Backup {
                    initial_rating,
                    history: &history,
                },
            ) {
                println!("{e:#}");
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Backup<'s, 't> {
    initial_rating: i16,
    history: &'t [HistoryEntry<'s>],
}
#[derive(Deserialize)]
struct BackupOwned {
    initial_rating: i16,
    history: Vec<HistoryEntryOwned>,
}
#[derive(Serialize)]
struct HistoryEntry<'s> {
    key: ScoreKey<'s>,
    name: &'s SongName,
    achievement: AchievementValue,
    rating: i16,
    time: PlayTime,
}
#[derive(Deserialize)]
struct HistoryEntryOwned {
    key: ScoreKeyOwned,
    name: SongName,
    achievement: AchievementValue,
    rating: i16,
    #[allow(unused)]
    time: PlayTime,
}
#[derive(Deserialize)]
pub struct ScoreKeyOwned {
    pub icon: SongIcon,
    pub generation: ScoreGeneration,
    pub difficulty: ScoreDifficulty,
}

fn read_i16(message: &'static str) -> i16 {
    loop {
        match CustomType::<i16>::new(message).prompt() {
            Ok(v) => break v,
            Err(v) => println!("Invalid rating value: {v}"),
        }
    }
}
fn read_usize(message: &str) -> usize {
    loop {
        match CustomType::<usize>::new(message).prompt() {
            Ok(v) => break v,
            Err(v) => println!("Invalid rating value: {v}"),
        }
    }
}

fn is_dx_master(key: ScoreKey<'_>) -> bool {
    key.generation == ScoreGeneration::Deluxe
        && [ScoreDifficulty::Master, ScoreDifficulty::ReMaster]
            .iter()
            .any(|&d| d == key.difficulty)
}

fn get_optimal_song<'s, 'o>(
    datas: &'s [(&estimator_config_multiuser::User, MaimaiUserData)],
    store: &ScoreConstantsStore<'s>,
    old_store: &'o ScoreConstantsStore<'s>,
    level_update_factor: f64,
    no_dx_master: bool,
) -> Result<Vec<OptimalSongEntry<'s, 'o>>, anyhow::Error> {
    let covered_scores = datas
        .iter()
        .flat_map(|(_, data)| covered_scores(data, store));
    let mut candidates = vec![];
    for key in covered_scores {
        if no_dx_master && is_dx_master(key) {
            continue;
        }
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
            store.show_details = PrintResult::Quiet;
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
        candidates.push(OptimalSongEntry {
            expected_count,
            key,
            song,
            old_constants,
            constants: constants.to_owned(),
        });
    }
    candidates.sort_by_key(|x| OrderedFloat(-x.expected_count));
    Ok(candidates)
}

fn covered_scores<'s>(
    data: &'s MaimaiUserData,
    store: &ScoreConstantsStore<'s>,
) -> BTreeSet<ScoreKey<'s>> {
    data.rating_targets
        .iter()
        .filter_map(move |(k, v)| {
            (MaimaiVersion::latest().start_time() <= k.get()).then_some((v, data))
        })
        .flat_map(|(r, data)| {
            [
                r.target_new(),
                r.target_old(),
                r.candidates_new(),
                r.candidates_old(),
            ]
            .into_iter()
            .flatten()
            .map(move |e| (e, data))
        })
        .filter_map(|(entry, data)| {
            if entry.achievement().get() < 80_0000 {
                return None;
            }
            match store.key_from_target_entry(entry, &data.idx_to_icon_map) {
                KeyFromTargetEntry::Unique(key) => Some(key),
                _ => None,
            }
        })
        .collect()
}

#[derive(Clone)]
struct OptimalSongEntry<'s, 'o> {
    expected_count: f64,
    key: ScoreKey<'s>,
    song: &'s load_score_level::Song,
    old_constants: &'o [ScoreConstant],
    constants: Vec<ScoreConstant>,
}

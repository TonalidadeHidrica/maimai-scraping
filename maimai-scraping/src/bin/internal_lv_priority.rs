use std::{collections::BTreeSet, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use enum_iterator::Sequence;
use inquire::CustomType;
use maimai_scraping::maimai::{
    associated_user_data::UserDataOrdinaryAssociated,
    internal_lv_estimator::{
        self,
        multi_user::{self, MultiUserEstimator, RecordLabel},
        Estimator,
    },
    load_score_level::MaimaiVersion,
    rating::InternalScoreLevel,
    schema::latest::{AchievementValue, ScoreDifficulty, ScoreGeneration},
    song_list::{
        database::{OrdinaryScoreRef, SongDatabase},
        Song,
    },
};
use maimai_scraping_utils::fs_json_util::read_json;
use ordered_float::OrderedFloat;

#[derive(Parser)]
struct Opts {
    config: PathBuf,
    database: PathBuf,

    // #[clap(long)]
    // backup_dir: Option<PathBuf>,
    #[clap(default_value = "10")]
    level_update_factor: f64,

    // #[clap(long, value_enum, default_value = "quiet")]
    // estimator_detail: PrintResult,

    // #[clap(long)]
    // restore_output: Option<PathBuf>,
    #[clap(long)]
    no_dx_master: bool,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(&opts.database)?;
    let database = SongDatabase::new(&songs)?;
    let mut estimator = Estimator::new(&database, MaimaiVersion::latest())?;

    let config: multi_user::Config = toml::from_str(&fs_err::read_to_string(&opts.config)?)?;
    let datas = config.read_all()?;
    let datas = multi_user::associate_all(&database, &datas)?;
    multi_user::estimate_all(&datas, &mut estimator)?;

    let count_initial = estimator.num_determined_scores();

    // if let Some(path) = &opts.restore_output {
    //     let data: BackupOwned = read_json(path)?;
    //     println!("{}", data.initial_rating);
    //     for entry in data.history {
    //         println!(
    //             "# {} {:?} {:?}",
    //             entry.name, entry.key.generation, entry.key.difficulty
    //         );
    //         println!("{}", entry.achievement);
    //         println!("{}", entry.rating);
    //     }
    //     return Ok(());
    // }

    let initial_rating = read_i16("Initial rating");
    let mut history: Vec<HistoryEntry> = vec![];
    let mut candidate_len: usize = 1;
    'outer_loop: loop {
        let mut estimator = estimator.clone();
        let optimal_songs = (|| {
            for (i, entry) in history.iter().enumerate() {
                let rating_before = history
                    .get(i.wrapping_sub(1))
                    .map_or(initial_rating, |x| x.rating);
                let rating_delta = entry.rating - rating_before;
                estimator
                    .register_single_song_rating(
                        entry.score,
                        entry.achievement,
                        rating_delta,
                        RecordLabel::Additional,
                    )
                    .context("While registering single song rating")?;
            }
            multi_user::estimate_all(&datas, &mut estimator)
                .context("While updating under assumptions")?;
            println!(
                "{} songs has been determined so far",
                estimator.num_determined_scores() - count_initial
            );
            for (user, data) in &datas {
                let (mut got, mut all) = (0, 0);
                for score in covered_scores(data) {
                    if opts.no_dx_master && is_dx_master(score) {
                        continue;
                    }
                    all += 1;
                    let candidates = estimator
                        .get(score)
                        .with_context(|| format!("Key not found in estimator: {score}"))?;
                    if candidates.candidates().is_unique() {
                        got += 1;
                    }
                }
                print!("{} {got}/{all}  ", user.name());
            }
            println!();
            let ret = get_optimal_song(
                &datas,
                &estimator,
                opts.level_update_factor,
                opts.no_dx_master,
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
                    let locked = match res
                        .score
                        .scores()
                        .song()
                        .song()
                        .locked_history
                        .values()
                        .last()
                    {
                        Some(true) => '!',
                        Some(false) => ' ',
                        None => '?',
                    };
                    println!("{i:3}: {locked} {}", res.score);
                    println!(
                        "       {:.3} more songs is expected to be determined",
                        res.expected_count
                    );
                    println!("       Old constants: {}", res.old_constants);
                    println!("       New constants: {}", res.new_constants);
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
                    println!("{} {} {}", entry.score, entry.achievement, entry.rating);
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
                    score: res.score,
                    achievement,
                    rating,
                    // time: jst_now().into(),
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
        // if let Some(backup_dir) = &opts.backup_dir {
        //     let path =
        //         backup_dir.join(format!("{}.json", Local::now().format("%Y-%m-%d_%H-%M-%S")));
        //     if let Err(e) = write_json(
        //         path,
        //         &Backup {
        //             initial_rating,
        //             history: &history,
        //         },
        //     ) {
        //         println!("{e:#}");
        //     }
        // }
    }
    Ok(())
}

// #[derive(Serialize)]
// struct Backup<'s, 't> {
//     initial_rating: i16,
//     history: &'t [HistoryEntry<'s>],
// }
// #[derive(Deserialize)]
// struct BackupOwned {
//     initial_rating: i16,
//     history: Vec<HistoryEntryOwned>,
// }
// #[derive(Serialize)]
struct HistoryEntry<'s> {
    score: OrdinaryScoreRef<'s>,
    // name: &'s SongName,
    achievement: AchievementValue,
    rating: i16,
    // time: PlayTime,
}
// #[derive(Deserialize)]
// struct HistoryEntryOwned {
//     key: ScoreKeyOwned,
//     name: SongName,
//     achievement: AchievementValue,
//     rating: i16,
//     #[allow(unused)]
//     time: PlayTime,
// }
// #[derive(Deserialize)]
// pub struct ScoreKeyOwned {
//     pub icon: SongIcon,
//     pub generation: ScoreGeneration,
//     pub difficulty: ScoreDifficulty,
// }

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

fn is_dx_master(score: OrdinaryScoreRef<'_>) -> bool {
    matches!(score.scores().generation(), ScoreGeneration::Deluxe)
        && matches!(
            score.difficulty(),
            ScoreDifficulty::Master | ScoreDifficulty::ReMaster
        )
    // key.generation == ScoreGeneration::Deluxe
    //     && [ScoreDifficulty::Master, ScoreDifficulty::ReMaster]
    //         .iter()
    //         .any(|&d| d == key.difficulty)
}

fn get_optimal_song<'s>(
    datas: &'s [multi_user::AssociatedDataPair],
    estimator: &MultiUserEstimator<'s, '_>,
    level_update_factor: f64,
    no_dx_master: bool,
) -> Result<Vec<OptimalSongEntry<'s>>, anyhow::Error> {
    let covered_scores = datas.iter().flat_map(|(_, data)| covered_scores(data));
    let mut res = vec![];
    for score in covered_scores {
        if no_dx_master && is_dx_master(score) {
            continue;
        }
        let candidates = estimator.get(score).with_context(|| {
            format!("Key not found in estimator (in get_optimal_song): {score}")
        })?;
        if candidates.candidates().is_unique() {
            continue;
        }
        let old_constants = score
            .for_version(
                MaimaiVersion::latest()
                    .previous()
                    .context("Current version has no previous version (!?)")?,
            )
            .and_then(|score| score.level())
            .unwrap_or_else(|| {
                println!("Warning: constans cannot be rertrieved: {score}");
                InternalScoreLevel::empty()
            });
        let mut factor_sum = 0f64;
        let mut weighted_count_sum = 0f64;
        for constant in candidates.candidates().candidates() {
            let factor = if old_constants.is_empty() {
                1.
            } else {
                old_constants
                    .candidates()
                    .map(|c| {
                        level_update_factor
                            .powi(-((u8::from(c)).abs_diff(u8::from(constant)) as i32))
                    })
                    .sum()
            };

            let mut estimator = estimator.clone();
            let count_before = estimator.num_determined_scores();
            // Error shuold not occur at this stage
            estimator
                .set(
                    score,
                    |x| x == constant,
                    internal_lv_estimator::Reason::Assumption,
                )
                .with_context(|| format!("When assuming {score} to be {constant}"))?;
            multi_user::estimate_all(datas, &mut estimator)?;
            let count_determined_anew = estimator.num_determined_scores() - count_before;
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
        res.push(OptimalSongEntry {
            expected_count,
            score,
            old_constants,
            new_constants: candidates.candidates(),
        });
    }
    res.sort_by_key(|x| OrderedFloat(-x.expected_count));
    Ok(res)
}

fn covered_scores<'s>(
    data: &'s UserDataOrdinaryAssociated,
    // store: &ScoreConstantsStore<'s>,
) -> BTreeSet<OrdinaryScoreRef<'s>> {
    data.rating_target()
        .iter()
        .filter_map(move |(k, v)| (MaimaiVersion::latest().start_time() <= k.get()).then_some(v))
        .flat_map(|r| {
            [
                r.target_new(),
                r.target_old(),
                r.candidates_new(),
                r.candidates_old(),
            ]
            .into_iter()
            .flatten()
        })
        .filter(|entry| entry.data().achievement().get() >= 80_0000)
        .map(|entry| entry.score().score())
        .collect()
}

#[derive(Clone)]
struct OptimalSongEntry<'s> {
    expected_count: f64,
    score: OrdinaryScoreRef<'s>,
    old_constants: InternalScoreLevel,
    new_constants: InternalScoreLevel,
}

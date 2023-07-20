use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Display,
};

use crate::{
    algorithm::possibilties_from_sum_and_ordering,
    maimai::{
        load_score_level::{InternalScoreLevel, MaimaiVersion, RemovedSong, Song},
        rating::{rank_coef, single_song_rating, ScoreConstant},
        rating_target_parser::{RatingTargetEntry, RatingTargetFile},
        schema::{
            latest::{PlayRecord, PlayTime, ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
            ver_20210316_2338::RatingValue,
        },
    },
};
use anyhow::{bail, Context};
use either::Either;
use itertools::{chain, Itertools};
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use strum::IntoEnumIterator;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct ScoreKey<'a> {
    icon: &'a SongIcon,
    generation: ScoreGeneration,
    difficulty: ScoreDifficulty,
}
impl<'a> From<&'a PlayRecord> for ScoreKey<'a> {
    fn from(record: &'a PlayRecord) -> Self {
        Self {
            icon: record.song_metadata().cover_art(),
            generation: record.score_metadata().generation(),
            difficulty: record.score_metadata().difficulty(),
        }
    }
}

pub struct ScoreConstantsStore<'s, 'r> {
    pub updated: bool,
    constants: HashMap<ScoreKey<'s>, ScoreConstantsEntry<'s>>,
    removed_songs: HashMap<&'s SongIcon, &'r RemovedSong>,
    song_name_to_icon: HashMap<&'s SongName, HashSet<&'s SongIcon>>,
    pub show_details: bool,
}
impl<'s, 'r> ScoreConstantsStore<'s, 'r> {
    pub fn new(
        map: HashMap<(&'s SongIcon, ScoreGeneration), &'s Song>,
        removed_songs: HashMap<&'s SongIcon, &'r RemovedSong>,
        song_name_to_icon: HashMap<&'s SongName, HashSet<&'s SongIcon>>,
    ) -> Self {
        Self {
            updated: false,
            constants: map
                .into_iter()
                .flat_map(|((icon, generation), song)| {
                    ScoreDifficulty::iter().filter_map(move |difficulty| {
                        let key = ScoreKey {
                            icon,
                            generation,
                            difficulty,
                        };
                        let (candidates, reason) = match song.levels().get(difficulty)? {
                            InternalScoreLevel::Unknown(level) => {
                                let levels = level.score_constant_candidates().collect();
                                let reason = lazy_format!("because this score's level is {level}");
                                (levels, Either::Left(reason))
                            }
                            InternalScoreLevel::Known(level) => {
                                (vec![level], Either::Right("as it is already known"))
                            }
                        };
                        let mut entry = ScoreConstantsEntry {
                            song,
                            candidates,
                            reasons: vec![],
                        };
                        entry.add_reason(reason);
                        Some((key, entry))
                    })
                })
                .collect(),
            removed_songs,
            song_name_to_icon,
            show_details: false,
        }
    }

    pub fn reset(&mut self) {
        self.updated = false;
    }

    fn get(&self, key: ScoreKey<'s>) -> anyhow::Result<Option<(&'s Song, &[ScoreConstant])>> {
        if self.removed_songs.contains_key(key.icon) {
            return Ok(None);
        }
        match self.constants.get(&key) {
            Some(entry) => Ok(Some((entry.song, &entry.candidates))),
            None => bail!("No score constant entry was found for {key:?}"),
        }
    }

    fn set(
        &mut self,
        key: ScoreKey<'s>,
        new: impl Iterator<Item = ScoreConstant>,
        reason: impl Display,
    ) -> anyhow::Result<()> {
        let entry = self.constants.get_mut(&key).unwrap();
        let old_len = entry.candidates.len();

        let new: BTreeSet<_> = new.collect();
        entry.candidates.retain(|x| new.contains(x));

        if entry.candidates.len() < old_len {
            self.updated = true;
            entry.add_reason(reason);
            let print_reasons = || {
                for reason in &entry.reasons {
                    println!("    - {reason}");
                }
            };
            let song = &entry.song;
            let score_name = lazy_format!(
                "{} ({:?} {:?})",
                song.song_name(),
                key.generation,
                key.difficulty,
            );
            match entry.candidates[..] {
                [] => {
                    let message = lazy_format!("No more candidates for {score_name} :(");
                    if self.show_details {
                        println!("  {message}");
                        print_reasons();
                    }
                    bail!("{message}");
                }
                [determined] => {
                    if self.show_details {
                        println!("  Internal level determined! {score_name}: {determined}");
                        print_reasons();
                    } else {
                        println!("{score_name}: {determined}");
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn key_from_target_entry(
        &self,
        entry: &RatingTargetEntry,
    ) -> anyhow::Result<Option<ScoreKey<'s>>> {
        match self.song_name_to_icon.get(entry.song_name()) {
            None => bail!("Unknown song: {:?}", entry.song_name()),
            Some(icons) => Ok((icons.len() == 1).then(|| {
                let m = entry.score_metadata();
                ScoreKey {
                    icon: icons.iter().next().unwrap(),
                    generation: m.generation(),
                    difficulty: m.difficulty(),
                }
            })),
        }
    }
}

struct ScoreConstantsEntry<'s> {
    song: &'s Song,
    candidates: Vec<ScoreConstant>,
    reasons: Vec<String>,
}
impl ScoreConstantsEntry<'_> {
    fn add_reason(&mut self, reason: impl Display) {
        self.reasons.push(format!(
            "Constrained to [{}] {reason}",
            self.candidates.iter().join_with(", ")
        ))
    }
}

pub fn analyze_new_songs<'s>(
    records: &'s [PlayRecord],
    levels: &mut ScoreConstantsStore<'s, '_>,
) -> anyhow::Result<()> {
    let version = MaimaiVersion::latest();
    let start_time: PlayTime = version.start_time().into();
    let mut r2s = BTreeSet::<(i16, _)>::new();
    let mut s2r = HashMap::<_, i16>::new();
    let mut key_to_record = HashMap::new();
    for record in records
        .iter()
        .filter(|r| start_time <= r.played_at().time())
    {
        let score_key = ScoreKey::from(record);
        let Some((song, _)) = levels.get(score_key)? else { continue };
        let delta = record.rating_result().delta();
        if song.version() == version && delta > 0 {
            use std::collections::hash_map::Entry::*;
            let rating = match s2r.entry(score_key) {
                Occupied(mut s2r_entry) => {
                    // println!("  Song list does not change, just updating score (delta={delta})");
                    let rating = s2r_entry.get_mut();
                    assert!(r2s.remove(&(*rating, score_key)));
                    *rating += delta;
                    assert!(r2s.insert((*rating, score_key)));
                    *rating
                }
                Vacant(s2r_entry) => {
                    if r2s.len() == 15 {
                        // println!("  Removing the song with lowest score & inserting new one instead (delta={delta})");
                        let (removed_rating, removed_key) = r2s.pop_first().unwrap();
                        // println!("    Removed={}", removed_rating);
                        let new_rating = removed_rating + delta;
                        assert!(r2s.insert((new_rating, score_key)));
                        s2r_entry.insert(new_rating);
                        assert!(s2r.remove(&removed_key).is_some());
                        new_rating
                    } else {
                        // Just insert the new song
                        s2r_entry.insert(delta);
                        assert!(r2s.insert((delta, score_key)));
                        delta
                    }
                }
            };
            key_to_record.insert(score_key, record);

            let a = record.achievement_result().value();
            levels.set(
                score_key,
                ScoreConstant::candidates().filter(|&level| {
                    single_song_rating(level, a, rank_coef(a)).get() as i16 == rating
                }),
                lazy_format!(
                    "beacuse record played at {} determines the single-song rating to be {rating}",
                    record.played_at().time()
                ),
            )?;
        }
    }
    // println!("Current best");
    // for (rating, key) in r2s.iter().rev() {
    //     let record = key_to_record[key];
    //     println!(
    //         "{rating:3}  {} {:?}",
    //         record.song_metadata().name(),
    //         record.score_metadata()
    //     );
    // }
    // println!("Sum: {}", r2s.iter().map(|x| x.0).sum::<i16>());
    Ok(())
}

fn single_song_rating_for_target_entry(
    level: ScoreConstant,
    entry: &RatingTargetEntry,
) -> RatingValue {
    let a = entry.achievement();
    single_song_rating(level, a, rank_coef(a))
}

pub fn guess_from_rating_target_order(
    rating_targets: &RatingTargetFile,
    levels: &mut ScoreConstantsStore,
) -> anyhow::Result<()> {
    for (&play_time, list) in rating_targets {
        // Process old songs
        // First, find the sum of single song ratings of old songs
        let mut new_song_raing_sum = 0;
        for entry in list.target_new() {
            let Some(key) = levels.key_from_target_entry(entry)? else {
                println!(
                    "TODO: score cannot be uniquely determined from the song name {:?}",
                    entry.song_name()
                );
                return Ok(());
            };
            let levels = levels.get(key)?.unwrap().1;
            if levels.len() != 1 {
                panic!("Score constants of new songs must be determined!");
            }
            new_song_raing_sum += single_song_rating_for_target_entry(levels[0], entry).get();
        }
        // Then, solve the DP
        let old_target_len = list.target_old().len();
        solve_target_order(
            levels,
            play_time,
            chain(list.target_old(), list.candidates_old()),
            |i, rating| rating * (i < old_target_len) as usize,
            list.rating()
                .get()
                .checked_sub(new_song_raing_sum)
                .context("new_song_raing_sum is greater than rating value")? as usize,
        )?;

        // Process new songs
        // candidates > 0 => target > 0 should hold
        // !(x => y) <=> !(!x || y) <=> x && !y
        if !list.candidates_new().is_empty() && list.target_new().is_empty() {
            bail!("Candidates are non-empty, but targets are empty");
        }
        solve_target_order(
            levels,
            play_time,
            chain(list.target_new().last(), list.candidates_new()),
            |_, _| 0,
            0,
        )?;
    }
    Ok(())
}

fn solve_target_order<'a>(
    levels: &mut ScoreConstantsStore,
    list_time: PlayTime,
    entries: impl Iterator<Item = &'a RatingTargetEntry>,
    score: impl Fn(usize, usize) -> usize,
    sum: usize,
) -> anyhow::Result<()> {
    let entries = entries.collect_vec();

    let mut keys = vec![];
    for entry in &entries {
        let Some(key) = levels.key_from_target_entry(entry)? else {
                println!(
                    "TODO: score cannot be uniquely determined from the song name {:?}",
                    entry.song_name()
                );
                return Ok(());
            };
        if levels.get(key)?.is_none() {
            bail!("Song not found for {key:?}");
        }
        keys.push(key);
    }
    // println!("{} - {}", list.rating(), new_song_raing_sum);
    let res = possibilties_from_sum_and_ordering::solve(
        entries.len(),
        |i| {
            let (key, entry) = (keys[i], entries[i]);
            let levels = levels.get(key).unwrap().unwrap().1.iter();
            let score = &score;
            levels.map(move |&level| {
                let rating = single_song_rating_for_target_entry(level, entry).get() as usize;
                (score(i, rating), ((rating, entry.achievement()), level))
            })
        },
        |x, y| x.1 .0.cmp(&y.1 .0).reverse(),
        sum,
    );
    for (&key, res) in keys.iter().zip_eq(res) {
        levels.set(
            key,
            res.iter().map(|x| x.1 .1),
            lazy_format!("by the rating target list on {list_time}"),
        )?;
    }
    Ok(())
}

#[allow(unused)]
pub fn analyze_old_songs(
    records: &[PlayRecord],
    levels: &HashMap<(&SongIcon, ScoreGeneration), &Song>,
    removed_songs: &HashMap<&SongIcon, &RemovedSong>,
) -> anyhow::Result<()> {
    let mut best = HashMap::<_, &PlayRecord>::new();
    for record in records {
        use std::collections::hash_map::Entry::*;
        match best.entry((record.song_metadata().cover_art(), record.score_metadata())) {
            Occupied(mut old) => {
                if old.get().achievement_result().value() < record.achievement_result().value() {
                    *old.get_mut() = record;
                }
            }
            Vacant(entry) => {
                entry.insert(record);
            }
        }
    }

    let mut bests = vec![];
    for ((icon, score_metadata), record) in best {
        if removed_songs.contains_key(icon) {
            continue;
        }
        let song = levels
            .get(&(icon, score_metadata.generation()))
            .with_context(|| {
                format!(
                    "Unknown song: {:?} {:?}",
                    record.song_metadata().name(),
                    record.score_metadata().generation()
                )
            })?;
        let level = song
            .levels()
            .get(score_metadata.difficulty())
            .with_context(|| {
                format!(
                    "Song not found: {:?} {:?}",
                    record.song_metadata(),
                    record.score_metadata()
                )
            })?;
        let a = record.achievement_result().value();
        let c = rank_coef(a);
        let (min, max) = match level {
            InternalScoreLevel::Unknown(lv) => {
                let lvs = lv.score_constant_candidates();
                let min = single_song_rating(lvs.clone().next().unwrap(), a, c);
                let max = single_song_rating(lvs.clone().last().unwrap(), a, c);
                (min, max)
            }
            InternalScoreLevel::Known(lv) => {
                let val = single_song_rating(lv, a, c);
                (val, val)
            }
        };
        bests.push((record, min, max));
        // println!(
        //     "{} ({:?} {:?}) => [{min}, {max}]",
        //     record.song_metadata().name(),
        //     record.score_metadata().generation(),
        //     record.score_metadata().difficulty()
        // );
    }

    // bests.sort_by_key(|x| (x.2, x.1));
    // for (i, (a, b, c)) in bests.iter().rev().enumerate().take(50) {
    //     println!(
    //         "{i:2}  {b} {c}   {} ({:?} {:?})",
    //         a.song_metadata().name(),
    //         a.score_metadata().generation(),
    //         a.score_metadata().difficulty()
    //     );
    // }

    Ok(())
}

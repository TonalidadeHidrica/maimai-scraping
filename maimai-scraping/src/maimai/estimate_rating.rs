use std::{
    collections::BTreeSet,
    fmt::Display,
    hash::{BuildHasher, Hash, Hasher},
};

use crate::{
    algorithm::possibilties_from_sum_and_ordering,
    maimai::{
        load_score_level::{InternalScoreLevel, MaimaiVersion, RemovedSong, Song},
        rating::{rank_coef, single_song_rating, ScoreConstant},
        rating_target_parser::{RatingTargetEntry, RatingTargetFile},
        schema::{
            latest::{
                AchievementValue, PlayRecord, PlayTime, ScoreDifficulty, ScoreGeneration, SongIcon,
                SongName,
            },
            ver_20210316_2338::RatingValue,
        },
    },
};
use anyhow::{bail, Context};
use either::Either;
use getset::Getters;
use hashbrown::{HashMap, HashSet};
use itertools::{chain, Itertools};
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::warn;
use strum::IntoEnumIterator;

use super::{
    load_score_level::{self, make_hash_multimap},
    schema::latest::{AchievementRank, ScoreMetadata},
};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ScoreKey<'a> {
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
impl<'a> ScoreKey<'a> {
    fn with<'b>(&self, icon: &'b SongIcon) -> ScoreKey<'b> {
        ScoreKey {
            icon,
            generation: self.generation,
            difficulty: self.difficulty,
        }
    }
    pub fn score_metadata(&self) -> ScoreMetadata {
        ScoreMetadata::builder()
            .generation(self.generation)
            .difficulty(self.difficulty)
            .build()
    }
}

#[derive(Getters)]
pub struct ScoreConstantsStore<'s, 'r> {
    #[getset(get = "pub")]
    events: Vec<(ScoreKey<'s>, String)>,
    constants: HashMap<ScoreKey<'s>, ScoreConstantsEntry<'s>>,
    removed_songs: HashMap<&'r SongIcon, &'r RemovedSong>,
    song_name_to_icon: HashMap<&'s SongName, HashSet<&'s SongIcon>>,
    pub show_details: bool,
}
impl<'s, 'r> ScoreConstantsStore<'s, 'r> {
    pub fn new(levels: &'s [Song], removed_songs: &'r [RemovedSong]) -> anyhow::Result<Self> {
        let song_name_to_icon =
            make_hash_multimap(levels.iter().map(|song| (song.song_name(), song.icon())));
        let removed_songs = load_score_level::make_map(removed_songs, |r| r.icon())?;
        let map = load_score_level::make_map(levels, |song| (song.icon(), song.generation()))?;

        let mut events = vec![];
        let mut constants = HashMap::new();
        for ((icon, generation), song) in map {
            for difficulty in ScoreDifficulty::iter() {
                let key = ScoreKey {
                    icon,
                    generation,
                    difficulty,
                };
                let (candidates, reason) = match song.levels().get(difficulty) {
                    None => continue,
                    Some(InternalScoreLevel::Unknown(level)) => {
                        let levels = level.score_constant_candidates().collect();
                        let reason = lazy_format!("because this score's level is {level}");
                        (levels, Either::Left(reason))
                    }
                    Some(InternalScoreLevel::Known(level)) => {
                        (vec![level], Either::Right("as it is already known"))
                    }
                };
                let mut entry = ScoreConstantsEntry {
                    song,
                    candidates,
                    reasons: vec![],
                };
                entry.reasons.push(events.len());
                events.push((key, entry.make_reason(reason)));
                constants.insert(key, entry);
            }
        }

        Ok(Self {
            events,
            constants,
            removed_songs,
            song_name_to_icon,
            show_details: false,
        })
    }

    pub fn get(&self, key: ScoreKey) -> anyhow::Result<Option<(&'s Song, &[ScoreConstant])>> {
        if self.removed_songs.contains_key(key.icon) {
            return Ok(None);
        }
        let hash = compute_hash(self.constants.hasher(), &key);
        match self.constants.raw_entry().from_hash(hash, |x| x == &key) {
            Some((_, entry)) => Ok(Some((entry.song, &entry.candidates))),
            None => bail!("No score constant entry was found for {key:?}"),
        }
    }

    fn set(
        &mut self,
        key: ScoreKey,
        new: impl Iterator<Item = ScoreConstant>,
        reason: impl Display,
    ) -> anyhow::Result<()> {
        let hash = compute_hash(self.constants.hasher(), &key);
        let hashbrown::hash_map::RawEntryMut::Occupied(mut entry) = self
            .constants
            .raw_entry_mut()
            .from_hash(hash, |x| x == &key)
        else {
           bail!("No score constant entry was found for {key:?}")
        };
        let entry = entry.get_mut();
        let old_len = entry.candidates.len();

        let new: BTreeSet<_> = new.collect();
        entry.candidates.retain(|x| new.contains(x));
        // println!("new = {new:?}");

        if entry.candidates.len() < old_len {
            entry.reasons.push(self.events.len());
            self.events
                .push((key.with(entry.song.icon()), entry.make_reason(reason)));
            let print_reasons = || {
                for &i in &entry.reasons {
                    let (_, reason) = &self.events[i];
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
    reasons: Vec<usize>,
}
impl ScoreConstantsEntry<'_> {
    fn make_reason(&mut self, reason: impl Display) -> String {
        format!(
            "Constrained to [{}] {reason}",
            self.candidates.iter().join_with(", ")
        )
    }
}

fn single_song_rating_for_target_entry(
    level: ScoreConstant,
    entry: &RatingTargetEntry,
) -> RatingValue {
    let a = entry.achievement();
    single_song_rating(level, a, rank_coef(a))
}

impl<'s> ScoreConstantsStore<'s, '_> {
    pub fn do_everything<'r>(
        &mut self,
        records: impl IntoIterator<Item = &'r PlayRecord> + Clone,
        rating_targets: &RatingTargetFile,
    ) -> anyhow::Result<()> {
        if self.show_details {
            println!("New songs");
        }
        self.analyze_new_songs(records.clone())?;
        for i in 1.. {
            if self.show_details {
                println!("Iteration {i}");
            }
            let before_len = self.events().len();
            self.guess_from_rating_target_order(rating_targets)?;
            self.records_not_in_targets(records.clone(), rating_targets)?;
            if before_len == self.events().len() {
                break;
            }
        }
        Ok(())
    }

    pub fn analyze_new_songs<'r>(
        &mut self,
        records: impl IntoIterator<Item = &'r PlayRecord>,
    ) -> anyhow::Result<()> {
        let version = MaimaiVersion::latest();
        let start_time: PlayTime = version.start_time().into();
        let mut r2s = BTreeSet::<(i16, _)>::new();
        let mut s2r = HashMap::<_, i16>::new();
        let mut key_to_record = HashMap::new();
        for record in records
            .into_iter()
            .filter(|r| start_time <= r.played_at().time())
        {
            let score_key = ScoreKey::from(record);
            let Some((song, _)) = self.get(score_key)? else { continue };
            let delta = record.rating_result().delta();
            if song.version() == version && delta > 0 {
                use hashbrown::hash_map::Entry::*;
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
                self.set(
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

    pub fn guess_from_rating_target_order(
        &mut self,
        rating_targets: &RatingTargetFile,
    ) -> anyhow::Result<()> {
        for (&play_time, list) in rating_targets {
            let mut sub_list = vec![];
            #[derive(Clone, Copy, Debug)]
            struct Entry<'a, 'k> {
                new: bool,
                contributes_to_sum: bool,
                rating_target_entry: &'a RatingTargetEntry,
                key: ScoreKey<'k>,
                levels: &'a [ScoreConstant],
            }
            for (new, contributes_to_sum, entries) in [
                (true, true, list.target_new()),
                (true, false, list.candidates_new()),
                (false, true, list.target_old()),
                (false, false, list.candidates_old()),
            ] {
                for rating_target_entry in entries {
                    let Some(key) = self.key_from_target_entry(rating_target_entry)? else {
                        println!(
                            "TODO: score cannot be uniquely determined from the song name {:?}",
                            rating_target_entry.song_name(),
                        );
                        return Ok(());
                    };
                    let levels = self.get(key)?.context("Song must not be removed")?.1;
                    sub_list.push(Entry {
                        new,
                        contributes_to_sum,
                        rating_target_entry,
                        key,
                        levels,
                    });
                }
            }
            #[derive(Clone, Copy)]
            struct DpElement<'a, 'k> {
                level: ScoreConstant,
                single_song_rating: usize,
                entry: Entry<'a, 'k>,
            }
            impl DpElement<'_, '_> {
                fn tuple(self) -> (bool, usize, AchievementValue) {
                    (
                        self.entry.new,
                        self.single_song_rating,
                        self.entry.rating_target_entry.achievement(),
                    )
                }
            }
            impl std::fmt::Debug for DpElement<'_, '_> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let (new, score, a) = self.tuple();
                    write!(f, "({new}, {score}, {a})")
                }
            }
            let res = possibilties_from_sum_and_ordering::solve(
                sub_list.len(),
                |i| {
                    let entry = sub_list[i];
                    entry.levels.iter().map(move |&level| {
                        let single_song_rating =
                            single_song_rating_for_target_entry(level, entry.rating_target_entry)
                                .get() as usize;
                        let score = entry.contributes_to_sum as usize * single_song_rating;
                        let element = DpElement {
                            level,
                            entry,
                            single_song_rating,
                        };
                        (score, element)
                    })
                },
                |(_, x), (_, y)| x.tuple().cmp(&y.tuple()).reverse(),
                list.rating().get() as usize,
            );
            let keys = sub_list.iter().map(|e| e.key).collect_vec();
            let res = res
                .iter()
                .map(|res| res.iter().map(|x| x.1.level).collect_vec())
                .collect_vec();
            for (&key, res) in keys.iter().zip(res) {
                let reason = lazy_format!("by the rating target list on {play_time}");
                self.set(key, res.into_iter(), reason)?;
            }
        }
        Ok(())
    }

    pub fn records_not_in_targets<'r>(
        &mut self,
        records: impl IntoIterator<Item = &'r PlayRecord>,
        rating_targets: &RatingTargetFile,
    ) -> anyhow::Result<()> {
        let version = MaimaiVersion::latest();
        // Assumption: records are in ascending order
        // (otherwise, this code will be inefficient, if working)
        let mut last_inspected: Option<(PlayTime, HashSet<ScoreKey>)> = None;
        'next_record: for record in records {
            let score_key = ScoreKey::from(record);
            // Ignore removed songs
            let Some((song, _)) = self.get(score_key)? else { continue };
            let Some((&target_time, list)) =
                    rating_targets.range(record.played_at().time()..).next() else {
                warn!(
                    "Rating target not collected for a record played at {}",
                    record.played_at().time()
                );
                continue;
            };
            let contained = match last_inspected.as_ref().filter(|l| l.0 == target_time) {
                Some((_, x)) => x,
                None => {
                    let mut set = HashSet::new();
                    for entry in chain(list.target_new(), list.target_old())
                        .chain(chain(list.candidates_new(), list.candidates_old()))
                    {
                        let Some(key) = self.key_from_target_entry(entry)? else {
                            println!(
                                "TODO: score cannot be uniquely determined from the song name {:?}",
                                entry.song_name()
                            );
                            continue 'next_record;
                        };
                        set.insert(key);
                    }
                    &last_inspected.insert((target_time, set)).1
                }
            };
            if contained.contains(&score_key) {
                continue;
            }

            let border_entry = if song.version() == version {
                list.target_new()
                    .last()
                    .context("New songs must be contained (1)")?
            } else {
                list.target_old()
                    .last()
                    .context("New songs must be contained (1)")?
            };
            let min_entry = if song.version() == version {
                (list.candidates_new().last())
                    .or_else(|| list.target_new().last())
                    .context("New songs must be contained")?
            } else {
                (list.candidates_old().last())
                    .or_else(|| list.target_old().last())
                    .context("Old songs must be contained")?
            };

            let compute = |entry: &RatingTargetEntry| {
                // println!("min = {min:?}");
                let a = entry.achievement();
                let lvs = &self
                    .get(
                        self.key_from_target_entry(entry)?
                            .context("Must not be removed (1)")?,
                    )?
                    .context("Must not be removed (2)")?
                    .1;
                // println!("min_constans = {min_constants:?}");
                let score = lvs
                    .iter()
                    .map(|&level| single_song_rating(level, a, rank_coef(a)))
                    .max()
                    .context("Empty level candidates")?;
                anyhow::Ok((score, a))
            };
            let border_pair = compute(border_entry)?;
            let min_pair = compute(min_entry)?;

            let this_a = record.achievement_result().value();
            let this_sssplus = record.achievement_result().rank() == AchievementRank::SSSPlus;
            let candidates = ScoreConstant::candidates().filter(|&level| {
                let this = single_song_rating(level, this_a, rank_coef(this_a));
                let this_pair = (this, this_a);
                // println!("{:?} {level} {:?}", (min_entry, min_a), (this, this_a));
                min_pair >= this_pair || this_sssplus && border_pair >= this_pair
            });
            let message = lazy_format!(
                "because record played at {} achieving {} is not in list at {}, so it's below {:?}",
                record.played_at().time(),
                this_a,
                target_time,
                if this_sssplus { border_pair } else { min_pair },
            );
            self.set(score_key, candidates, message)?;
        }
        Ok(())
    }
}

#[allow(unused)]
pub fn analyze_old_songs(
    records: &[PlayRecord],
    levels: &HashMap<(&SongIcon, ScoreGeneration), &Song>,
    removed_songs: &HashMap<&SongIcon, &RemovedSong>,
) -> anyhow::Result<()> {
    let mut best = HashMap::<_, &PlayRecord>::new();
    for record in records {
        use hashbrown::hash_map::Entry::*;
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

fn compute_hash<K: Hash + ?Sized, S: BuildHasher>(hash_builder: &S, key: &K) -> u64 {
    let mut state = hash_builder.build_hasher();
    key.hash(&mut state);
    state.finish()
}

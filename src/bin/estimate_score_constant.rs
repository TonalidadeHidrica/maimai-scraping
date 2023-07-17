use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context};
use chrono::NaiveTime;
use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        load_score_level::{self, InternalScoreLevel, MaimaiVersion, RemovedSong, Song},
        rating::{rank_coef, single_song_rating, ScoreConstant},
        schema::latest::{PlayRecord, PlayTime, ScoreDifficulty, ScoreGeneration, SongIcon},
    },
};
use strum::IntoEnumIterator;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> = read_json(opts.input_file)?;

    let levels = load_score_level::load(opts.level_file)?;
    let levels = load_score_level::make_map(&levels, |song| (song.icon(), song.generation()))?;

    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let removed_songs = load_score_level::make_map(&removed_songs, |r| r.icon())?;

    let mut levels = ScoreConstantsStore::new(levels, removed_songs);
    levels.reset();
    analyze_new_songs(&records, &mut levels)?;
    // analyze_old_songs(&records, &mut levels, &removed_songs)?;

    Ok(())
}

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

struct ScoreConstantsStore<'s, 'r> {
    updated: bool,
    constants: HashMap<ScoreKey<'s>, (&'s Song, Vec<ScoreConstant>)>,
    removed_songs: HashMap<&'s SongIcon, &'r RemovedSong>,
}
impl<'s, 'r> ScoreConstantsStore<'s, 'r> {
    fn new(
        map: HashMap<(&'s SongIcon, ScoreGeneration), &'s Song>,
        removed_songs: HashMap<&'s SongIcon, &'r RemovedSong>,
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
                        let levels = match song.levels().get(difficulty)? {
                            InternalScoreLevel::Unknown(level) => {
                                level.score_constant_candidates().collect()
                            }
                            InternalScoreLevel::Known(level) => vec![level],
                        };
                        Some((key, (song, levels)))
                    })
                })
                .collect(),
            removed_songs,
        }
    }

    fn reset(&mut self) {
        self.updated = false;
    }

    fn get(&self, key: ScoreKey<'s>) -> anyhow::Result<Option<(&'s Song, &[ScoreConstant])>> {
        if self.removed_songs.contains_key(key.icon) {
            return Ok(None);
        }
        match self.constants.get(&key) {
            Some((x, y)) => Ok(Some((*x, &y[..]))),
            None => bail!("No score constant entry was found for {key:?}"),
        }
    }

    fn set(
        &mut self,
        key: ScoreKey<'s>,
        new: impl Iterator<Item = ScoreConstant>,
    ) -> anyhow::Result<()> {
        let (song, levels) = self.constants.get_mut(&key).unwrap();
        let old_len = levels.len();
        let new: BTreeSet<_> = new.collect();
        levels.retain(|x| new.contains(x));
        if levels.is_empty() {
            bail!("No more candidates! {:?} {key:?}", song.song_name());
        }
        if levels.len() < old_len {
            self.updated = true;
            if levels.len() == 1 {
                println!(
                    "Internal level determined! {} ({:?} {:?}): {}",
                    song.song_name(),
                    key.generation,
                    key.difficulty,
                    levels[0]
                );
            }
        }
        Ok(())
    }
}

fn analyze_new_songs<'s>(
    records: &'s [PlayRecord],
    levels: &mut ScoreConstantsStore<'s, '_>,
) -> anyhow::Result<()> {
    let version = MaimaiVersion::latest();
    let start_time: PlayTime = version
        .start_date()
        .and_time(NaiveTime::from_hms(5, 0, 0))
        .into();
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

#[allow(unused)]
fn analyze_old_songs(
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
                anyhow!(
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

#[allow(unused)]
struct MultiBTreeSet<T> {
    map: BTreeMap<T, usize>,
    len: usize,
}

#[allow(unused)]
impl<T: Ord> MultiBTreeSet<T> {
    fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn insert(&mut self, t: T) {
        self.len += 1;
        *self.map.entry(t).or_default() += 1;
    }
    fn remove(&mut self, t: T) -> bool {
        use std::collections::btree_map::*;
        self.len -= 1;
        match self.map.entry(t) {
            Entry::Vacant(_) => false,
            Entry::Occupied(mut e) => {
                *e.get_mut() -= 1;
                if *e.get() == 0 {
                    e.remove_entry();
                }
                true
            }
        }
    }
}
#[allow(unused)]
impl<T: Ord + Clone> MultiBTreeSet<T> {
    fn pop_first(&mut self) -> Option<T> {
        self.len -= 1;
        let mut first = self.map.first_entry()?;
        *first.get_mut() -= 1;
        Some(if *first.get() == 0 {
            first.remove_entry().0
        } else {
            first.key().clone()
        })
    }
}

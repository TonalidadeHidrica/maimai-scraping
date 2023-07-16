use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::BufReader,
    path::PathBuf,
};

use anyhow::{anyhow, bail, Context};
use chrono::NaiveTime;
use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::{
    load_score_level::{self, InternalScoreLevel, MaimaiVersion, RemovedSong, Song},
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::{PlayRecord, ScoreGeneration},
};
use url::Url;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
}

fn main() -> anyhow::Result<()> {
    // TODO: use rank coeffieicnts for appropriate versions
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.input_file)?))?;

    let levels = load_score_level::load(opts.level_file)?;
    let levels = load_score_level::make_map(&levels, |song| (song.icon(), song.generation()))?;

    let removed_songs: Vec<RemovedSong> =
        serde_json::from_reader(BufReader::new(File::open(opts.removed_songs)?))?;
    let removed_songs = load_score_level::make_map(&removed_songs, |r| r.icon())?;

    // analyze_new_songs(&records, &levels)?;
    analyze_old_songs(&records, &levels, &removed_songs)?;

    Ok(())
}

#[allow(unused)]
fn analyze_new_songs(
    records: &[PlayRecord],
    levels: &HashMap<(&Url, ScoreGeneration), &Song>,
) -> anyhow::Result<()> {
    let version = MaimaiVersion::latest();
    let start_time = version.start_date().and_time(NaiveTime::from_hms(5, 0, 0));
    let mut r2s = BTreeSet::<(i16, _)>::new();
    let mut s2r = HashMap::<_, i16>::new();
    let mut key_to_record = HashMap::new();
    for record in records
        .iter()
        .filter(|r| start_time <= r.played_at().time())
    {
        let song_key = (
            record.song_metadata().cover_art(),
            record.score_metadata().generation(),
        );
        let song = levels.get(&song_key).with_context(|| {
            format!(
                "Song not found: {:?} {:?}",
                record.song_metadata(),
                record.score_metadata()
            )
        })?;
        let delta = record.rating_result().delta();
        if song.version() == version && delta > 0 {
            use std::collections::hash_map::Entry::*;
            let key = (song_key.0, record.score_metadata());
            let rating = match s2r.entry(key) {
                Occupied(mut s2r_entry) => {
                    // println!("  Song list does not change, just updating score (delta={delta})");
                    let rating = s2r_entry.get_mut();
                    assert!(r2s.remove(&(*rating, key)));
                    *rating += delta;
                    assert!(r2s.insert((*rating, key)));
                    *rating
                }
                Vacant(s2r_entry) => {
                    if r2s.len() == 15 {
                        // println!("  Removing the song with lowest score & inserting new one instead (delta={delta})");
                        let (removed_rating, removed_key) = r2s.pop_first().unwrap();
                        // println!("    Removed={}", removed_rating);
                        let new_rating = removed_rating + delta;
                        assert!(r2s.insert((new_rating, key)));
                        s2r_entry.insert(new_rating);
                        assert!(s2r.remove(&removed_key).is_some());
                        new_rating
                    } else {
                        // Just insert the new song
                        s2r_entry.insert(delta);
                        assert!(r2s.insert((delta, key)));
                        delta
                    }
                }
            };
            key_to_record.insert(key, record);

            let a = record.achievement_result().value();
            let estimated_levels = ScoreConstant::candidates()
                .filter(|&level| single_song_rating(level, a, rank_coef(a)).get() as i16 == rating)
                .collect_vec();
            match estimated_levels[..] {
                [] => bail!("Error: no possible levels!"),
                [estimated] => {
                    match song
                        .levels()
                        .get(record.score_metadata().difficulty())
                        .unwrap()
                        .known()
                    {
                        None => {
                            println!(
                                "Constant confirmed! {} ({:?} {:?}): {estimated}",
                                record.song_metadata().name(),
                                record.score_metadata().generation(),
                                record.score_metadata().difficulty(),
                            );
                        }
                        Some(known) if known != estimated => {
                            bail!("Conflict levels! Database: {known}, esimated: {estimated}");
                        }
                        _ => {}
                    }
                }
                _ => println!("Multiple candidates: {estimated_levels:?}"),
            }

            // println!(
            //     "{} {:?} {} => {rating} (Expected: {expected:?})",
            //     record.song_metadata().name(),
            //     record.score_metadata(),
            //     record.achievement_result().value(),
            // );
        }
    }
    println!("Current best");
    for (rating, key) in r2s.iter().rev() {
        let record = key_to_record[key];
        println!(
            "{rating:3}  {} {:?}",
            record.song_metadata().name(),
            record.score_metadata()
        );
    }
    println!("Sum: {}", r2s.iter().map(|x| x.0).sum::<i16>());
    Ok(())
}

fn analyze_old_songs(
    records: &[PlayRecord],
    levels: &HashMap<(&Url, ScoreGeneration), &Song>,
    removed_songs: &HashMap<&Url, &RemovedSong>,
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

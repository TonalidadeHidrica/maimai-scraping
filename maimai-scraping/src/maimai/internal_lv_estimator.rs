//! Throughout this module:
//! - Lifetime parameter `'s` refers to that of the song database.
//! - Type parameter `L` is the label for the source, i.e. play record / rating target list.
//!   `L` is used for debugging and must implement cheap `Copy`.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
};

use anyhow::{bail, Context};
use chrono::NaiveTime;
use derive_more::Display;
use hashbrown::HashMap;
use itertools::Itertools;
use joinery::JoinableIterator;
use log::warn;
use smallvec::{smallvec, SmallVec};

use crate::{
    algorithm::possibilties_from_sum_and_ordering,
    maimai::associated_user_data::RatingTargetEntryAssociated,
};

use super::{
    associated_user_data::{OrdinaryPlayRecordAssociated, RatingTargetList},
    load_score_level::{InternalScoreLevel, MaimaiVersion},
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::{AchievementValue, PlayTime},
    song_list::{
        database::{OrdinaryScoreForVersionRef, OrdinaryScoreRef, SongDatabase},
        RemoveState,
    },
};

type CandidateList = SmallVec<[ScoreConstant; 6]>;

/// See the [module doc](`self`) for the definition of type parameters `'s` and `L`.
pub struct Estimator<'s, L> {
    version: MaimaiVersion,
    map: HashMap<OrdinaryScoreRef<'s>, Candidates<'s>>,
    events: IndexedVec<Event<'s, L>>,
}

#[derive(Debug)]
struct Candidates<'s> {
    #[allow(unused)]
    score: OrdinaryScoreRef<'s>,
    candidates: CandidateList,
    reasons: Vec<usize>,
}

struct IndexedVec<T>(Vec<T>);
impl<T> IndexedVec<T> {
    fn push(&mut self, element: T) -> usize {
        self.0.push(element);
        self.0.len() - 1
    }
}

/// See the [module doc](`self`) for the definition of type parameters `'s` and `L`.
pub struct Event<'s, L> {
    #[allow(unused)]
    score: OrdinaryScoreRef<'s>,
    candidates: CandidateList,
    reason: Reason,
    user: Option<L>,
}
impl<L> Display for Event<'_, L>
where
    L: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Constrained to [{}] {}",
            self.candidates.iter().join_with(", "),
            self.reason
        )?;
        if let Some(user) = &self.user {
            write!(f, " (played by {user})")?;
        }
        Ok(())
    }
}
#[derive(Debug, Display)]
pub enum Reason {
    #[display(fmt = "according to the database which stores {_0:?}")]
    Database(InternalScoreLevel),
    #[display(
        fmt = "because the record played at {_0:?} achieving {_1} determines the single-song rating to be {_2}"
    )]
    Delta(PlayTime, AchievementValue, i16),
    #[display(fmt = "by the rating target list on {_0}")]
    List(PlayTime),
}

impl<'s, L> Estimator<'s, L> {
    pub fn new(database: &SongDatabase<'s>, version: MaimaiVersion) -> Self {
        // TODO check if version is supported

        let mut events = IndexedVec(vec![]);
        let map = database
            .all_scores_for_version(version)
            .map(|score| (score.score(), Candidates::new(&mut events, version, score)))
            .collect();

        Self {
            version,
            map,
            events,
        }
    }

    pub fn set(
        &mut self,
        score: OrdinaryScoreRef<'s>,
        predicate: impl Fn(ScoreConstant) -> bool,
        reason: Reason,
        user: Option<L>,
    ) -> anyhow::Result<()>
    where
        L: Display,
    {
        let candidates = self
            .map
            .get_mut(&score)
            .with_context(|| format!("The following score was not in the map: {score:?}"))?;
        let old_len = candidates.candidates.len();
        candidates.candidates.retain(|&mut v| predicate(v));
        if candidates.candidates.len() < old_len {
            candidates.reasons.push(self.events.push(Event {
                score,
                candidates: candidates.candidates.clone(),
                reason,
                user,
            }));
        }
        if candidates.candidates.is_empty() {
            bail!(
                "No more candidates for {score:?}: {}",
                candidates
                    .reasons
                    .iter()
                    .map(|&r| &self.events.0[r])
                    .join_with("; "),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NewOrOld {
    New,
    Old,
}
impl<'s, L> Estimator<'s, L>
where
    L: Copy + Display,
{
    pub fn determine_by_delta<'d>(
        &mut self,
        user: Option<L>,
        records: impl IntoIterator<Item = OrdinaryPlayRecordAssociated<'d, 's>>,
        new_or_old: NewOrOld,
        ignore_time: bool,
    ) -> anyhow::Result<()> {
        let start_time: PlayTime = self.version.start_time().into();
        let end_time: PlayTime = self.version.end_time().into();
        let mut r2s = BTreeSet::<(i16, _)>::new();
        let mut s2r = HashMap::<_, i16>::new();
        let mut key_to_record = HashMap::new();
        let max_count = match new_or_old {
            NewOrOld::New => 15,
            NewOrOld::Old => 35,
        };

        // TODO: Does it really work fine when `new_or_old == Old` (analyzing old songs)?
        // We should update `r2s` and `s2r` based on the records *before* the version starts,
        // but there is no such process!

        for record in records {
            if !(ignore_time
                || (start_time..end_time).contains(&record.record().played_at().time()))
            {
                continue;
            }

            let score = record.score().score();
            // TODO: I wish this property is guaranteed at the database level.
            let score_version = score
                .scores()
                .scores()
                .version
                .with_context(|| format!("No version associated to {score:?}"))?;

            // This block used to be here, but it turns out irrelevant.
            // Removed scores stay in the target even after they are removed;
            // so we do not have to worry about removed song here, in `determine_by_delta`.
            // The deltas are consistent even after such removals.
            //
            // let Some((song, _)) = self.get(score_key)? else {
            //     // This block is visited when the given score is removed.
            //     // Skipping it is problematic when this song is removed within the latest version,
            //     // i.e. removed after played.
            //     // To handle this issue, one has to update r2s and s2r,
            //     // which requires songs not included in them
            //     // (songs with lower score than target songs).
            //     // In all likelihood, such an event may happen only when analyzing old songs.
            //     //
            //     // Below is the justification of not implementing for my case.
            //     // For the main card, "old songs are complete" shall not be assumed.
            //     // For the sub cards, I assume that such songs are not played.
            //     continue;
            // };

            let delta = record.record().rating_result().delta();
            if !((matches!(new_or_old, NewOrOld::Old) ^ (score_version == self.version))
                && delta > 0)
            {
                continue;
            }

            use hashbrown::hash_map::Entry::*;
            let rating = match s2r.entry(score) {
                Occupied(mut s2r_entry) => {
                    // println!("  Song list does not change, just updating score (delta={delta})");
                    let rating = s2r_entry.get_mut();
                    assert!(r2s.remove(&(*rating, score)));
                    *rating += delta;
                    assert!(r2s.insert((*rating, score)));
                    *rating
                }
                Vacant(s2r_entry) => {
                    if r2s.len() == max_count {
                        // println!("  Removing the song with lowest score & inserting new one instead (delta={delta})");
                        let (removed_rating, removed_key) = r2s.pop_first().unwrap();
                        // println!("    Removed={}", removed_rating);
                        let new_rating = removed_rating + delta;
                        assert!(r2s.insert((new_rating, score)));
                        s2r_entry.insert(new_rating);
                        assert!(s2r.remove(&removed_key).is_some());
                        new_rating
                    } else {
                        // Just insert the new song
                        s2r_entry.insert(delta);
                        assert!(r2s.insert((delta, score)));
                        delta
                    }
                }
            };
            key_to_record.insert(score, record);

            self.register_single_song_rating(
                score,
                record.record().achievement_result().value(),
                user,
                rating,
                record.record().played_at().time(),
            )?;
        }

        Ok(())
    }

    pub fn register_single_song_rating(
        &mut self,
        score: OrdinaryScoreRef<'s>,
        a: AchievementValue,
        user: Option<L>,
        rating: i16,
        time: PlayTime,
    ) -> anyhow::Result<()> {
        self.set(
            score,
            |lv| single_song_rating(lv, a, rank_coef(a)).get() as i16 == rating,
            Reason::Delta(time, a, rating),
            user,
        )
    }

    pub fn guess_from_rating_target_order<'d>(
        &mut self,
        user: Option<L>,
        rating_targets: &BTreeMap<PlayTime, RatingTargetList<'d, 's>>,
        ignore_time: bool,
    ) -> anyhow::Result<()> {
        let start_time: PlayTime = self.version.start_time().into();
        let end_time: PlayTime = self.version.end_time().into();

        // Once a song is removed not because of major version update,
        // The rating sum is no longer reliable.
        // Every score key in our `map` is available at least once in this song,
        // so if it has remove date within the version range, it means a removal occurred.
        let removal_time = self
            .map
            .keys()
            .filter_map(|score| match score.scores().song().song().remove_state {
                RemoveState::Removed(date)
                    if (start_time.get().date()..end_time.get().date()).contains(&date) =>
                {
                    Some(date.and_time(NaiveTime::from_hms_opt(5, 0, 0).unwrap()))
                }
                _ => None,
            })
            .min();
        // let removal_time = self
        //     .removed_songs
        //     .iter()
        //     .map(|x| {
        //         x.1.date()
        //             .and_time(NaiveTime::from_hms_opt(5, 0, 0).unwrap())
        //     })
        //     .filter(|&x| start_time.get() < x && x < end_time.get())
        //     .min();

        // println!("{removal_time:?}");
        for (&play_time, list) in rating_targets
            .iter()
            .filter(|p| ignore_time || (start_time..end_time).contains(p.0))
        {
            let rating_sum_is_reliable =
                removal_time.map_or(true, |removal_time| play_time.get() < removal_time);
            // println!("{rating_sum_is_reliable}");
            let mut sub_list = vec![];
            #[derive(Clone, Copy)]
            struct Entry<'a, 'k> {
                new: bool,
                contributes_to_sum: bool,
                rating_target_entry: RatingTargetEntryAssociated<'a, 'k>,
                key: OrdinaryScoreRef<'k>,
                levels: &'a [ScoreConstant],
            }
            for (new, contributes_to_sum, entries) in [
                (true, true, list.target_new()),
                (true, false, list.candidates_new()),
                (false, true, list.target_old()),
                (false, false, list.candidates_old()),
            ] {
                for rating_target_entry in entries {
                    let rating_target_entry = match rating_target_entry.as_associated() {
                        Ok(entry) => entry,
                        Err(e) => {
                            warn!("Not unique: {:?}: {e:#}", rating_target_entry.data());
                            continue;
                        }
                    };
                    let score = rating_target_entry.score().score();
                    let levels = self.map.get(&score).with_context(|| {
                        format!(
                            "While procesing {:?}: no key matches {score:?}",
                            rating_target_entry.data(),
                        )
                    })?;
                    sub_list.push(Entry {
                        new,
                        contributes_to_sum,
                        rating_target_entry,
                        key: score,
                        levels: &levels.candidates,
                    });

                    // let Some((key, levels)) = self.levels_from_target_entry(rating_target_entry)?
                    // else {
                    //     return Ok(());
                    // };
                    // sub_list.push(Entry {
                    //     new,
                    //     contributes_to_sum,
                    //     rating_target_entry,
                    //     key,
                    //     levels,
                    // });
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
                        self.entry.rating_target_entry.data().achievement(),
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
                        let a = entry.rating_target_entry.data().achievement();
                        let single_song_rating =
                            single_song_rating(level, a, rank_coef(a)).get() as usize;
                        let score = entry.contributes_to_sum as usize
                            * single_song_rating
                            * rating_sum_is_reliable as usize;
                        let element = DpElement {
                            level,
                            entry,
                            single_song_rating,
                        };
                        (score, element)
                    })
                },
                |(_, x), (_, y)| x.tuple().cmp(&y.tuple()).reverse(),
                list.list().rating().get() as usize * rating_sum_is_reliable as usize,
            );
            let keys = sub_list.iter().map(|e| e.key).collect_vec();
            let res = res
                .iter()
                .map(|res| res.iter().map(|x| x.1.level).collect::<BTreeSet<_>>())
                .collect_vec();
            // println!("==== {} ====", play_time);
            // for (&elem, res) in sub_list.iter().zip(&res) {
            //     println!("{} {:?}", elem.rating_target_entry.song_name(), res);
            // }
            for (&key, res) in keys.iter().zip(res) {
                // let reason = lazy_format!(
                //     "by the rating target list on {play_time}{}",
                //     display_played_by(name),
                // );
                // self.set(key, res.into_iter(), reason)?;
                self.set(key, |lv| res.contains(&lv), Reason::List(play_time), user)?;
            }
        }
        Ok(())
    }

    // We could do this, but not to for now, as it is less significant now.
    //
    // pub fn records_not_in_targets<'d>(
    //     &mut self,
    //     user: Option<L>,
    //     records: impl IntoIterator<Item = OrdinaryPlayRecordAssociated<'d, 's>>,
    //     rating_targets: &BTreeMap<PlayTime, RatingTargetList<'d, 's>>,
    //     ignore_time: bool,
    // ) -> anyhow::Result<()> {
    //     let start_time: PlayTime = self.version.start_time().into();
    //     let end_time: PlayTime = self.version.end_time().into();

    //     'next_group: for (_, group) in &records
    //         .into_iter()
    //         .filter(|record| {
    //             ignore_time || (start_time..end_time).contains(&record.record().played_at().time())
    //         })
    //         .filter_map(|record| {
    //             Some((
    //                 record,
    //                 rating_targets
    //                     .range(record.record().played_at().time()..)
    //                     .next()?,
    //             ))
    //         })
    //         .group_by(|record| record.1 .0)
    //     {
    //         let records = group.collect_vec();
    //         let (target_time, list) = records[0].1;

    //         // Stores score keys included in the current target list
    //         let mut contained = HashSet::new();
    //         for entry in chain(list.target_new(), list.target_old())
    //             .chain(chain(list.candidates_new(), list.candidates_old()))
    //         {
    //             use KeyFromTargetEntry::*;
    //             let key = match self.key_from_target_entry(entry, idx_to_icon_map) {
    //                 NotFound(name) => bail!("Unknown song: {name:?}"),
    //                 Unique(key) => key,
    //                 Multiple => {
    //                     println!(
    //                         "TODO: score cannot be uniquely determined from the song name {:?}",
    //                         entry.song_name()
    //                     );
    //                     continue 'next_group;
    //                 }
    //             };
    //             contained.insert(key);
    //         }

    //         // Stores the record with the best achievement value
    //         // among the currently inspected records
    //         // for each score key not included in the current target list
    //         let mut best = HashMap::new();
    //         for &(record, _) in &records {
    //             let score_key = ScoreKey::from(record);
    //             if contained.contains(&score_key) {
    //                 continue;
    //             }
    //             let a = |record: &PlayRecord| record.achievement_result().value();
    //             use hashbrown::hash_map::Entry::*;
    //             match best.entry(score_key) {
    //                 Vacant(entry) => {
    //                     entry.insert(record);
    //                 }
    //                 Occupied(mut entry) => {
    //                     if a(entry.get()) < a(record) {
    //                         *entry.get_mut() = record;
    //                     }
    //                 }
    //             }
    //         }

    //         for (score_key, record) in best {
    //             // Ignore removed songs
    //             let Some((song, _)) = self.get(score_key)? else {
    //                 continue;
    //             };

    //             let border_entry = if song.version() == version {
    //                 list.target_new()
    //                     .last()
    //                     .context("New songs must be contained (1)")?
    //             } else {
    //                 list.target_old()
    //                     .last()
    //                     .context("New songs must be contained (1)")?
    //             };
    //             let min_entry = if song.version() == version {
    //                 (list.candidates_new().last())
    //                     .or_else(|| list.target_new().last())
    //                     .context("New songs must be contained")?
    //             } else {
    //                 (list.candidates_old().last())
    //                     .or_else(|| list.target_old().last())
    //                     .context("Old songs must be contained")?
    //             };

    //             // Finds the maximum possible rating value for the given entry
    //             let compute = |entry: &RatingTargetEntry| {
    //                 // println!("min = {min:?}");
    //                 let a = entry.achievement();
    //                 let KeyFromTargetEntry::Unique(key) =
    //                     self.key_from_target_entry(entry, idx_to_icon_map)
    //                 else {
    //                     bail!("Removed or not found")
    //                 };
    //                 let lvs = &self.get(key)?.context("Must not be removed (2)")?.1;
    //                 // println!("min_constans = {min_constants:?}");
    //                 let score = lvs
    //                     .iter()
    //                     .map(|&level| single_song_rating(level, a, rank_coef(a)))
    //                     .max()
    //                     .context("Empty level candidates")?;
    //                 anyhow::Ok((score, a))
    //             };
    //             let border_pair = compute(border_entry)?;
    //             let min_pair = compute(min_entry)?;

    //             let this_a = record.achievement_result().value();
    //             let this_sssplus = record.achievement_result().rank() == AchievementRank::SSSPlus;
    //             let candidates = ScoreConstant::candidates().filter(|&level| {
    //                 let this = single_song_rating(level, this_a, rank_coef(this_a));
    //                 let this_pair = (this, this_a);
    //                 // println!("{:?} {level} {:?}", (min_entry, min_a), (this, this_a));
    //                 min_pair >= this_pair || this_sssplus && border_pair >= this_pair
    //             });
    //             let message = lazy_format!(
    //                 "because record played at {} achieving {}{} is not in list at {}, so it's below {:?}",
    //                 record.played_at().time(),
    //                 this_a,
    //                 display_played_by(name),
    //                 target_time,
    //                 if this_sssplus { border_pair } else { min_pair },
    //             );
    //             self.set(score_key, candidates, message)?;
    //         }
    //     }

    //     Ok(())
    // }
}

impl<'s> Candidates<'s> {
    fn new<L>(
        events: &mut IndexedVec<Event<'s, L>>,
        version: MaimaiVersion,
        score: OrdinaryScoreForVersionRef<'s>,
    ) -> Candidates<'s> {
        let mut reasons = vec![];
        let candidates = match score.level() {
            Some(lv) => {
                let candidates = match lv {
                    InternalScoreLevel::Known(lv) => smallvec![lv],
                    InternalScoreLevel::Unknown(lv) => lv
                        .score_constant_candidates_aware(MaimaiVersion::BuddiesPlus <= version)
                        .collect(),
                };
                reasons.push(events.push(Event {
                    score: score.score(),
                    candidates: candidates.clone(),
                    reason: Reason::Database(lv),
                    user: None,
                }));
                candidates
            }
            None => ScoreConstant::candidates().collect(),
        };
        Self {
            score: score.score(),
            candidates,
            reasons,
        }
    }
}

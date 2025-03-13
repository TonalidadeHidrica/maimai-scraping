//! Throughout this module:
//! - Lifetime parameter `'s` refers to that of the song database.
//! - Type parameter `L` is the label for the source, i.e. play record / rating target list.
//!   `L` is used for debugging and must implement cheap `Copy`.

pub mod multi_user;

mod song_score;

use std::{collections::BTreeSet, fmt::Debug, ops::Range};

use anyhow::{bail, Context};
use chrono::{NaiveDateTime, NaiveTime};
use derive_more::Display;
use getset::{CopyGetters, Getters};
use hashbrown::HashMap;
use itertools::{repeat_n, Itertools};
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::trace;
use song_score::{AssociatedSongScoreList, ScoreOrder};

use crate::algorithm::possibilties_from_sum_and_ordering;

use super::{
    associated_user_data,
    rating::{rank_coef, single_song_rating, InternalScoreLevel, ScoreConstant, ScoreLevel},
    schema::latest::{AchievementValue, PlayTime, RatingValue},
    song_list::{
        database::{OrdinaryScoreForVersionRef, OrdinaryScoreRef, SongDatabase},
        RemoveState,
    },
    version::MaimaiVersion,
};

type CandidateList = InternalScoreLevel;

/// See the [module doc](`self`) for the definition of type parameters `'s` and `L`.
#[derive(Clone)]
pub struct Estimator<'s, LD, LL> {
    version: MaimaiVersion,
    map: HashMap<OrdinaryScoreRef<'s>, Candidates<'s>>,
    events: IndexedVec<Event<'s, LD, LL>>,
}

#[derive(Clone, Debug)]
struct Candidates<'s> {
    #[allow(unused)]
    score: OrdinaryScoreRef<'s>,
    candidates: CandidateList,
    reasons: Vec<usize>,
}
#[derive(Clone, Copy)]
pub struct CandidatesRef<'s, 'e, LD, LL> {
    candidates: &'e Candidates<'s>,
    parent: &'s Estimator<'s, LD, LL>,
}
impl<'s, 'e, LD, LL> CandidatesRef<'s, 'e, LD, LL> {
    pub fn score(self) -> OrdinaryScoreRef<'s> {
        self.candidates.score
    }
    pub fn candidates(self) -> CandidateList {
        self.candidates.candidates
    }
    pub fn reasons(self) -> impl Iterator<Item = &'e Event<'s, LD, LL>> + Clone {
        self.candidates
            .reasons
            .iter()
            .map(|&i| &self.parent.events.0[i])
    }
}

#[derive(Clone)]
struct IndexedVec<T>(Vec<T>);
impl<T> IndexedVec<T> {
    fn push(&mut self, element: T) -> usize {
        self.0.push(element);
        self.0.len() - 1
    }
}

/// See the [module doc](`self`) for the definition of type parameters `'s` and `L`.
#[derive(Clone, Copy, Getters, CopyGetters)]
pub struct Event<'s, LD, LL> {
    #[allow(unused)]
    #[getset(get_copy = "pub")]
    score: OrdinaryScoreRef<'s>,
    #[getset(get = "pub")]
    candidates: CandidateList,
    #[getset(get = "pub")]
    reason: Reason<LD, LL>,
}
impl<LD, LL> Display for Event<'_, LD, LL>
where
    LD: Display,
    LL: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {} to {} {}",
            self.score,
            if self.candidates.is_unique() {
                "determined"
            } else {
                "constrained"
            },
            self.candidates,
            self.reason
        )?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Display)]
#[display(bound(LD: Display, LL: Display))]
pub enum Reason<LD, LL> {
    #[display("according to the database which stores {_0:?}")]
    Database(InternalScoreLevel),
    #[display(
        "because the record achieving {_0} determines the single-song rating to be {_1} (source: {_2})"
    )]
    Delta(AchievementValue, i16, LD),
    #[display("by the rating target list (source: {_0})")]
    List(LL),
    #[display("by the song score list (Lv.{_0})")]
    SongScoreList(ScoreLevel),
    #[display("by assumption")]
    Assumption,
}

impl<'s, LD, LL> Estimator<'s, LD, LL> {
    pub fn new(database: &SongDatabase<'s>, version: MaimaiVersion) -> anyhow::Result<Self> {
        Self::new_impl(database, version, false)
    }

    pub fn new_distrust_all(
        database: &SongDatabase<'s>,
        version: MaimaiVersion,
    ) -> anyhow::Result<Self> {
        Self::new_impl(database, version, true)
    }

    fn new_impl(
        database: &SongDatabase<'s>,
        version: MaimaiVersion,
        distrust_all: bool,
    ) -> anyhow::Result<Self> {
        // TODO check if version is supported

        let mut events = IndexedVec(vec![]);
        let map = database
            .all_scores_for_version(version)
            .map(|score| {
                anyhow::Ok((
                    score.score(),
                    Candidates::new(&mut events, version, score, distrust_all)?,
                ))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            version,
            map,
            events,
        })
    }

    pub fn set(
        &mut self,
        score: OrdinaryScoreRef<'s>,
        predicate: impl Fn(ScoreConstant) -> bool,
        reason: Reason<LD, LL>,
    ) -> anyhow::Result<()>
    where
        Event<'s, LD, LL>: Display,
    {
        let candidates = self
            .map
            .get_mut(&score)
            .with_context(|| format!("The following score was not in the map: {score:?}"))?;
        let old_len = candidates.candidates.count_candidates();
        candidates.candidates.retain(predicate);
        if candidates.candidates.count_candidates() < old_len {
            let event = Event {
                score,
                candidates: candidates.candidates,
                reason,
            };
            trace!("{event}");
            candidates.reasons.push(self.events.push(event));
        }
        if candidates.candidates.is_empty() {
            bail!(
                "No more candidates for {score}: {}",
                candidates
                    .reasons
                    .iter()
                    .map(|&r| &self.events.0[r])
                    .join_with("; "),
            );
        }
        Ok(())
    }
    pub fn get<'e: 's>(
        &'e self,
        score: OrdinaryScoreRef<'s>,
    ) -> Option<CandidatesRef<'s, 'e, LD, LL>> {
        Some(CandidatesRef {
            candidates: self.map.get(&score)?,
            parent: self,
        })
    }
    pub fn get_scores<'e: 's>(&'e self) -> impl Iterator<Item = CandidatesRef<'s, 'e, LD, LL>> {
        self.map.values().map(|candidates| CandidatesRef {
            candidates,
            parent: self,
        })
    }

    pub fn events(&self) -> &[Event<'s, LD, LL>] {
        &self.events.0
    }
    pub fn event_len(&self) -> usize {
        self.events.0.len()
    }

    pub fn num_determined_scores(&self) -> usize {
        self.map
            .values()
            .filter(|x| x.candidates.is_unique())
            .count()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NewOrOld {
    New,
    Old,
}
impl<'s, LD, LL> Estimator<'s, LD, LL>
where
    Event<'s, LD, LL>: Display,
{
    /// It is allowed for `records` to contain target list
    /// that is recorded outside the specified version.
    /// Such records are omitted internally.
    pub fn determine_by_delta<R>(
        &mut self,
        records: impl IntoIterator<Item = R>,
        new_or_old: NewOrOld,
    ) -> anyhow::Result<()>
    where
        R: RecordLike<'s, LD>,
    {
        let start_time: PlayTime = self.version.start_time().into();
        let end_time: PlayTime = self.version.end_time().into();
        let mut r2s = BTreeSet::<(i16, _)>::new();
        let mut s2r = HashMap::<_, i16>::new();
        let max_count = match new_or_old {
            NewOrOld::New => 15,
            NewOrOld::Old => 35,
        };

        // TODO: Does it really work fine when `new_or_old == Old` (analyzing old songs)?
        // We should update `r2s` and `s2r` based on the records *before* the version starts,
        // but there is no such process!

        for record in records {
            if !record.played_within(start_time..end_time) {
                continue;
            }

            let score = record.score();
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

            let delta = record.rating_delta();
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

            self.register_single_song_rating(score, record.achievement(), rating, record.label())?;
        }

        Ok(())
    }

    pub fn register_single_song_rating(
        &mut self,
        score: OrdinaryScoreRef<'s>,
        achievement: AchievementValue,
        rating: i16,
        label: LD,
    ) -> anyhow::Result<()> {
        let a = achievement;
        self.set(
            score,
            |lv| single_song_rating(lv, a, rank_coef(a)).get() as i16 == rating,
            Reason::Delta(a, rating, label),
        )
    }

    /// It is allowed for `rating_targets` to contain target list
    /// that is recorded outside the specified version.
    /// Such list are omitted internally.
    pub fn guess_from_rating_target_order<R>(
        &mut self,
        rating_targets: impl IntoIterator<Item = R>,
    ) -> anyhow::Result<()>
    where
        R: RatingTargetListLike<'s, LL>,
        R::Entry: Copy + Debug,
    {
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
        for list in rating_targets
            .into_iter()
            .filter(|p| p.played_within(start_time..end_time))
        {
            let rating_sum_is_reliable =
                removal_time.is_none_or(|removal_time| list.play_time() < removal_time);
            // println!("{rating_sum_is_reliable}");
            let mut sub_list = vec![];
            #[derive(Clone, Copy)]
            struct Entry<'s, E> {
                new: bool,
                contributes_to_sum: bool,
                rating_target_entry: E,
                key: OrdinaryScoreRef<'s>,
                // levels: &'a [ScoreConstant],
                levels: InternalScoreLevel,
            }
            for (new, contributes_to_sum, entries) in [
                (true, true, list.target_new()),
                (true, false, list.candidates_new()),
                (false, true, list.target_old()),
                (false, false, list.candidates_old()),
            ] {
                for rating_target_entry in entries {
                    // let rating_target_entry = match rating_target_entry.as_associated() {
                    //     Ok(entry) => entry,
                    //     Err(e) => {
                    //         warn!("Not unique: {:?}: {e:#}", rating_target_entry.data());
                    //         continue;
                    //     }
                    // };
                    let score = rating_target_entry.score();
                    let levels = self.map.get(&score).with_context(|| {
                        format!(
                            "While procesing {:?}: no key matches {score:?}",
                            rating_target_entry,
                        )
                    })?;
                    sub_list.push(Entry {
                        new,
                        contributes_to_sum,
                        rating_target_entry,
                        key: score,
                        levels: levels.candidates,
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
            struct DpElement<'s, E> {
                level: ScoreConstant,
                single_song_rating: usize,
                entry: Entry<'s, E>,
                // _phantom: PhantomData<fn() -> L>,
            }
            impl<'s, E> DpElement<'s, E>
            where
                E: RatingTargetEntryLike<'s>,
            {
                fn tuple(&self) -> (bool, usize, AchievementValue) {
                    (
                        self.entry.new,
                        self.single_song_rating,
                        self.entry.rating_target_entry.achievement(),
                    )
                }
            }
            impl<'s, E> std::fmt::Debug for DpElement<'s, E>
            where
                E: RatingTargetEntryLike<'s>,
            {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    let (new, score, a) = self.tuple();
                    write!(f, "({new}, {score}, {a})")
                }
            }
            let res = possibilties_from_sum_and_ordering::solve(
                sub_list.len(),
                |i| {
                    let entry = sub_list[i];
                    entry.levels.candidates().map(move |level| {
                        let a = entry.rating_target_entry.achievement();
                        let single_song_rating =
                            single_song_rating(level, a, rank_coef(a)).get() as usize;
                        let score = entry.contributes_to_sum as usize
                            * single_song_rating
                            * rating_sum_is_reliable as usize;
                        let element = DpElement {
                            level,
                            entry,
                            single_song_rating,
                            // _phantom: PhantomData,
                        };
                        (score, element)
                    })
                },
                |(_, x), (_, y)| x.tuple().cmp(&y.tuple()).reverse(),
                list.rating().get() as usize * rating_sum_is_reliable as usize,
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
                self.set(key, |lv| res.contains(&lv), Reason::List(list.label()))?;
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

    pub fn guess_by_sort_order(
        &mut self,
        data: &AssociatedSongScoreList<'s>,
    ) -> anyhow::Result<()> {
        for (&level, scores) in &data.scores_by_level {
            struct Entry<C> {
                order: ScoreOrder,
                candidates: C,
            }
            let entries = scores
                .iter()
                .map(|score| {
                    let candidates = self
                        .get(score.score)
                        .with_context(|| format!("Candidates entry missing for {}", score.score))?;
                    anyhow::Ok(Entry {
                        order: score.order,
                        candidates,
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            let res =
                possibilties_from_sum_and_ordering::solve::<(ScoreConstant, ScoreOrder), _, _, _>(
                    scores.len(),
                    |i| {
                        let entry = &entries[i];
                        entry
                            .candidates
                            .candidates
                            .candidates
                            .candidates()
                            .map(|inner_lv| (0, (inner_lv, entry.order)))
                    },
                    |(_x_score, x_innerlv_order), (_y_score, y_innerlv_order)| {
                        x_innerlv_order.cmp(y_innerlv_order)
                    },
                    0,
                );
            // let previous_counts = entries
            //     .iter()
            //     .map(|x| x.candidates.candidates.candidates.count_candidates())
            //     .collect_vec();
            // println!("- Lv.{level}");
            // for (((s, res), previous_count), next) in (scores.iter().zip_eq(&res))
            //     .zip_eq(previous_counts)
            //     .zip(scores.iter().skip(1).map(Some).chain([None]))
            // {
            //     let res_str = res.iter().map(|x| x.1 .0).join(" ");
            //     let c = if res.len() == 1 { '-' } else { '+' };
            //     let p = if previous_count == 1 { "  ★" } else { "" };
            //     println!("  {c} {} {:?} => {res_str}{p}", s.score, s.order);
            //     if let Some(next) = next {
            //         if s.order > next.order {
            //             println!("   ===================");
            //         }
            //         if s.score.scores() == next.score.scores() {
            //             println!("   ???????????????????");
            //         }
            //     }
            // }
            for (s, res) in scores.iter().zip_eq(&res) {
                let res = res.iter().map(|x| x.1 .0).collect::<BTreeSet<_>>();
                self.set(
                    s.score,
                    |lv| res.contains(&lv),
                    Reason::SongScoreList(level),
                )?;
            }
        }
        Ok(())
    }
}

pub trait RecordLike<'s, L> {
    /// The argument is the time span of the version specified.
    /// If you want to assume always that it was played within the version,
    /// just return `true`.
    fn played_within(&self, time_range: Range<PlayTime>) -> bool;

    fn score(&self) -> OrdinaryScoreRef<'s>;
    fn achievement(&self) -> AchievementValue;
    fn rating_delta(&self) -> i16;

    fn label(&self) -> L;
}
pub trait RatingTargetListLike<'s, L> {
    /// The argument is the time span of the version specified.
    /// If you want to assume always that it was played within the version,
    /// just return `true`.
    /// This is used for filtering by version.
    fn played_within(&self, time_range: Range<PlayTime>) -> bool;
    /// This is used to compare with the first removal time during the version,
    /// determining whether the rating sum is reliable or not.
    fn play_time(&self) -> NaiveDateTime;

    fn rating(&self) -> RatingValue;

    type Entry: RatingTargetEntryLike<'s>;
    type Entries: IntoIterator<Item = Self::Entry>;
    fn target_new(&self) -> Self::Entries;
    fn target_old(&self) -> Self::Entries;
    fn candidates_new(&self) -> Self::Entries;
    fn candidates_old(&self) -> Self::Entries;

    fn label(&self) -> L;
}
pub trait RatingTargetEntryLike<'s> {
    fn score(&self) -> OrdinaryScoreRef<'s>;
    fn achievement(&self) -> AchievementValue;
}

impl<'s> Candidates<'s> {
    fn new<LD, LL>(
        events: &mut IndexedVec<Event<'s, LD, LL>>,
        version: MaimaiVersion,
        score: OrdinaryScoreForVersionRef<'s>,
        distrust: bool,
    ) -> anyhow::Result<Candidates<'s>> {
        let mut reasons = vec![];
        let candidates = score
            .level()
            .with_context(|| format!("Missing score level: {score:?}"))?;
        let candidates = if distrust {
            InternalScoreLevel::unknown(version, candidates.into_level(version))
        } else {
            candidates
        };
        reasons.push(events.push(Event {
            score: score.score(),
            candidates,
            reason: Reason::Database(candidates),
        }));
        // let candidates = match score.level() {
        //     Some(lv) => {
        //         let candidates = match lv {
        //             InternalScoreLevel::Known(lv) => smallvec![lv],
        //             InternalScoreLevel::Unknown(lv) => lv
        //                 .score_constant_candidates_aware(MaimaiVersion::BuddiesPlus <= version)
        //                 .collect(),
        //         };
        //         reasons.push(events.push(Event {
        //             score: score.score(),
        //             candidates,
        //             reason: Reason::Database(candidates),
        //         }));
        //         candidates
        //     }
        //     None => ScoreConstant::candidates().collect(),
        // };
        Ok(Self {
            score: score.score(),
            candidates,
            reasons,
        })
    }
}

pub fn visualize_rating_target<'est, 'd, 's: 'est + 'd, 'ent: 'd, 'e, LD: 'est, LL: 'est>(
    estimator: impl Into<Option<&'est Estimator<'s, LD, LL>>>,
    entry: &'ent associated_user_data::RatingTargetEntry<'d, 's>,
) -> impl Display + 'd + 'ent {
    let estimator = estimator.into();

    let level = entry.data().level();
    let possible_lvs = InternalScoreLevel::unknown(entry.version(), level);
    let lvs = estimator
        .and_then(|estimator| Some(estimator.get(entry.score().ok()?.score())?.candidates()))
        .unwrap_or(possible_lvs);

    let a = entry.data().achievement();

    let fill = repeat_n(None, 6usize.saturating_sub(possible_lvs.count_candidates()));
    let candidates = (possible_lvs.candidates().map(Some).chain(fill))
        .map(move |constant| {
            let value = constant.map(|lv| {
                lvs.contains(lv)
                    .then(|| single_song_rating(lv, a, rank_coef(a)).get() as usize)
            });
            lazy_format!(match (value) {
                Some(Some(value)) => ("[{:>3}]", value),
                Some(_) => "[   ]",
                None => "     ",
            })
        })
        .join_with(" ");

    entry.data().score_metadata().generation();
    let achievement = entry.data().achievement();
    let score = lazy_format!(match (entry.score()) {
        Ok(score) => ("{}", score.score()),
        Err(_) => (
            "{} ({:?} {:?}) [*]",
            entry.data().song_name(),
            entry.data().score_metadata().generation(),
            entry.data().score_metadata().difficulty(),
        ),
    });
    lazy_format!("{level:<3} {candidates} {achievement:>9} {score}")
}

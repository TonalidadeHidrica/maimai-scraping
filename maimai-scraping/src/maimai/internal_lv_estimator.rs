use std::{collections::BTreeSet, fmt::Display};

use anyhow::{bail, Context};
use derive_more::Display;
use hashbrown::HashMap;
use joinery::JoinableIterator;
use smallvec::{smallvec, SmallVec};

use super::{
    associated_user_data::OrdinaryPlayRecordAssociated,
    estimator_config_multiuser::UserName,
    load_score_level::{InternalScoreLevel, MaimaiVersion},
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::{AchievementValue, PlayTime},
    song_list::database::{OrdinaryScoreForVersionRef, OrdinaryScoreRef, SongDatabase},
};

type CandidateList = SmallVec<[ScoreConstant; 6]>;

pub struct Estimator<'s, 'n> {
    version: MaimaiVersion,
    map: HashMap<OrdinaryScoreRef<'s>, Candidates<'s>>,
    events: IndexedVec<Event<'s, 'n>>,
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

pub struct Event<'s, 'n> {
    #[allow(unused)]
    score: OrdinaryScoreRef<'s>,
    candidates: CandidateList,
    reason: Reason,
    user: Option<&'n UserName>,
}
impl Display for Event<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Constrained to [{}] {}",
            self.candidates.iter().join_with(", "),
            self.reason
        )?;
        if let Some(user) = self.user {
            write!(f, " (played by {user:?})")?;
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
}

impl<'s, 'n> Estimator<'s, 'n> {
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
        user: Option<&'n UserName>,
    ) -> anyhow::Result<()> {
        let candidates = self
            .map
            .get_mut(&score)
            .with_context(|| format!("The following score was not in the map: {score:?}"))?;
        candidates.candidates.retain(|&mut v| predicate(v));
        candidates.reasons.push(self.events.push(Event {
            score,
            candidates: candidates.candidates.clone(),
            reason,
            user,
        }));
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
impl<'s, 'n> Estimator<'s, 'n> {
    pub fn determine_by_delta<'d>(
        &mut self,
        user: Option<&'n UserName>,
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

        // TODO: Does `new_or_old == Old` (analyzing old songs) really works fine?
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
        user: Option<&'n UserName>,
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
}

impl<'s> Candidates<'s> {
    fn new(
        events: &mut IndexedVec<Event<'s, '_>>,
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

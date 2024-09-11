use hashbrown::HashMap;
use smallvec::{smallvec, SmallVec};

use super::{
    load_score_level::{InternalScoreLevel, MaimaiVersion},
    rating::ScoreConstant,
    song_list::database::{OrdinaryScoreForVersionRef, OrdinaryScoreRef, SongDatabase},
};

type CandidateList = SmallVec<[ScoreConstant; 6]>;

pub struct Estimator<'s> {
    map: HashMap<OrdinaryScoreRef<'s>, Candidates<'s>>,
    events: IndexedVec<Event<'s>>,
}

struct Candidates<'s> {
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

struct Event<'s> {
    score: OrdinaryScoreRef<'s>,
    candidates: CandidateList,
    kind: EventKind,
}
enum EventKind {
    Database(InternalScoreLevel),
}

impl<'s> Estimator<'s> {
    pub fn new(database: &SongDatabase<'s>, version: MaimaiVersion) -> Self {
        // TODO check if version is supported

        let mut events = IndexedVec(vec![]);
        let map = database
            .all_scores_for_version(version)
            .map(|score| (score.score(), Candidates::new(&mut events, version, score)))
            .collect();

        Self { map, events }
    }
}

impl<'s> Candidates<'s> {
    fn new(
        events: &mut IndexedVec<Event<'s>>,
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
                    kind: EventKind::Database(lv),
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

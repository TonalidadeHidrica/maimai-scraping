use anyhow::Context;
use derive_by_key::DeriveByKey;
use getset::{CopyGetters, Getters};
use hashbrown::HashMap;
use itertools::Itertools;

use crate::maimai::{
    load_score_level::MaimaiVersion,
    official_song_list::UtageScore,
    rating::ScoreLevel,
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon},
};

use super::{OrdinaryScore, OrdinaryScores, Song};

#[derive(Getters)]
#[getset(get = "pub")]
pub struct SongDatabase<'s> {
    songs: Vec<SongRef<'s>>,
    icon_map: HashMap<&'s SongIcon, SongRef<'s>>,
}
impl<'s> SongDatabase<'s> {
    pub fn new(songs: &'s [Song]) -> Self {
        let songs = songs
            .iter()
            .enumerate()
            .map(|(id, song)| SongRef { song, id })
            .collect_vec();

        // Make icon map.
        // `verify_properties` guarantees that an icon exists for all unremoved songs.
        let icon_map = songs
            .iter()
            .filter_map(|&x| Some((x.song.icon.as_ref()?, x)))
            .collect();

        Self { songs, icon_map }
    }

    pub fn song_from_icon(&self, icon: &SongIcon) -> anyhow::Result<SongRef<'s>> {
        self.icon_map
            .get(&icon)
            .copied()
            .with_context(|| format!("No song matches {icon:?}"))
    }
}

#[derive(Clone, Copy, Debug, CopyGetters, DeriveByKey)]
#[derive_by_key(key = "key", PartialEq, Eq, PartialOrd, Ord, Hash)]
#[getset(get_copy = "pub")]
pub struct SongRef<'s> {
    song: &'s Song,
    id: usize,
}
impl<'s> SongRef<'s> {
    fn key(self) -> usize {
        self.id
    }

    pub fn scores(self, generation: ScoreGeneration) -> Option<OrdinaryScoresRef<'s>> {
        let scores = self.song.scores[generation].as_ref()?;
        Some(OrdinaryScoresRef {
            song: self,
            generation,
            scores,
        })
    }
}

/// A reference to a score for a specific version.
#[derive(Clone, Copy, Debug)]
pub enum ScoreForVersionRef<'s> {
    Ordinary(OrdinaryScoreForVersionRef<'s>),
    Utage(UtageScoreRef<'s>),
}

/// A reference to a set of scores for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoresRef<'s> {
    song: SongRef<'s>,
    generation: ScoreGeneration,
    scores: &'s OrdinaryScores,
}
impl<'s> OrdinaryScoresRef<'s> {
    pub fn score(self, difficulty: ScoreDifficulty) -> Option<OrdinaryScoreRef<'s>> {
        let score = match difficulty {
            ScoreDifficulty::Basic => &self.scores.basic,
            ScoreDifficulty::Advanced => &self.scores.advanced,
            ScoreDifficulty::Expert => &self.scores.expert,
            ScoreDifficulty::Master => &self.scores.master,
            ScoreDifficulty::ReMaster => self.scores.re_master.as_ref()?,
            ScoreDifficulty::Utage => return None,
        };
        Some(OrdinaryScoreRef {
            scores: self,
            difficulty,
            score,
        })
    }
}

/// A reference to an ordinary for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoreRef<'s> {
    scores: OrdinaryScoresRef<'s>,
    difficulty: ScoreDifficulty,
    score: &'s OrdinaryScore,
}
impl <'s> OrdinaryScoreRef {
    pub fn for_version(self, version: MaimaiVersion) -> Option<OrdinaryScoreForVersionRef<'s>> {
        if let Some(start_version) = self.scores.version {
            if version < start_version {
                return None;
            }
        }
    }
}

/// A reference to an ordinary score for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoreForVersionRef<'s> {
    score: OrdinaryScoreRef<'s>,
    version: MaimaiVersion,
    level: Option<ScoreLevel>,
}

/// A reference to an utage score.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct UtageScoreRef<'s> {
    song: SongRef<'s>,
    score: &'s UtageScore,
}

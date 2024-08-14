use derive_more::{AsRef, Display, From, FromStr};
use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use super::{
    load_score_level::{InternalScoreLevel, MaimaiVersion},
    official_song_list::Category,
    rating::ScoreLevel,
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
};

/// A song has zero or one standard score, zero or one deluxe score,
/// and zero or more utage scores.
#[derive(Debug, Serialize, Deserialize)]
pub struct Song {
    pub name: EnumMap<MaimaiVersion, Option<SongName>>,
    pub category: Option<Category>,
    pub artist: Option<SongArtist>,
    pub pronunciation: Option<SongPronunciation>,
    pub abbreviation: EnumMap<MaimaiVersion, Option<SongAbbreviation>>,
    pub scores: EnumMap<ScoreGeneration, Option<OrdinaryScores>>,
    pub icon: Option<SongIcon>,
}

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct SongArtist(String);

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct SongPronunciation(String);

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct SongAbbreviation(String);

#[derive(Debug, Serialize, Deserialize)]
pub struct OrdinaryScores {
    pub easy: Option<OrdinaryScore>,
    pub basic: OrdinaryScore,
    pub advanced: OrdinaryScore,
    pub expert: OrdinaryScore,
    pub master: OrdinaryScore,
    pub re_master: Option<OrdinaryScore>,
    pub version: Option<MaimaiVersion>,
}
impl OrdinaryScores {
    pub fn get_score_mut(&mut self, difficulty: ScoreDifficulty) -> Option<&mut OrdinaryScore> {
        match difficulty {
            ScoreDifficulty::Basic => Some(&mut self.basic),
            ScoreDifficulty::Advanced => Some(&mut self.advanced),
            ScoreDifficulty::Expert => Some(&mut self.expert),
            ScoreDifficulty::Master => Some(&mut self.master),
            ScoreDifficulty::ReMaster => self.re_master.as_mut(),
            ScoreDifficulty::Utage => None,
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct OrdinaryScore {
    pub levels: EnumMap<MaimaiVersion, Option<InternalScoreLevel>>,
}

pub struct UtageScore {
    pub level: ScoreLevel,
    pub comment: UtageComment,
    pub kanji: UtageKanji,
    pub buddy: bool,
}

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct UtageComment(String);

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct UtageKanji(String);

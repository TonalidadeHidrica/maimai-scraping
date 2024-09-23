use std::collections::BTreeMap;

use chrono::{NaiveDate, NaiveDateTime};
use derive_more::{AsRef, Display, From, FromStr};
use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use super::{
    load_score_level::MaimaiVersion,
    official_song_list::{ArtistName, Category, SongKana, UtageScore},
    rating::InternalScoreLevel,
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
};

pub mod database;

/// A song has zero or one standard score, zero or one deluxe score,
/// and zero or more utage scores.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Song {
    pub name: EnumMap<MaimaiVersion, Option<SongName>>,
    pub category: EnumMap<MaimaiVersion, Option<Category>>,
    pub artist: EnumMap<MaimaiVersion, Option<ArtistName>>,
    pub pronunciation: Option<SongKana>,
    pub abbreviation: EnumMap<MaimaiVersion, Option<SongAbbreviation>>,
    pub scores: EnumMap<ScoreGeneration, Option<OrdinaryScores>>,
    pub utage_scores: Vec<UtageScore>,
    pub icon: Option<SongIcon>,
    pub remove_state: RemoveState,
    pub locked_history: BTreeMap<NaiveDateTime, bool>,
}

impl Song {
    pub fn removed(&self) -> bool {
        matches!(self.remove_state, RemoveState::Removed(_))
    }

    pub fn latest_song_name(&self) -> Option<&SongName> {
        self.name.values().flatten().last()
    }
}

#[derive(
    Clone, PartialEq, Eq, Hash, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize,
)]
#[as_ref(forward)]
pub struct SongAbbreviation(String);

#[derive(Default, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum RemoveState {
    Present,
    Removed(NaiveDate),
    Revived(NaiveDate, NaiveDate),
}
impl Default for RemoveState {
    fn default() -> Self {
        Self::Present
    }
}
impl RemoveState {
    pub fn exist_for_version(self, version: MaimaiVersion) -> bool {
        match self {
            RemoveState::Present => {}
            RemoveState::Removed(x) => {
                let remove_version = MaimaiVersion::of_date(x).unwrap();
                let removed_at_the_beginning = x == remove_version.start_date();
                let removed = if removed_at_the_beginning {
                    remove_version <= version
                } else {
                    remove_version < version
                };
                if removed {
                    return false;
                }
            }
            RemoveState::Revived(x, y) => {
                let remove_version = MaimaiVersion::of_date(x).unwrap();
                let recover_version = MaimaiVersion::of_date(y).unwrap();

                let removed_at_the_beginning = x == remove_version.start_date();
                let after_removed = if removed_at_the_beginning {
                    remove_version <= version
                } else {
                    remove_version < version
                };

                if after_removed && version < recover_version {
                    return false;
                }
            }
        }
        true
    }
}
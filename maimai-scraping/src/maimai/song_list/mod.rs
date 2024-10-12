use std::{borrow::Cow, cmp::Ordering, collections::BTreeMap};

use chrono::{NaiveDate, NaiveDateTime};
use derive_more::{AsRef, Display, From, FromStr};
use enum_map::EnumMap;
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use super::{
    rating::{InternalScoreLevel, ScoreLevel},
    schema::latest::{
        ArtistName, Category, ScoreDifficulty, ScoreGeneration, SongIcon, SongName, UtageKindRaw,
    },
    version::MaimaiVersion,
};

pub mod database;
pub mod in_lv;
pub mod official;

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

#[derive(
    Clone, PartialEq, Eq, Debug, Getters, CopyGetters, Serialize, Deserialize, TypedBuilder,
)]
pub struct UtageScore {
    #[getset(get_copy = "pub")]
    level: ScoreLevel,
    #[getset(get = "pub")]
    comment: UtageComment,
    #[getset(get = "pub")]
    kanji: UtageKindRaw,
    #[getset(get_copy = "pub")]
    buddy: bool,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize)]
pub struct UtageIdentifier<'a>(Cow<'a, UtageKindRaw>, ScoreLevel);
impl<'a> UtageIdentifier<'a> {
    pub fn to_owned(&self) -> UtageIdentifier<'static> {
        let e: UtageKindRaw = self.0.as_ref().clone();
        UtageIdentifier(Cow::Owned(e), self.1)
    }
}

impl UtageScore {
    /// For now, we assume that this uniquely specifies an utage score in a song.
    /// Otherwise, how on earth can we determine the score???
    pub fn identifier(&self) -> UtageIdentifier {
        UtageIdentifier(Cow::Borrowed(&self.kanji), self.level)
    }
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Serialize, Deserialize)]
pub struct UtageComment(String);

#[derive(Clone, Debug, From, AsRef, FromStr, Display, Serialize, Deserialize)]
// FIXME: Commenting out because otherwise `.as_ref()` seems to require explicit target type
// #[as_ref(forward)]
pub struct SongKana(String);

impl PartialEq for SongKana {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for SongKana {}
impl PartialOrd for SongKana {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SongKana {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let [x, y] = [&self.0, &other.0];
        (x.is_empty().cmp(&y.is_empty()))
            .then_with(|| {
                let [x, y] = [x, y].map(|x| maimai_char_order(x.chars().next().unwrap()));
                x.cmp(&y)
            })
            .then_with(|| {
                let [u, v] = [x, y].map(|x| &x[(1..).find(|&i| x.is_char_boundary(i)).unwrap()..]);
                u.cmp(v)
            })
    }
}
pub fn maimai_char_order(c: char) -> (u8, char) {
    match c {
        'ア'..='ン' => (0, c),
        'A'..='Z' => (1, c),
        '0'..='9' => (2, c),
        _ => (3, c),
    }
}

#[cfg(test)]
mod tests {
    use super::SongKana;

    #[test]
    fn test_song_kana_cmp() {
        for [x, y] in [
            ["ソウキユウフカク", "YETANOTHERDRIZZLYRAIN"],
            ["L4TS2018FEATアヒルアントケイタ", "LUNATICVIBES"],
            [
                "MAIMAIチヤンノテエマ",
                "MAIムMAIムFEATシユウカンシヨウネンマカシン",
            ],
        ]
        .map(|x| x.map::<_, SongKana>(|x| x.to_owned().into()))
        {
            assert!(x < y);
            assert!(y > x);
        }
    }
}

use anyhow::bail;
use chrono::naive::NaiveDateTime;
use getset::{CopyGetters, Getters};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::str::FromStr;
use typed_builder::TypedBuilder;
use url::Url;

#[derive(PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Getters, Serialize, Deserialize)]
pub struct PlayRecord {
    #[getset(get = "pub")]
    played_at: PlayedAt,
    #[getset(get = "pub")]
    song_metadata: SongMetadata,
    #[getset(get_copy = "pub")]
    score_metadata: ScoreMetadata,
    #[getset(get_copy = "pub")]
    cleared: bool,
    #[getset(get_copy = "pub")]
    achievement_result: AchievementResult,
    #[getset(get_copy = "pub")]
    deluxscore_result: DeluxscoreResult,
    #[getset(get_copy = "pub")]
    combo_result: ComboResult,
    #[getset(get = "pub")]
    battle_result: Option<BattleResult>,
    #[getset(get = "pub")]
    matching_result: Option<MatchingResult>,
    #[getset(get_copy = "pub")]
    life_result: LifeResult,
    #[getset(get = "pub")]
    tour_members: TourMemberList,
    #[getset(get_copy = "pub")]
    rating_result: RatingResult,
    #[getset(get_copy = "pub")]
    judge_result: JudgeResult,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Getters, Serialize, Deserialize)]
pub struct PlayedAt {
    #[getset(get_copy = "pub")]
    idx: Idx,
    #[getset(get_copy = "pub")]
    time: PlayTime,
    #[getset(get = "pub")]
    place: PlaceName,
    #[getset(get_copy = "pub")]
    track: TrackIndex,
}

// Default is used for Idx(0), which is valid
#[derive(
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Debug,
    derive_more::Display,
    derive_more::Into,
    Serialize,
    Deserialize,
)]
pub struct Idx(u8);

impl TryFrom<u8> for Idx {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0..=49 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Debug,
    derive_more::From,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
pub struct PlayTime(NaiveDateTime);

impl PlayTime {
    pub fn get(self) -> NaiveDateTime {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct PlaceName(String);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TrackIndex(u8);

impl TryFrom<u8> for TrackIndex {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=4 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct SongMetadata {
    name: SongName,
    cover_art: SongIcon,
}

#[derive(
    Clone,
    PartialEq,
    Eq,
    Hash,
    Debug,
    derive_more::From,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
pub struct SongName(String);

#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    derive_more::From,
    derive_more::FromStr,
    Serialize,
    Deserialize,
)]
pub struct SongIcon(Url);

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    TypedBuilder,
    CopyGetters,
    Serialize,
    Deserialize,
)]
#[getset(get_copy = "pub")]
pub struct ScoreMetadata {
    generation: ScoreGeneration,
    difficulty: ScoreDifficulty,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ScoreGeneration {
    Standard,
    Deluxe,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    ReMaster,
}

impl FromStr for ScoreDifficulty {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ScoreDifficulty::*;
        Ok(match s.chars().next() {
            Some('b' | 'B') => Basic,
            Some('a' | 'A') => Advanced,
            Some('e' | 'E') => Expert,
            Some('m' | 'M') => Master,
            Some('r' | 'R') => ReMaster,
            _ => bail!("Invalid score difficulty: {:?}", s),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct AchievementResult {
    value: AchievementValue,
    new_record: bool,
    rank: AchievementRank,
}

pub use super::ver_20210316_2338::AchievementValue;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AchievementRank {
    SSSPlus,
    SSS,
    SSPlus,
    SS,
    SPlus,
    S,
    AAA,
    AA,
    A,
    BBB,
    BB,
    B,
    C,
    D,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct DeluxscoreResult {
    score: ValueWithMax<u32>,
    rank: DeluxscoreRank,
    new_record: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DeluxscoreRank(u8);

impl TryFrom<u8> for DeluxscoreRank {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0..=5 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct ComboResult {
    full_combo_kind: FullComboKind,
    combo: ValueWithMax<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FullComboKind {
    Nothing,
    FullCombo,
    FullComboPlus,
    AllPerfect,
    AllPerfectPlus,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct RatingResult {
    rating: RatingValue,
    delta: i16,
    delta_sign: RatingDeltaSign,
    border_color: RatingBorderColor,
    // Abolished as of DELUXE Splash PLUS, started on 2021/3/18
    // grade_icon: GradeIcon,
}

pub use super::ver_20210316_2338::RatingValue;

// #[derive(PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
// pub struct GradeIcon(Url);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RatingDeltaSign {
    Up,
    Keep,
    Down,
}

// ["normal", "blue", "green", "orange", "red", "purple", "bronze", "silver", "gold", "rainbow"]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RatingBorderColor {
    Normal,
    Blue,
    Green,
    Orange,
    Red,
    Purple,
    Bronze,
    Silver,
    Gold,
    /// Added as of DELUXE Splash PLUS, started on 2021/3/18
    Platinum,
    Rainbow,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct TourMemberList(Vec<TourMember>);

impl TryFrom<Vec<TourMember>> for TourMemberList {
    type Error = Vec<TourMember>;
    fn try_from(value: Vec<TourMember>) -> Result<Self, Self::Error> {
        match value.len() {
            1..=5 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct TourMember {
    #[getset(get_copy = "pub")]
    star: u32,
    #[getset(get = "pub")]
    icon: TourMemberIcon,
    #[getset(get_copy = "pub")]
    level: u32,
}

#[derive(Clone, PartialEq, Eq, Debug, derive_more::FromStr, Serialize, Deserialize)]
pub struct TourMemberIcon(Url);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ValueWithMax<T: PartialOrd> {
    value: T,
    max: T,
}

impl<T: PartialOrd> ValueWithMax<T> {
    pub fn new(value: T, max: T) -> Result<Self, (T, T)> {
        if value <= max {
            Ok(Self { value, max })
        } else {
            Err((value, max))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct JudgeResult {
    fast: u32,
    late: u32,
    tap: JudgeCount,
    hold: JudgeCount,
    slide: JudgeCount,
    touch: JudgeCount,
    break_: JudgeCountWithCP,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JudgeCount {
    Nothing,
    JudgeCountWithCP(JudgeCountWithCP),
    // The order of variants is important for correct deserializing.
    // Due to the flattening, if the following variant is wrote first,
    // critical_perfect will be ignored and lost.
    JudgeCountWithoutCP(JudgeCountWithoutCP),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct JudgeCountWithCP {
    critical_perfect: u32,
    #[serde(flatten)]
    others: JudgeCountWithoutCP,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
pub struct JudgeCountWithoutCP {
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct MatchingResult {
    #[getset(get_copy = "pub")]
    full_sync_kind: FullSyncKind,
    #[getset(get_copy = "pub")]
    max_sync: ValueWithMax<u32>,
    #[getset(get = "pub")]
    other_players: OtherPlayersList,
    #[getset(get_copy = "pub")]
    rank: MatchingRank,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FullSyncKind {
    Nothing,
    FullSync,
    FullSyncPlus,
    FullSyncDx,
    FullSyncDxPlus,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct OtherPlayersList(Vec<OtherPlayer>);

impl TryFrom<Vec<OtherPlayer>> for OtherPlayersList {
    type Error = Vec<OtherPlayer>;
    fn try_from(value: Vec<OtherPlayer>) -> Result<Self, Self::Error> {
        match value.len() {
            1..=3 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct OtherPlayer {
    #[getset(get_copy = "pub")]
    difficulty: ScoreDifficulty,
    #[getset(get = "pub")]
    user_name: UserName,
}

#[derive(Clone, PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct UserName(String);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct MatchingRank(u8);

impl TryFrom<u8> for MatchingRank {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=4 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Serialize, Deserialize)]
pub struct BattleResult {
    kind: BattleKind,
    win_or_lose: BattleWinOrLose,
    opponent: BattleOpponent,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BattleKind {
    VsFriend,
    Promotion,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BattleWinOrLose {
    Win,
    Lose,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Serialize, Deserialize)]
pub struct BattleOpponent {
    user_name: UserName,
    achievement_value: AchievementValue,
    rating: RatingValue,
    border_color: RatingBorderColor,
    // Abolished as of DELUXE Splash PLUS, started on 2021/3/18
    // grade_icon: GradeIcon,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum LifeResult {
    Nothing,
    PerfectChallengeResult(ValueWithMax<u32>),
    CourseResult(ValueWithMax<u32>),
}

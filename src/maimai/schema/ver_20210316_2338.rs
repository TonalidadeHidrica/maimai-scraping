use chrono::naive::NaiveDateTime;
use derive_getters::Getters;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, fmt::Display};
use typed_builder::TypedBuilder;
use url::Url;

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct PlayRecord {
    played_at: PlayedAt,
    song_metadata: SongMetadata,
    score_metadata: ScoreMetadata,
    cleared: bool,
    achievement_result: AchievementResult,
    deluxscore_result: DeluxscoreResult,
    combo_result: ComboResult,
    battle_result: Option<BattleResult>,
    matching_result: Option<MatchingResult>,
    perfect_challenge_result: Option<PerfectChallengeResult>,
    tour_members: TourMemberList,
    rating_result: RatingResult,
    judge_result: JudgeResult,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct PlayedAt {
    time: NaiveDateTime,
    place: String,
    track: TrackIndex,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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
pub struct SongMetadata {
    name: String,
    cover_art: Url,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct ScoreMetadata {
    generation: ScoreGeneration,
    difficulty: ScoreDifficulty,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScoreGeneration {
    Standard,
    Deluxe,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    ReMaster,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct AchievementResult {
    value: AchievementValue,
    new_record: bool,
    rank: AchievementRank,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct AchievementValue(u32);

impl TryFrom<u32> for AchievementValue {
    type Error = u32;

    fn try_from(v: u32) -> Result<Self, u32> {
        match v {
            0..=101_0000 => Ok(Self(v)),
            _ => Err(v),
        }
    }
}

impl Display for AchievementValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = self.0 / 10000;
        let y = self.0 % 10000;
        write!(f, "{}.{:04}%", x, y)
    }
}

impl AchievementValue {
    pub fn get(&self) -> u32 {
        self.0
    }
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct DeluxscoreResult {
    score: ValueWithMax<u32>,
    rank: DeluxscoreRank,
    new_record: bool,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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
#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct ComboResult {
    full_combo_kind: FullComboKind,
    combo: ValueWithMax<u32>,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FullComboKind {
    Nothing,
    FullCombo,
    FullComboPlus,
    AllPerfect,
    AllPerfectPlus,
}

#[derive(PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct PerfectChallengeResult(ValueWithMax<u32>);

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct RatingResult {
    rating: RatingValue,
    delta: i16,
    delta_sign: RatingDeltaSign,
    border_color: RatingBorderColor,
    grade_icon: GradeIcon,
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
pub struct RatingValue(u16);

impl RatingValue {
    pub fn get(&self) -> u16 {
        self.0
    }
}

#[derive(PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct GradeIcon(Url);

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum RatingDeltaSign {
    Up,
    Keep,
    Down,
}

// ["normal", "blue", "green", "orange", "red", "purple", "bronze", "silver", "gold", "rainbow"]
#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct TourMember {
    star: u32,
    icon: Url,
    level: u32,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct JudgeResult {
    fast: u32,
    late: u32,
    tap: JudgeCount,
    hold: JudgeCount,
    slide: JudgeCount,
    touch: JudgeCount,
    break_: JudgeCountWithCP,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JudgeCount {
    Nothing,
    JudgeCountWithCP(JudgeCountWithCP),
    // The order of variants is important for correct deserializing.
    // Due to the flattening, if the following variant is wrote first,
    // critical_perfect will be ignored and lost.
    JudgeCountWithoutCP(JudgeCountWithoutCP),
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct JudgeCountWithCP {
    critical_perfect: u32,
    #[serde(flatten)]
    others: JudgeCountWithoutCP,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct JudgeCountWithoutCP {
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct MatchingResult {
    full_sync_kind: FullSyncKind,
    max_sync: ValueWithMax<u32>,
    other_players: OtherPlayersList,
    rank: MatchingRank,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
pub struct OtherPlayer {
    difficulty: ScoreDifficulty,
    user_name: String,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
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
    user_name: String,
    achievement_value: AchievementValue,
    rating: RatingValue,
    border_color: RatingBorderColor,
    grade_icon: GradeIcon,
}

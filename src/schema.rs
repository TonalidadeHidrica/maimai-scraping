#![allow(unused)]
use chrono::naive::NaiveDateTime;
use derive_more::Constructor;
use std::convert::TryFrom;
use typed_builder::TypedBuilder;
use url::Url;

#[derive(Debug)]
pub struct PlayRecord {
    played_at: PlayedAt,
    song_metadata: SongMetadata,
    score_metadata: ScoreMetadata,
    cleared: bool,
    achievement_result: AchievementResult,
    deluxscore_result: DeluxscoreResult,
    full_combo_result: ComboResult,
    matching_result: Option<MatchingResult>,
    perfect_challenge_result: Option<PerfectChallengeResult>,
    tour_members: TourMemberList,
    rating_result: RatingResult,
    judge_result: JudgeResult,
}

#[derive(Debug, TypedBuilder)]
pub struct PlayedAt {
    time: NaiveDateTime,
    place: String,
    track: TrackIndex,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct SongMetadata {
    name: String,
    cover_art: Url,
}

#[derive(Debug)]
pub struct ScoreMetadata {
    generation: ScoreGeneration,
    difficulty: ScoreDifficulty,
}

#[derive(Debug)]
pub enum ScoreGeneration {
    Standard,
    Deluxe,
}

#[derive(Debug)]
pub enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    ReMaster,
}

#[derive(Debug, TypedBuilder)]
pub struct AchievementResult {
    value: AchievementValue,
    new_record: bool,
    rank: AchievementRank,
}

#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug, TypedBuilder)]
pub struct DeluxscoreResult {
    score: ValueWithMax<u32>,
    rank: DeluxscoreRank,
    new_record: bool,
}

#[derive(Debug)]
pub struct DeluxscoreRank(u8);

impl TryFrom<u8> for DeluxscoreRank {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=5 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}
#[derive(Debug)]
pub struct ComboResult {
    kind: FullComboKind,
    combo: ValueWithMax<u32>,
}

#[derive(Debug)]
pub enum FullComboKind {
    Nothing,
    FullCombo,
    FullComboPlus,
    AllPerfect,
    AllPerfectPlus,
}

#[derive(Debug, derive_more::From)]
pub struct PerfectChallengeResult(ValueWithMax<u32>);

#[derive(Debug)]
pub struct RatingResult {
    rating: u16,
    delta: i16,
    delta_sign: RatingDeltaSign,
    border_color: RatingBorderColor,
    grade_icon: Url,
}

#[derive(Debug)]
pub enum RatingDeltaSign {
    Up,
    Keep,
    Down,
}

// ["normal", "blue", "green", "orange", "red", "purple", "bronze", "silver", "gold", "rainbow"]
#[derive(Debug)]
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

#[derive(Debug)]
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

#[derive(Debug)]
pub struct TourMember {
    star: u32,
    icon: Url,
    level: u32,
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct JudgeResult {
    fast: u32,
    late: u32,
    tap: JudgeCount,
    hold: JudgeCount,
    slide: JudgeCount,
    touch: JudgeCount,
    break_: JudgeCountWithCP,
}

#[derive(Debug)]
pub enum JudgeCount {
    Nothing,
    JudgeCountWithoutCP(JudgeCountWithoutCP),
    JudgeCountWithCP(JudgeCountWithCP),
}

#[derive(Debug)]
pub struct JudgeCountWithCP {
    critical_perfect: u32,
    others: JudgeCountWithoutCP,
}

#[derive(Debug)]
pub struct JudgeCountWithoutCP {
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
}

#[derive(Debug)]
pub struct MatchingResult {
    kind: FullSyncKind,
    sync: ValueWithMax<u32>,
    other_players: Vec<OtherPlayer>,
    rank: MatchingRank,
}

#[derive(Debug)]
pub enum FullSyncKind {
    Nothing,
    FullSync,
    FullSyncPlus,
    FullSyncDx,
    FullSyncDxPlus,
}

#[derive(Debug)]
pub struct OtherPlayer {
    difficulty: ScoreDifficulty,
    user_name: String,
}

#[derive(Debug)]
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

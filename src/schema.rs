#![allow(unused)]
use chrono::naive::NaiveDateTime;
use std::convert::TryFrom;
use url::Url;

struct PlayRecord {
    played_at: PlayedAt,
    song_metadata: SongMetadata,
    score_metadata: ScoreMetadata,
    cleared: bool,
    achievement_result: AchievementResult,
    deluxscore_result: DeluxscoreResult,
    full_combo_result: FullComboResult,
    matching_result: Option<MatchingResult>,
    perfect_challenge_result: Option<PerfectChallengeResult>,
    tour_members: TourMemberList,
    rating_result: RatingResult,
    judge_result: JudgeResult,
}

struct PlayedAt {
    time: NaiveDateTime,
    place: String,
    track: TrackIndex,
}

struct TrackIndex(u8);

impl TryFrom<u8> for TrackIndex {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=4 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

struct SongMetadata {
    name: String,
    cover_art: Url,
}

struct ScoreMetadata {
    generation: ScoreGeneration,
    difficulty: ScoreDifficulty,
}

enum ScoreGeneration {
    Standard,
    Deluxe,
}

enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    MasterPlus,
}

struct AchievementResult {
    value: AchievementValue,
    new_record: bool,
    rank: AchievementRank,
}

struct AchievementValue(u32);

impl TryFrom<u32> for AchievementValue {
    type Error = u32;

    fn try_from(v: u32) -> Result<Self, u32> {
        match v {
            0..=101_0000 => Ok(Self(v)),
            _ => Err(v),
        }
    }
}

enum AchievementRank {
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

struct DeluxscoreResult {
    score: ValueWithMax<u32>,
    rank: DeluxscoreRank,
    new_record: bool,
}

struct DeluxscoreRank(u8);

impl TryFrom<u8> for DeluxscoreRank {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=4 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}
struct FullComboResult {
    kind: FullComboKind,
    combo: ValueWithMax<u32>,
}

enum FullComboKind {
    Nothing,
    FullCombo,
    FullComboPlus,
    AllPerfect,
    AllPerfectPlus,
}

struct PerfectChallengeResult {
    life: u32,
    total_life: u32,
}

struct RatingResult {
    rating: u16,
    delta: i16,
    delta_sign: RatingDeltaSign,
    border_color: RatingBorderColor,
    grade_icon: Url,
}

enum RatingDeltaSign {
    Up,
    Keep,
    Down,
}

// ["normal", "blue", "green", "orange", "red", "purple", "bronze", "silver", "gold", "rainbow"]
enum RatingBorderColor {
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

struct TourMemberList(Vec<TourMember>);

impl TryFrom<Vec<TourMember>> for TourMemberList {
    type Error = Vec<TourMember>;
    fn try_from(value: Vec<TourMember>) -> Result<Self, Self::Error> {
        match value.len() {
            1..=5 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

struct TourMember {
    star: u32,
    icon: Url,
    level: u32,
}

struct ValueWithMax<T: PartialOrd> {
    value: T,
    max: T,
}

impl<T: PartialOrd> ValueWithMax<T> {
    fn new(value: T, max: T) -> Result<Self, (T, T)> {
        if value <= max {
            Ok(Self { value, max })
        } else {
            Err((value, max))
        }
    }
}

struct JudgeResult {
    fast: u32,
    late: u32,
    tap: JudgeCount,
    hold: JudgeCount,
    slide: JudgeCount,
    touch: JudgeCount,
    break_: JudgeCountWithCP,
}

enum JudgeCount {
    Nothing,
    JudgeCountWithoutCP(JudgeCountWithoutCP),
    JudgeCountWithCP(JudgeCountWithCP),
}

struct JudgeCountWithCP {
    critical_perfect: u32,
    others: JudgeCountWithoutCP,
}

struct JudgeCountWithoutCP {
    perfect: u32,
    great: u32,
    good: u32,
    miss: u32,
}

struct MatchingResult {
    kind: FullSyncKind,
    sync: ValueWithMax<u32>,
    other_players: Vec<OtherPlayer>,
    rank: MatchingRank,
}

enum FullSyncKind {
    Nothing,
    FullSync,
    FullSyncPlus,
    FullSyncDx,
    FullSyncDxPlus,
}

struct OtherPlayer {
    difficulty: ScoreDifficulty,
    user_name: String,
}

struct MatchingRank(u8);

impl TryFrom<u8> for MatchingRank {
    type Error = u8;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1..=4 => Ok(Self(value)),
            _ => Err(value),
        }
    }
}

use std::fmt::Display;

use chrono::NaiveDateTime;
use deranged::U8;
use derive_more::{AsRef, Display, From, FromStr, Into};
use getset::{CopyGetters, Getters};
use num_format::{Locale, WriteFormatted};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use url::Url;

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct PlayRecord {
    played_at: PlayedAt,
    song_metadata: SongMetadata,
    score_metadata: ScoreMetadata,
    battle_result: BattleResult,
    technical_result: TechnicalResult,
    combo_result: ComboResult,
    bell_result: BellResult,
    judge_result: JudgeResult,
    damage_count: DamageCount,
    achievement_per_note_kind: AchievementPerNoteKindResult,
    battle_participants: BattleParticipants,
    mission_result: MissionResult,
}

#[derive(
    Clone, PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize,
)]
pub struct PlayedAt {
    #[getset(get_copy = "pub")]
    idx: Idx,
    #[getset(get_copy = "pub")]
    time: PlayTime,
    #[getset(get = "pub")]
    place: PlayPlace,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Into, Display, Serialize, Deserialize)]
pub struct Idx(U8<0, 49>);
impl TryFrom<u8> for Idx {
    type Error = <U8<0, 49> as TryFrom<u8>>::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(Idx(U8::try_from(value)?))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, Into, Display, Serialize, Deserialize)]
pub struct PlayTime(NaiveDateTime);

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct PlayPlace(String);

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct SongMetadata {
    name: SongName,
    cover_art: SongCoverArtUrl,
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct SongName(String);

#[derive(Clone, PartialEq, Eq, Debug, From, FromStr, AsRef, Display, Serialize, Deserialize)]
pub struct SongCoverArtUrl(Url);

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct ScoreMetadata {
    #[getset(get_copy = "pub")]
    difficulty: ScoreDifficulty,
    #[getset(get = "pub")]
    id: ScoreId,
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct ScoreId(String);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    Lunatic,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct BattleResult {
    rank: BattleRank,
    score: ValueWithNewRecord<BattleScore>,
    over_damage: ValueWithNewRecord<OverDamage>,
    win_or_lose: WinOrLose,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BattleRank {
    // /// 不可
    // Bad,
    /// 可 (gray, uncleared)
    FairLose,
    /// 可 (blue, cleared)
    FairCleared,
    /// 良
    Good,
    /// 優
    Great,
    /// 秀
    Excellent,
    // /// 極 (platinum)
    // UltimatePlatinum,
    // /// 極 (rainbow)
    // UltimateRainbow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, Into, Serialize, Deserialize)]
pub struct BattleScore(u32);
impl Display for BattleScore {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match f.write_formatted(&self.0, &Locale::en) {
            Ok(_) => Ok(()),
            Err(_) => Err(std::fmt::Error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, Into, Serialize, Deserialize)]
/// Multiplied by x100, it represents the first two fractional digits
pub struct OverDamage(u32);
impl Display for OverDamage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WinOrLose {
    Win,
    Draw,
    Lose,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct TechnicalResult {
    score: ValueWithNewRecord<TechnicalScore>,
    rank: TechnicalRank,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, Into, Serialize, Deserialize)]
pub struct TechnicalScore(u32);
impl Display for TechnicalScore {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match f.write_formatted(&self.0, &Locale::en) {
            Ok(_) => Ok(()),
            Err(_) => Err(std::fmt::Error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum TechnicalRank {
    SSSPlus,
    SSS,
    SS,
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
pub struct ValueWithNewRecord<T: Copy> {
    value: T,
    new_record: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct ComboResult {
    max_combo: ComboCount,
    full_combo_kind: FullComboKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Serialize, Deserialize)]
pub struct ComboCount(u32);
impl Display for ComboCount {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match f.write_formatted(&self.0, &Locale::en) {
            Ok(_) => Ok(()),
            Err(_) => Err(std::fmt::Error),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FullComboKind {
    Nothing,
    FullCombo,
    AllBreak,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct BellResult {
    count: BellCount,
    max: BellCount,
    full_bell_kind: FullBellKind,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct BellCount(u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FullBellKind {
    Nothing,
    FullBell,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct JudgeResult {
    critical_break: JudgeCount,
    break_: JudgeCount,
    hit: JudgeCount,
    miss: JudgeCount,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Serialize, Deserialize)]
pub struct JudgeCount(u32);
impl Display for JudgeCount {
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match f.write_formatted(&self.0, &Locale::en) {
            Ok(_) => Ok(()),
            Err(_) => Err(std::fmt::Error),
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct DamageCount(u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug, TypedBuilder, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct AchievementPerNoteKindResult {
    tap: Option<AchievementPerNoteKind>,
    hold: Option<AchievementPerNoteKind>,
    flick: Option<AchievementPerNoteKind>,
    side_tap: Option<AchievementPerNoteKind>,
    side_hold: Option<AchievementPerNoteKind>,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct AchievementPerNoteKind(u32);

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, Serialize, Deserialize)]
#[getset(get = "pub")]
pub struct BattleParticipants {
    opponent: BattleOpponent,
    deck: [DeckCard; 3],
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct BattleOpponent {
    #[getset(get = "pub")]
    name: BattleOpponentName,
    #[getset(get_copy = "pub")]
    level: BattleParticipantLevel,
    #[getset(get_copy = "pub")]
    color: BattleOpponentColor,
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct BattleOpponentName(String);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum BattleOpponentColor {
    Fire,
    Leaf,
    Aqua,
}

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct DeckCard {
    #[getset(get_copy = "pub")]
    level: BattleParticipantLevel,
    #[getset(get_copy = "pub")]
    power: DeckCardPower,
    #[getset(get = "pub")]
    card_image: DeckCardUrl,
}

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct BattleParticipantLevel(u32);

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct DeckCardPower(u32);

#[derive(Clone, PartialEq, Eq, Debug, From, FromStr, AsRef, Display, Serialize, Deserialize)]
pub struct DeckCardUrl(Url);

#[derive(PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize)]
pub struct MissionResult {
    #[getset(get = "pub")]
    name: MissionName,
    #[getset(get_copy = "pub")]
    score: MissionScore,
}

#[derive(Clone, PartialEq, Eq, Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct MissionName(String);

#[derive(
    Clone, Copy, PartialEq, Eq, Debug, From, FromStr, Into, Display, Serialize, Deserialize,
)]
pub struct MissionScore(u32);

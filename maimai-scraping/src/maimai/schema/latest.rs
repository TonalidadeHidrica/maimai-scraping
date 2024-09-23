use std::hash::Hash;

use anyhow::bail;
use chrono::naive::NaiveDateTime;
use chrono::FixedOffset;
use enum_map::Enum;
use getset::{CopyGetters, Getters};
use log::warn;
use maimai_scraping_utils::regex;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt::Display;
use std::num::ParseIntError;
use std::str::FromStr;
use strum::EnumIter;
use thiserror::Error;
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
    #[getset(get = "pub")]
    utage_metadata: Option<UtageMetadata>,
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
    place: Option<PlaceName>,
    #[getset(get_copy = "pub")]
    track: TrackIndex,
}

// Default is used for Idx(0), which is valid
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct Idx {
    index: u8,
    timestamp: Option<NaiveDateTime>,
}
impl Idx {
    pub fn timestamp_jst(self) -> Option<PlayTime> {
        self.timestamp.map(PlayTime::from_utc)
    }
}

#[derive(PartialEq, Eq, Debug, Error)]
pub enum IdxParseError {
    #[error("Value cannot be parsed as an integer: {0}")]
    ParseIntError(#[from] ParseIntError),
    #[error("Value is not less than 50: {0}")]
    IndexOutOfRange(u8),
    #[error("Error while parsing timestamp: {0}")]
    TimestampParseError(#[from] chrono::format::ParseError),
}
impl FromStr for Idx {
    type Err = IdxParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (index, timestamp) = match s.split_once(',') {
            Some((index, timestamp)) => {
                let timestamp = NaiveDateTime::parse_from_str(timestamp, "%s")?;
                (index, Some(timestamp))
            }
            _ => (s, None),
        };
        let index = index.parse()?;
        let index = match index {
            0..=49 => Ok(index),
            _ => Err(IdxParseError::IndexOutOfRange(index)),
        }?;
        Ok(Idx { index, timestamp })
    }
}
impl Display for Idx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.index)?;
        if let Some(timestamp) = self.timestamp {
            write!(f, ",{}", timestamp.format("%s"))?;
        }
        Ok(())
    }
}

// impl TryFrom<u8> for Idx {
//     type Error = u8;
//     fn try_from(value: u8) -> Result<Self, Self::Error> {
//         match value {
//             0..=49 => Ok(Self(value)),
//             _ => Err(value),
//         }
//     }
// }

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Debug,
    derive_more::From,
    derive_more::Into,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
pub struct PlayTime(NaiveDateTime);

impl PlayTime {
    pub fn get(self) -> NaiveDateTime {
        self.0
    }

    pub(crate) fn from_utc(time: NaiveDateTime) -> PlayTime {
        time.and_utc()
            .with_timezone(&FixedOffset::east_opt(9 * 60 * 60).unwrap())
            .naive_local()
            .into()
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
            1.. => Ok(Self(value)),
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
    PartialOrd,
    Ord,
    Hash,
    Debug,
    derive_more::From,
    derive_more::AsRef,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
#[as_ref(forward)]
pub struct SongName(String);

#[derive(
    Clone, derive_more::From, derive_more::FromStr, derive_more::Display, Serialize, Deserialize,
)]
pub struct SongIcon(Url);

impl SongIcon {
    pub fn standard_part(&self) -> Option<&str> {
        let url = self.0.as_str();
        let ret = regex!(
            r"https://(maimaidx.jp|maimaidx-eng.com)/maimai-mobile/img/Music/([0-9a-f]{16}).png"
        )
        .captures(url)
        .map(|url| url.get(2).unwrap().as_str());
        if ret.is_none() {
            warn!("Song icon url is not in expcected format: {url:?}");
        }
        ret
    }

    fn comparator(&self) -> (bool, &str) {
        let res = self.standard_part();
        (res.is_some(), res.unwrap_or(self.0.as_str()))
    }
}
impl PartialEq for SongIcon {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for SongIcon {}
impl PartialOrd for SongIcon {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SongIcon {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.comparator()).cmp(&other.comparator())
    }
}
impl Hash for SongIcon {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.comparator().hash(state);
    }
}
impl std::fmt::Debug for SongIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.comparator().1)
    }
}

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

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize, Enum,
)]
pub enum ScoreGeneration {
    Standard,
    Deluxe,
}
impl ScoreGeneration {
    pub fn abbrev(self) -> &'static str {
        use ScoreGeneration::*;
        match self {
            Standard => "Std",
            Deluxe => "DX",
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize, EnumIter,
)]
pub enum ScoreDifficulty {
    Basic,
    Advanced,
    Expert,
    Master,
    ReMaster,
    Utage,
}
impl ScoreDifficulty {
    pub fn abbrev(self) -> &'static str {
        use ScoreDifficulty::*;
        match self {
            Basic => "Bas",
            Advanced => "Adv",
            Expert => "Exp",
            Master => "Mas",
            ReMaster => "ReMas",
            Utage => "Utg",
        }
    }

    pub fn abbrev_kanji(self) -> char {
        use ScoreDifficulty::*;
        match self {
            Basic => '緑',
            Advanced => '黄',
            Expert => '赤',
            Master => '紫',
            ReMaster => '白',
            Utage => '宴',
        }
    }
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct ValueWithMax<T: PartialOrd + Copy> {
    value: T,
    max: T,
}

impl<T: PartialOrd + Copy> ValueWithMax<T> {
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
#[getset(get_copy = "pub")]
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
    SyncPlay,
    FullSync,
    FullSyncPlus,
    FullSyncDx,
    FullSyncDxPlus,
}

#[derive(PartialEq, Eq, Debug, derive_more::AsRef, Serialize, Deserialize)]
#[as_ref(forward)]
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

#[derive(
    Clone, PartialEq, Eq, PartialOrd, Ord, Debug, derive_more::From, Serialize, Deserialize,
)]
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

#[derive(
    Clone, PartialEq, Eq, Debug, TypedBuilder, Getters, CopyGetters, Serialize, Deserialize,
)]
pub struct UtageMetadata {
    #[getset(get = "pub")]
    kind: UtageKind,
    #[getset(get_copy = "pub")]
    buddy: bool,
}
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum UtageKind {
    /// 光
    AllBreak,
    /// 協
    Collaborative,
    /// 狂
    Insane,
    /// 蛸
    ManyHands,
    /// 覚
    Memorize,
    /// 宴
    Miscellaneous,
    /// 蔵
    Shelved,
    /// 星
    Slides,

    // I gave up enumearting all possible utage kinds.
    Raw(UtageKindRaw),
}

#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    derive_more::From,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
pub struct UtageKindRaw(String);
impl From<UtageKind> for UtageKindRaw {
    fn from(value: UtageKind) -> Self {
        match value {
            UtageKind::AllBreak => "光",
            UtageKind::Collaborative => "協",
            UtageKind::Insane => "光",
            UtageKind::ManyHands => "蛸",
            UtageKind::Memorize => "覚",
            UtageKind::Miscellaneous => "宴",
            UtageKind::Shelved => "蔵",
            UtageKind::Slides => "星",
            UtageKind::Raw(value) => return value,
        }
        .to_owned()
        .into()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Category {
    GamesVariety,
    PopsAnime,
    MaimaiOriginal,
    NiconicoVocaloid,
    OngekiChunithm,
    TouhouProject,
}

#[derive(
    Clone,
    PartialEq,
    Eq,
    Hash,
    Debug,
    derive_more::From,
    derive_more::AsRef,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
#[as_ref(forward)]
pub struct ArtistName(String);

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{Idx, IdxParseError as E, PlayTime};

    #[test]
    fn parse_idx() {
        assert_eq!(
            "0".parse::<Idx>(),
            Ok(Idx {
                index: 0,
                timestamp: None
            })
        );
        assert_eq!(
            "12".parse::<Idx>(),
            Ok(Idx {
                index: 12,
                timestamp: None
            })
        );
        assert_eq!(
            "49".parse::<Idx>(),
            Ok(Idx {
                index: 49,
                timestamp: None
            })
        );
        assert_eq!("50".parse::<Idx>(), Err(E::IndexOutOfRange(50)));
        assert_eq!("255".parse::<Idx>(), Err(E::IndexOutOfRange(255)));
        assert!(matches!("256".parse::<Idx>(), Err(E::ParseIntError(_))));
        let timestamp = Some(
            NaiveDate::from_ymd_opt(2023, 9, 13)
                .unwrap()
                .and_hms_opt(16, 55, 3)
                .unwrap(),
        );
        assert_eq!(
            "0,1694624103".parse::<Idx>(),
            Ok(Idx {
                index: 0,
                timestamp
            })
        );
        assert_eq!(
            "12,1694624103".parse::<Idx>(),
            Ok(Idx {
                index: 12,
                timestamp
            })
        );
        assert_eq!(
            "49,1694624103".parse::<Idx>(),
            Ok(Idx {
                index: 49,
                timestamp
            })
        );
        assert_eq!("50,1694624103".parse::<Idx>(), Err(E::IndexOutOfRange(50)));
        assert_eq!(
            "255,1694624103".parse::<Idx>(),
            Err(E::IndexOutOfRange(255))
        );
        assert!(matches!(
            "256,1694624103".parse::<Idx>(),
            Err(E::ParseIntError(_))
        ));

        assert!(matches!(
            "255,abc,def".parse::<Idx>(),
            Err(E::TimestampParseError(_))
        ));
        assert!(matches!("abcdef".parse::<Idx>(), Err(E::ParseIntError(_))));
    }

    #[test]
    fn display_idx() {
        assert_eq!(
            &Idx {
                index: 0,
                timestamp: None
            }
            .to_string(),
            "0"
        );
        assert_eq!(
            &Idx {
                index: 32,
                timestamp: None
            }
            .to_string(),
            "32"
        );
        let timestamp = Some(
            NaiveDate::from_ymd_opt(2023, 9, 13)
                .unwrap()
                .and_hms_opt(16, 55, 3)
                .unwrap(),
        );
        assert_eq!(
            &Idx {
                index: 12,
                timestamp
            }
            .to_string(),
            "12,1694624103",
        );
    }

    #[test]
    fn play_time_from_utc() {
        let utc = NaiveDate::from_ymd_opt(2023, 9, 13)
            .unwrap()
            .and_hms_opt(16, 55, 3)
            .unwrap();
        let jst = NaiveDate::from_ymd_opt(2023, 9, 14)
            .unwrap()
            .and_hms_opt(1, 55, 3)
            .unwrap();
        assert_eq!(PlayTime::from_utc(utc), PlayTime::from(jst));
    }
}

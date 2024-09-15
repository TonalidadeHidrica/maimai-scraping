use std::{
    fmt::{Debug, Display},
    marker::PhantomData,
    path::PathBuf,
};

use anyhow::{anyhow, bail};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use derive_more::Display;
use enum_iterator::Sequence;
use enum_map::Enum;
use getset::{CopyGetters, Getters};
use hashbrown::{HashMap, HashSet};
use itertools::{iterate, Itertools};
use maimai_scraping_utils::fs_json_util::read_json;
use serde::{Deserialize, Deserializer, Serialize};
use strum::EnumString;
use url::Url;

use super::{
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
};

pub fn load(path: impl Into<PathBuf> + Debug) -> anyhow::Result<Vec<Song>> {
    load_impl(path)
}
pub fn load_mask(
    path: impl Into<PathBuf> + Debug,
) -> anyhow::Result<Vec<Song<in_lv_kind::Bitmask>>> {
    load_impl(path)
}
fn load_impl<K: in_lv_kind::Kind>(
    path: impl Into<PathBuf> + Debug,
) -> anyhow::Result<Vec<Song<K>>> {
    let songs: Vec<SongRaw<K>> = read_json(path)?;
    songs.into_iter().map(Song::<K>::try_from).collect()
}
pub fn make_map<T, I, U, F>(songs: I, mut key: F) -> anyhow::Result<HashMap<U, T>>
where
    T: std::fmt::Debug,
    I: IntoIterator<Item = T>,
    U: std::hash::Hash + std::cmp::Eq,
    F: for<'t> FnMut(&'t T) -> U,
{
    let mut map = HashMap::new();
    for song in songs {
        let key = key(&song);
        if let Some(entry) = map.insert(key, song) {
            bail!("Duplicating key: {entry:?}");
        }
    }
    Ok(map)
}
pub fn make_hash_multimap<K, V, I>(elems: I) -> HashMap<K, HashSet<V>>
where
    K: std::hash::Hash + std::cmp::Eq,
    V: std::hash::Hash + std::cmp::Eq,
    I: IntoIterator<Item = (K, V)>,
{
    let mut map = HashMap::<_, HashSet<V>>::new();
    for (k, v) in elems {
        map.entry(k).or_default().insert(v);
    }
    map
}

pub mod in_lv_kind {
    use std::{convert::Infallible, fmt::Debug, hash::Hash};

    use sealed::sealed;

    use super::{CandidateBitmask, InternalScoreLevel};

    #[sealed]
    pub trait Kind: Copy + Ord + Hash + Debug {
        type Value: Copy + Eq + Debug + TryFrom<f64, Error = anyhow::Error>;
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct Levels(Infallible);
    #[sealed]
    impl Kind for Levels {
        type Value = InternalScoreLevel;
    }
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
    pub struct Bitmask(Infallible);
    #[sealed]
    impl Kind for Bitmask {
        type Value = CandidateBitmask;
    }
}
pub use in_lv_kind::Kind as InLvKind;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SongRaw<K = in_lv_kind::Levels> {
    pub dx: u8,
    pub v: i8,
    pub lv: Vec<f64>,
    pub n: String,
    pub nn: Option<String>,
    pub ico: String,
    #[serde(skip)]
    pub _phantom: PhantomData<fn() -> K>,
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Getters, CopyGetters)]
pub struct Song<K: InLvKind = in_lv_kind::Levels> {
    #[getset(get_copy = "pub")]
    generation: ScoreGeneration,
    #[getset(get_copy = "pub")]
    version: MaimaiVersion,
    #[getset(get_copy = "pub")]
    levels: ScoreLevels<K>,
    #[getset(get = "pub")]
    song_name: SongName,
    #[getset(get = "pub")]
    song_name_abbrev: String,
    #[getset(get = "pub")]
    icon: SongIcon,
}
impl<K: InLvKind> TryFrom<SongRaw<K>> for Song<K> {
    type Error = anyhow::Error;
    fn try_from(song: SongRaw<K>) -> anyhow::Result<Self> {
        let entry = |index: usize| InternalScoreLevelEntry::<K>::new(song.lv[index], index);
        let zero = song.lv[4].abs() < 1e-8;
        let re_master = match song.lv.len() {
            6 => {
                if !zero {
                    bail!("song.lv[4] is not zero, but there are 6 elements");
                } else {
                    Some(entry(5)?)
                }
            }
            5 => (!zero).then(|| entry(4)).transpose()?,
            _ => bail!("Unexpected length: {:?}", song.lv),
        };
        Ok(Self {
            generation: match song.dx {
                0 => ScoreGeneration::Standard,
                1 => ScoreGeneration::Deluxe,
                _ => bail!("Unexpected generation: {}", song.dx),
            },
            version: song.v.try_into()?,
            levels: ScoreLevels {
                basic: entry(0)?,
                advanced: entry(1)?,
                expert: entry(2)?,
                master: entry(3)?,
                re_master,
            },
            song_name_abbrev: song.nn.to_owned().unwrap_or_else(|| song.n.to_owned()),
            song_name: song.n.into(),
            // TODO: to support intl, cosider how to ignore domain
            icon: Url::parse(&format!(
                "https://maimaidx.jp/maimai-mobile/img/Music/{}.png",
                song.ico
            ))?
            .into(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, CopyGetters)]
pub struct ScoreLevels<K: InLvKind = in_lv_kind::Levels> {
    basic: InternalScoreLevelEntry<K>,
    advanced: InternalScoreLevelEntry<K>,
    expert: InternalScoreLevelEntry<K>,
    master: InternalScoreLevelEntry<K>,
    re_master: Option<InternalScoreLevelEntry<K>>,
}
impl<K: InLvKind> ScoreLevels<K> {
    pub fn get(&self, difficulty: ScoreDifficulty) -> Option<K::Value> {
        use ScoreDifficulty::*;
        Some(match difficulty {
            Basic => self.basic.value,
            Advanced => self.advanced.value,
            Expert => self.expert.value,
            Master => self.master.value,
            ReMaster => self.re_master?.value,
            Utage => None?, // TODO support utage?
        })
    }
}

impl<K: InLvKind> ScoreLevels<K> {
    pub fn iter(&self) -> impl Iterator<Item = (ScoreDifficulty, InternalScoreLevelEntry<K>)> {
        use ScoreDifficulty::*;
        [
            (Basic, self.basic),
            (Advanced, self.advanced),
            (Expert, self.expert),
            (Master, self.master),
        ]
        .into_iter()
        .chain(self.re_master.map(|x| (ReMaster, x)))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct InternalScoreLevelEntry<K: InLvKind = in_lv_kind::Levels> {
    value: <K as InLvKind>::Value,
    index: usize,
}
impl<K: InLvKind> InternalScoreLevelEntry<K> {
    fn new(value: f64, index: usize) -> anyhow::Result<Self> {
        Ok(Self {
            value: value.try_into()?,
            index,
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug, Display, Serialize, Deserialize)]
pub enum InternalScoreLevel {
    Unknown(ScoreLevel),
    Known(ScoreConstant),
}
impl TryFrom<f64> for InternalScoreLevel {
    type Error = anyhow::Error;

    fn try_from(value: f64) -> anyhow::Result<Self> {
        let int = (value.abs() * 10.).round() as u8;
        Ok(if value > 0. {
            Self::Known(
                ScoreConstant::try_from(int)
                    .map_err(|_| anyhow!("Out-of-range known value: {value}"))?,
            )
        } else {
            let plus = match int % 10 {
                0 => false,
                // Keeping both 6 and 7 for compatibility
                6 | 7 => true,
                _ => bail!("Absurd fractional part for unknown value: {value}"),
            };
            Self::Unknown(
                ScoreLevel::new(int / 10, plus)
                    .map_err(|_| anyhow!("Out-of-range unknown value: {value}"))?,
            )
        })
    }
}
impl InternalScoreLevel {
    pub fn get_if_unique(self) -> Option<ScoreConstant> {
        match self {
            InternalScoreLevel::Unknown(_) => None,
            // InternalScoreLevel::Candidates(..) => None,
            InternalScoreLevel::Known(x) => Some(x),
        }
    }

    pub fn is_unique(self) -> bool {
        self.get_if_unique().is_some()
    }

    pub fn into_level(self, version: MaimaiVersion) -> ScoreLevel {
        match self {
            InternalScoreLevel::Unknown(v) => v,
            // InternalScoreLevel::Candidates(v) => v.level.to_lv(version),
            InternalScoreLevel::Known(v) => v.to_lv(version),
        }
    }
}

#[non_exhaustive]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Debug,
    EnumString,
    Serialize,
    Deserialize,
    Sequence,
    Enum,
)]
pub enum MaimaiVersion {
    Maimai,
    MaimaiPlus,
    Green,
    GreenPlus,
    Orange,
    OrangePlus,
    Pink,
    PinkPlus,
    Murasaki,
    MurasakiPlus,
    Milk,
    MilkPlus,
    Finale,
    Deluxe,
    DeluxePlus,
    Splash,
    SplashPlus,
    Universe,
    UniversePlus,
    Festival,
    FestivalPlus,
    Buddies,
    BuddiesPlus,
    Prism,
}
impl TryFrom<i8> for MaimaiVersion {
    type Error = anyhow::Error;
    fn try_from(v: i8) -> anyhow::Result<Self> {
        use MaimaiVersion::*;
        Ok(match v.abs() {
            0 => Maimai,
            1 => MaimaiPlus,
            2 => Green,
            3 => GreenPlus,
            4 => Orange,
            5 => OrangePlus,
            6 => Pink,
            7 => PinkPlus,
            8 => Murasaki,
            9 => MurasakiPlus,
            10 => Milk,
            11 => MilkPlus,
            12 => Finale,
            13 => Deluxe,
            14 => DeluxePlus,
            15 => Splash,
            16 => SplashPlus,
            17 => Universe,
            18 => UniversePlus,
            19 => Festival,
            20 => FestivalPlus,
            21 => Buddies,
            22 => BuddiesPlus,
            23 => Prism,
            _ => bail!("Unexpected version: {v}"),
        })
    }
}
impl From<MaimaiVersion> for i8 {
    fn from(v: MaimaiVersion) -> i8 {
        use MaimaiVersion::*;
        match v {
            Maimai => 0,
            MaimaiPlus => 1,
            Green => 2,
            GreenPlus => 3,
            Orange => 4,
            OrangePlus => 5,
            Pink => 6,
            PinkPlus => 7,
            Murasaki => 8,
            MurasakiPlus => 9,
            Milk => 10,
            MilkPlus => 11,
            Finale => 12,
            Deluxe => 13,
            DeluxePlus => 14,
            Splash => 15,
            SplashPlus => 16,
            Universe => 17,
            UniversePlus => 18,
            Festival => 19,
            FestivalPlus => 20,
            Buddies => 21,
            BuddiesPlus => 22,
            Prism => 23,
        }
    }
}
impl MaimaiVersion {
    pub fn start_date(self) -> NaiveDate {
        use MaimaiVersion::*;
        match self {
            Maimai => NaiveDate::from_ymd_opt(2012, 7, 12).unwrap(),
            MaimaiPlus => NaiveDate::from_ymd_opt(2012, 12, 13).unwrap(),
            Green => NaiveDate::from_ymd_opt(2013, 7, 11).unwrap(),
            GreenPlus => NaiveDate::from_ymd_opt(2014, 2, 26).unwrap(),
            Orange => NaiveDate::from_ymd_opt(2014, 9, 18).unwrap(),
            OrangePlus => NaiveDate::from_ymd_opt(2015, 3, 19).unwrap(),
            Pink => NaiveDate::from_ymd_opt(2015, 12, 9).unwrap(),
            PinkPlus => NaiveDate::from_ymd_opt(2016, 6, 30).unwrap(),
            Murasaki => NaiveDate::from_ymd_opt(2016, 12, 14).unwrap(),
            MurasakiPlus => NaiveDate::from_ymd_opt(2017, 6, 22).unwrap(),
            Milk => NaiveDate::from_ymd_opt(2017, 12, 14).unwrap(),
            MilkPlus => NaiveDate::from_ymd_opt(2018, 6, 21).unwrap(),
            Finale => NaiveDate::from_ymd_opt(2018, 12, 13).unwrap(),
            Deluxe => NaiveDate::from_ymd_opt(2019, 7, 11).unwrap(),
            DeluxePlus => NaiveDate::from_ymd_opt(2020, 1, 23).unwrap(),
            Splash => NaiveDate::from_ymd_opt(2020, 9, 17).unwrap(),
            SplashPlus => NaiveDate::from_ymd_opt(2021, 3, 18).unwrap(),
            Universe => NaiveDate::from_ymd_opt(2021, 9, 16).unwrap(),
            UniversePlus => NaiveDate::from_ymd_opt(2022, 3, 24).unwrap(),
            Festival => NaiveDate::from_ymd_opt(2022, 9, 15).unwrap(),
            FestivalPlus => NaiveDate::from_ymd_opt(2023, 3, 23).unwrap(),
            Buddies => NaiveDate::from_ymd_opt(2023, 9, 14).unwrap(),
            BuddiesPlus => NaiveDate::from_ymd_opt(2024, 3, 21).unwrap(),
            Prism => NaiveDate::from_ymd_opt(2024, 9, 12).unwrap(),
        }
    }
    pub fn start_time(self) -> NaiveDateTime {
        self.start_date()
            .and_time(NaiveTime::from_hms_opt(6, 0, 0).unwrap())
    }
    pub fn end_time(self) -> NaiveDateTime {
        match self.next() {
            Some(next) => next
                .start_date()
                .and_time(NaiveTime::from_hms_opt(6, 0, 0).unwrap()),
            None => NaiveDateTime::MAX,
        }
    }
    pub fn of_time(time: NaiveDateTime) -> Option<MaimaiVersion> {
        enum_iterator::all()
            .find(|v: &MaimaiVersion| (v.start_time()..v.end_time()).contains(&time))
    }
    pub fn of_date(x: NaiveDate) -> Option<MaimaiVersion> {
        enum_iterator::all().find(|v: &MaimaiVersion| {
            v.start_date() <= x && v.next().map_or(true, |v| x < v.start_date())
        })
    }
    pub fn latest() -> Self {
        Self::Prism
    }
}

#[derive(Debug, Deserialize, Getters)]
pub struct RemovedSong {
    #[getset(get = "pub")]
    icon: SongIcon,
    #[getset(get = "pub")]
    name: SongName,
    #[getset(get = "pub")]
    date: NaiveDate,
    #[getset(get = "pub")]
    #[serde(default, deserialize_with = "deserialize_levels")]
    levels: Option<Song>,
}

fn deserialize_levels<'de, D>(deserializer: D) -> Result<Option<Song>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<SongRaw>::deserialize(deserializer)?
        .map(|song| Song::try_from(song).map_err(serde::de::Error::custom))
        .transpose()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct Candidates {
    level: ScoreConstant,
    mask: CandidateBitmask,
}
impl Display for Candidates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = u8::from(self.level);
        write!(
            f,
            "{}.{}",
            x / 10,
            self.mask.bits().map(|i| i + x % 10).join(","),
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct CandidateBitmask(u16);
impl TryFrom<f64> for CandidateBitmask {
    type Error = anyhow::Error;

    fn try_from(value: f64) -> anyhow::Result<Self> {
        let mask = value as u16;
        if mask as f64 != value {
            bail!("Unexpceted value (possibly fractional): {value}");
        }
        if mask > (1 << 10) {
            bail!("Too large mask: {value}");
        }
        Ok(Self(mask))
    }
}
impl CandidateBitmask {
    pub fn has(self, x: u8) -> bool {
        (x as u32) < u8::BITS && ((1 << x) & self.0) > 0
    }
    pub fn bits(self) -> impl Iterator<Item = u8> {
        iterate(self.0, |x| x >> 1)
            .enumerate()
            .take_while(|&(_, x)| x > 0)
            .filter_map(|(i, x)| ((x & 1) > 0).then_some(i as u8))
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::CandidateBitmask;

    #[test]
    fn test_candidate_bitmask() {
        let x = CandidateBitmask(0b0100_1011);
        assert!(x.has(0));
        assert!(!x.has(2));
        assert!(x.has(6));
        assert!(!x.has(7));
        assert!(!x.has(15));
        assert!(!x.has(16));
        assert!(!x.has(255));
        assert!(x.bits().collect_vec() == [0, 1, 3, 6])
    }
}

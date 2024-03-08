use std::path::PathBuf;

use anyhow::{anyhow, bail};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use getset::{CopyGetters, Getters};
use hashbrown::{HashMap, HashSet};
use serde::Deserialize;
use url::Url;

use crate::fs_json_util::read_json;

use super::{
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
};

pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Vec<Song>> {
    let songs: Vec<SongRaw> = read_json(path)?;
    songs.into_iter().map(Song::try_from).collect()
}
pub fn make_map<'t, T, U, F>(songs: &'t [T], mut key: F) -> anyhow::Result<HashMap<U, &'t T>>
where
    T: std::fmt::Debug,
    U: std::hash::Hash + std::cmp::Eq,
    F: FnMut(&'t T) -> U,
{
    let mut map = HashMap::new();
    for song in songs {
        if let Some(entry) = map.insert(key(song), song) {
            bail!("Duplicating icon url: {entry:?}");
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

#[derive(Debug, Deserialize)]
pub struct SongRaw {
    pub dx: u8,
    pub v: i8,
    pub lv: Vec<f64>,
    pub n: String,
    pub ico: String,
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Getters, CopyGetters)]
pub struct Song {
    #[getset(get_copy = "pub")]
    generation: ScoreGeneration,
    #[getset(get_copy = "pub")]
    version: MaimaiVersion,
    #[getset(get_copy = "pub")]
    levels: ScoreLevels,
    #[getset(get = "pub")]
    song_name: SongName,
    #[getset(get = "pub")]
    icon: SongIcon,
}
impl TryFrom<SongRaw> for Song {
    type Error = anyhow::Error;
    fn try_from(song: SongRaw) -> anyhow::Result<Self> {
        let entry = |index: usize| InternalScoreLevelEntry::new(song.lv[index], index);
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
            song_name: song.n.into(),
            icon: Url::parse(&format!(
                "https://maimaidx.jp/maimai-mobile/img/Music/{}.png",
                song.ico
            ))?
            .into(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScoreLevels {
    basic: InternalScoreLevelEntry,
    advanced: InternalScoreLevelEntry,
    expert: InternalScoreLevelEntry,
    master: InternalScoreLevelEntry,
    re_master: Option<InternalScoreLevelEntry>,
}
impl ScoreLevels {
    pub fn get(&self, difficulty: ScoreDifficulty) -> Option<InternalScoreLevel> {
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

    pub fn iter(&self) -> impl Iterator<Item = (ScoreDifficulty, InternalScoreLevelEntry)> {
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
pub struct InternalScoreLevelEntry {
    value: InternalScoreLevel,
    index: usize,
}
impl InternalScoreLevelEntry {
    fn new(value: f64, index: usize) -> anyhow::Result<Self> {
        Ok(Self {
            value: value.try_into()?,
            index,
        })
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
                7 => true,
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
    pub fn known(self) -> Option<ScoreConstant> {
        match self {
            InternalScoreLevel::Unknown(_) => None,
            InternalScoreLevel::Known(x) => Some(x),
        }
    }

    pub fn is_known(self) -> bool {
        self.known().is_some()
    }
}

#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
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
            _ => bail!("Unexpected version: {v}"),
        })
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
        }
    }
    pub fn start_time(self) -> NaiveDateTime {
        self.start_date()
            .and_time(NaiveTime::from_hms_opt(5, 0, 0).unwrap())
    }
    pub fn latest() -> Self {
        Self::Buddies
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
    levels: Option<SongRaw>,
}

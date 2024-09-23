use std::{fmt::Debug, marker::PhantomData, path::PathBuf};

use anyhow::{anyhow, bail};
use chrono::NaiveDate;
use derive_more::Display;
use getset::{CopyGetters, Getters};
use hashbrown::{HashMap, HashSet};
use maimai_scraping_utils::fs_json_util::read_json;
use serde::{Deserialize, Deserializer, Serialize};
use url::Url;

use crate::maimai::{
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
    version::MaimaiVersion,
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

    use super::InternalScoreLevel;
    use crate::maimai::rating::CandidateBitmask;

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

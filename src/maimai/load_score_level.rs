use std::{collections::HashMap, io::BufReader, path::PathBuf};

use anyhow::{anyhow, bail};
use chrono::NaiveDate;
use fs_err::File;
use getset::{CopyGetters, Getters};
use serde::Deserialize;
use url::Url;

use super::{
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration},
};

pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Vec<Song>> {
    let songs: Vec<SongRaw> = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    songs.into_iter().map(Song::try_from).collect()
}
pub fn make_map(songs: &[Song]) -> anyhow::Result<HashMap<(&Url, ScoreGeneration), &Song>> {
    let mut map = HashMap::new();
    for song in songs {
        if let Some(entry) = map.insert((&song.icon, song.generation), song) {
            bail!("Duplicating icon url: {entry:?}");
        }
    }
    Ok(map)
}

#[allow(unused)]
#[derive(Deserialize)]
struct SongRaw {
    dx: u8,
    v: i8,
    lv: Vec<f64>,
    n: String,
    ico: String,
}

#[allow(unused)]
#[derive(Debug, Getters, CopyGetters)]
pub struct Song {
    #[getset(get_copy = "pub")]
    generation: ScoreGeneration,
    #[getset(get_copy = "pub")]
    version: MaimaiVersion,
    #[getset(get_copy = "pub")]
    levels: ScoreLevels,
    #[getset(get = "pub")]
    song_name: String,
    #[getset(get = "pub")]
    icon: Url,
}
impl TryFrom<SongRaw> for Song {
    type Error = anyhow::Error;
    fn try_from(song: SongRaw) -> anyhow::Result<Self> {
        let zero = song.lv[4].abs() < 1e-8;
        let re_master = match song.lv.len() {
            6 => {
                if !zero {
                    bail!("song.lv[4] is not zero, but there are 6 elements");
                } else {
                    Some(song.lv[4].try_into()?)
                }
            }
            5 => (!zero).then(|| song.lv[4].try_into()).transpose()?,
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
                basic: song.lv[0].try_into()?,
                advanced: song.lv[1].try_into()?,
                expert: song.lv[2].try_into()?,
                master: song.lv[3].try_into()?,
                re_master,
            },
            song_name: song.n,
            icon: Url::parse(&format!(
                "https://maimaidx.jp/maimai-mobile/img/Music/{}.png",
                song.ico
            ))?,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ScoreLevels {
    basic: InternalScoreLevel,
    advanced: InternalScoreLevel,
    expert: InternalScoreLevel,
    master: InternalScoreLevel,
    re_master: Option<InternalScoreLevel>,
}
impl ScoreLevels {
    pub fn get(&self, difficulty: ScoreDifficulty) -> Option<InternalScoreLevel> {
        use ScoreDifficulty::*;
        Some(match difficulty {
            Basic => self.basic,
            Advanced => self.advanced,
            Expert => self.expert,
            Master => self.master,
            ReMaster => self.re_master?,
        })
    }
}

#[derive(Clone, Copy, Debug)]
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
            _ => bail!("Unexpected version: {v}"),
        })
    }
}
impl MaimaiVersion {
    pub fn start_date(self) -> NaiveDate {
        use MaimaiVersion::*;
        match self {
            Maimai => NaiveDate::from_ymd(2012, 7, 12),
            MaimaiPlus => NaiveDate::from_ymd(2012, 12, 13),
            Green => NaiveDate::from_ymd(2013, 7, 11),
            GreenPlus => NaiveDate::from_ymd(2014, 2, 26),
            Orange => NaiveDate::from_ymd(2014, 9, 18),
            OrangePlus => NaiveDate::from_ymd(2015, 3, 19),
            Pink => NaiveDate::from_ymd(2015, 12, 9),
            PinkPlus => NaiveDate::from_ymd(2016, 6, 30),
            Murasaki => NaiveDate::from_ymd(2016, 12, 14),
            MurasakiPlus => NaiveDate::from_ymd(2017, 6, 22),
            Milk => NaiveDate::from_ymd(2017, 12, 14),
            MilkPlus => NaiveDate::from_ymd(2018, 6, 21),
            Finale => NaiveDate::from_ymd(2018, 12, 13),
            Deluxe => NaiveDate::from_ymd(2019, 7, 11),
            DeluxePlus => NaiveDate::from_ymd(2020, 1, 23),
            Splash => NaiveDate::from_ymd(2020, 9, 17),
            SplashPlus => NaiveDate::from_ymd(2021, 3, 18),
            Universe => NaiveDate::from_ymd(2021, 9, 16),
            UniversePlus => NaiveDate::from_ymd(2022, 3, 24),
            Festival => NaiveDate::from_ymd(2022, 9, 15),
            FestivalPlus => NaiveDate::from_ymd(2023, 3, 23),
        }
    }
    pub fn latest() -> Self {
        Self::FestivalPlus
    }
}
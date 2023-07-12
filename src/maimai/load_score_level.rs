use std::{io::BufReader, path::PathBuf};

use anyhow::{anyhow, bail};
use fs_err::File;
use serde::Deserialize;

use super::{
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration},
};

pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Vec<Song>> {
    let songs: Vec<SongRaw> = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    songs.into_iter().map(Song::try_from).collect()
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
#[derive(Debug)]
pub struct Song {
    generation: ScoreGeneration,
    levels: ScoreLevels,
    song_name: String,
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
            levels: ScoreLevels {
                basic: song.lv[0].try_into()?,
                advanced: song.lv[1].try_into()?,
                expert: song.lv[2].try_into()?,
                master: song.lv[3].try_into()?,
                re_master,
            },
            song_name: song.n,
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

#[non_exhaustive]
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
// impl TryFrom<u8> for MaimaiVersion {
//     type Error = anyhow::Error;
//     fn try_from(v: u8) -> anyhow::Result<Self> {
//     }
// }

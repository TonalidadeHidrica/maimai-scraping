use std::path::PathBuf;

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::{
    load_score_level,
    rating::{RankCoefficient, ScoreConstant},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongName},
};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    levels_path: PathBuf,
    tsv_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    #[allow(unused)]
    let levels = load_score_level::load(&opts.levels_path)?;
    let table = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(File::open(&opts.tsv_path)?)
        .into_deserialize::<Record>();
    for record in table {
        let record = record?;
        println!("{record:?}");
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
#[allow(unused)]
struct Record {
    #[serde(rename = "曲名")]
    song_name: SongName,
    #[serde(rename = "譜面1", deserialize_with = "de::generation")]
    generation: ScoreGeneration,
    #[serde(rename = "譜面2", deserialize_with = "de::difficulty")]
    difficulty: ScoreDifficulty,
    #[serde(rename = "Rate")]
    single_score_rating: u16,
    #[serde(rename = "係数", deserialize_with = "de::rank_coef")]
    coefficient: RankCoefficient,
    #[serde(rename = "定数", deserialize_with = "de::internal_lv")]
    internal_lv: ScoreConstant,
}

mod de {
    use maimai_scraping::maimai::{
        load_score_level::InternalScoreLevel,
        rating::{RankCoefficient, ScoreConstant},
        schema::latest::{ScoreDifficulty, ScoreGeneration},
    };
    use serde::{de::Error, Deserialize, Deserializer};

    fn de_str<'de, D: Deserializer<'de>>(d: D) -> Result<&'de str, D::Error> {
        Deserialize::deserialize(d)
    }

    fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        Deserialize::deserialize(d)
    }

    pub(super) fn generation<'de, D: Deserializer<'de>>(d: D) -> Result<ScoreGeneration, D::Error> {
        use ScoreGeneration::*;
        match de_str(d)? {
            "STD" => Ok(Standard),
            "DX" => Ok(Deluxe),
            s => Err(D::Error::custom(format!("Unknown generation: {s:?}"))),
        }
    }

    pub(super) fn difficulty<'de, D: Deserializer<'de>>(d: D) -> Result<ScoreDifficulty, D::Error> {
        use ScoreDifficulty::*;
        match de_str(d)? {
            "BAS" => Ok(Basic),
            "ADV" => Ok(Advanced),
            "EXP" => Ok(Expert),
            "MAS" => Ok(Master),
            "ReMAS" => Ok(ReMaster),
            s => Err(D::Error::custom(format!("Unknown generation: {s:?}"))),
        }
    }

    pub(super) fn rank_coef<'de, D: Deserializer<'de>>(d: D) -> Result<RankCoefficient, D::Error> {
        Ok(RankCoefficient((de_f64(d)? * 10.).round() as u64))
    }

    pub(super) fn internal_lv<'de, D: Deserializer<'de>>(d: D) -> Result<ScoreConstant, D::Error> {
        let v = de_f64(d)?;
        match InternalScoreLevel::try_from(v) {
            Ok(InternalScoreLevel::Known(v)) => Ok(v),
            _ => Err(D::Error::custom(format!("Invalid internal lv: {v:?}"))),
        }
    }
}

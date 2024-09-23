use std::path::PathBuf;

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, RankCoefficient, ScoreConstant},
    schema::{
        latest::{ScoreDifficulty, ScoreGeneration, SongName},
        ver_20210316_2338::AchievementValue,
    },
};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    // levels_path: PathBuf,
    tsv_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    // 2024-09-24 commenting out.
    // We want to avoid using `load_score_level` (a.k.a. `song_list::in_lv`) as much as possible.
    // This line is supposed to be replaced with `database` when used.
    // #[allow(unused)]
    // let levels = load_score_level::load(&opts.levels_path)?;
    let table = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(File::open(opts.tsv_path)?)
        .into_deserialize::<Record>();
    for record in table {
        let record = record?;
        let rank_coef = rank_coef(record.achievement);
        let candidates = ScoreConstant::candidates()
            .filter(|&level| {
                single_song_rating(level, record.achievement, rank_coef).get()
                    == record.single_score_rating
            })
            .collect_vec();
        if (rank_coef, &candidates[..]) != (record.coefficient, &[record.internal_lv][..]) {
            println!("Warning: {record:?} => ({rank_coef}, {candidates:?})");
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(unused)]
    #[serde(rename = "曲名")]
    song_name: SongName,
    #[allow(unused)]
    #[serde(rename = "譜面1", deserialize_with = "de::generation")]
    generation: ScoreGeneration,
    #[allow(unused)]
    #[serde(rename = "譜面2", deserialize_with = "de::difficulty")]
    difficulty: ScoreDifficulty,
    #[serde(rename = "達成率", deserialize_with = "de::achievement")]
    achievement: AchievementValue,
    #[serde(rename = "Rate")]
    single_score_rating: u16,
    #[serde(rename = "係数", deserialize_with = "de::rank_coef")]
    coefficient: RankCoefficient,
    #[serde(rename = "定数", deserialize_with = "de::internal_lv")]
    internal_lv: ScoreConstant,
}

mod de {
    use maimai_scraping::maimai::{
        rating::{RankCoefficient, ScoreConstant},
        schema::{
            latest::{ScoreDifficulty, ScoreGeneration},
            ver_20210316_2338::AchievementValue,
        },
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

    pub(super) fn achievement<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<AchievementValue, D::Error> {
        AchievementValue::try_from((de_f64(d)? * 10_000.).round() as u32).map_err(D::Error::custom)
    }

    pub(super) fn rank_coef<'de, D: Deserializer<'de>>(d: D) -> Result<RankCoefficient, D::Error> {
        Ok(RankCoefficient((de_f64(d)? * 10.).round() as u64))
    }

    pub(super) fn internal_lv<'de, D: Deserializer<'de>>(d: D) -> Result<ScoreConstant, D::Error> {
        let value = de_f64(d)?;
        // TODO duplicate code in `in_lv::InternalScoreLevel`?
        ((value.abs() * 10.).round() as u8)
            .try_into()
            .map_err(|_| D::Error::custom(format!("Invalid internal lv: {value:?}")))
    }
}

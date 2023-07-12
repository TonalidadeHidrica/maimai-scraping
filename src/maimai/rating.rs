#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::zero_prefixed_literal)]
use std::{fmt::Display, str::FromStr};

use anyhow::bail;
use serde::{Deserialize, Serialize};

use super::schema::latest::{AchievementValue, RatingValue};

#[derive(Clone, Copy, PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct RankCoefficient(u64);

impl Display for RankCoefficient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = self.0 / 10;
        let y = self.0 % 10;
        write!(f, "{}.{:01}", x, y)
    }
}

// https://gamerch.com/maimai/entry/533647#content_2_1
// https://sgimera.github.io/mai_RatingAnalyzer/maidx_rating.html
// Retrieved 2023/07/10 20:02
pub fn rank_coef(achievement_value: AchievementValue) -> RankCoefficient {
    #[allow(clippy::mistyped_literal_suffixes)]
    let ret = match achievement_value.get() {
        100_5000.. => 22_4,
        100_4999.. => 22_2,
        100_0000.. => 21_6,
        99_9999.. => 21_4,
        99_5000.. => 21_1,
        99_0000.. => 20_8,
        98_0000.. => 20_3,
        97_0000.. => 20_0,
        96_9999.. => 17_6,
        94_0000.. => 16_8,
        90_0000.. => 15_2,
        80_0000.. => 13_6,
        75_0000.. => 12_0,
        70_0000.. => 11_2,
        60_0000.. => 9_6,
        50_0000.. => 8_0,
        40_0000.. => 6_4,
        30_0000.. => 4_8,
        20_0000.. => 3_2,
        10_0000.. => 1_6,
        0_0000.. => 0_0,
    };
    ret.into()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, derive_more::Into, Serialize, Deserialize)]
pub struct ScoreConstant(u8);

impl TryFrom<u8> for ScoreConstant {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            1_0..=15_0 => Ok(Self(v)),
            _ => Err(v),
        }
    }
}

impl Display for ScoreConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = self.0 / 10;
        let y = self.0 % 10;
        write!(f, "{}.{:01}", x, y)
    }
}

impl ScoreConstant {
    pub fn candidates() -> impl Iterator<Item = Self> {
        (1_0..=15_0).map(Self)
    }
}

pub fn single_song_rating(
    score_const: ScoreConstant,
    achievement_value: AchievementValue,
    rank_coef: RankCoefficient,
) -> RatingValue {
    let achievement_value_clamped = achievement_value.get().min(100_5000);
    let prod = score_const.0 as u64 * achievement_value_clamped as u64 * rank_coef.0;
    RatingValue::from((prod / 10 / 100_0000 / 10) as u16)
}

#[allow(unused)]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ScoreLevel {
    level: u8,
    plus: bool,
}

impl ScoreLevel {
    pub fn new(level: u8, plus: bool) -> anyhow::Result<Self> {
        match (level, plus) {
            (16.., _) | (15, true) => {
                bail!("Level out of range: {level}{}", if plus { "+" } else { "" })
            }
            _ => Ok(ScoreLevel { level, plus }),
        }
    }
}

impl FromStr for ScoreLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let stripped = s.strip_suffix('+');
        let level = stripped.unwrap_or(s).parse()?;
        let plus = stripped.is_some();
        Self::new(level, plus)
    }
}

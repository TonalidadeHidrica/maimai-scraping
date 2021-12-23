#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::zero_prefixed_literal)]
use std::fmt::Display;

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

// https://maimai.gamerch.com/%E3%81%A7%E3%82%89%E3%81%A3%E3%81%8F%E3%81%99RATING#content_2_1
// Retrieved 2021/11/20 1:58
pub fn rank_coef_gamerch_old(achievement_value: AchievementValue) -> RankCoefficient {
    let ret = match achievement_value.get() / 100 {
        100_50..=101_00 => 15_0,
        100_00..=101_00 => 14_0,
        99_99..=101_00 => 13_5,
        99_50..=101_00 => 13_0,
        99_00..=101_00 => 12_0,
        98_00..=101_00 => 11_0,
        97_00..=101_00 => 10_0,
        94_00..=101_00 => 9_4,
        90_00..=101_00 => 9_0,
        80_00..=101_00 => 8_0,
        75_00..=101_00 => 7_5,
        70_00..=101_00 => 7_0,
        60_00..=101_00 => 6_0,
        50_00..=101_00 => 5_0,
        40_00..=101_00 => 4_0,
        30_00..=101_00 => 3_0,
        20_00..=101_00 => 2_0,
        10_00..=101_00 => 1_0,
        0_00..=101_00 => 0_0,
        _ => unreachable!("The range of value is guarded"),
    };
    ret.into()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ScoreConstant(u8);

impl TryFrom<u8> for ScoreConstant {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        match v {
            0_1..=15_0 => Ok(Self(v)),
            _ => Err(v),
        }
    }
}

impl Display for ScoreConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let x = self.0 / 10;
        let y = self.0 % 10;
        write!(f, "{}.{:01}%", x, y)
    }
}

impl ScoreConstant {
    pub fn candidates() -> impl Iterator<Item = Self> {
        (0_1..=15_0).map(Self)
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

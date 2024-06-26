#![allow(clippy::inconsistent_digit_grouping)]
#![allow(clippy::zero_prefixed_literal)]
use std::{fmt::Display, iter::successors, str::FromStr};

use anyhow::bail;
use serde::{Deserialize, Serialize};

use super::schema::latest::{AchievementValue, RatingValue};

#[derive(Clone, Copy, PartialEq, Eq, Debug, derive_more::From, Serialize, Deserialize)]
pub struct RankCoefficient(pub u64);

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

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, derive_more::Into, Serialize, Deserialize,
)]
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
    pub fn candidates() -> impl DoubleEndedIterator<Item = Self> {
        (1_0..=15_0).map(Self)
    }
}

pub fn single_song_rating_precise(
    score_const: ScoreConstant,
    achievement_value: AchievementValue,
    rank_coef: RankCoefficient,
) -> u64 {
    let achievement_value_clamped = achievement_value.get().min(100_5000);
    score_const.0 as u64 * achievement_value_clamped as u64 * rank_coef.0
}

pub fn single_song_rating(
    score_const: ScoreConstant,
    achievement_value: AchievementValue,
    rank_coef: RankCoefficient,
) -> RatingValue {
    let prod = single_song_rating_precise(score_const, achievement_value, rank_coef);
    RatingValue::from((prod / 10 / 100_0000 / 10) as u16)
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct ScoreLevel {
    pub level: u8,
    pub plus: bool,
}

impl ScoreLevel {
    pub fn new(level: u8, plus: bool) -> anyhow::Result<Self> {
        match (level, plus) {
            (0 | 16.., _) | (1..=6 | 15, true) => {
                bail!("Level out of range: {level}{}", if plus { "+" } else { "" })
            }
            _ => Ok(ScoreLevel { level, plus }),
        }
    }
    pub fn score_constant_candidates(self) -> impl Iterator<Item = ScoreConstant> + Clone {
        // TODO: This function should be deprecated and every usage must be aware of its version.
        self.score_constant_candidates_aware(true)
    }

    pub fn score_constant_candidates_aware(
        self,
        buddies_plus_or_later: bool,
    ) -> impl Iterator<Item = ScoreConstant> + Clone {
        let range = match self.level {
            a @ 1..=6 => a * 10..(a + 1) * 10,
            a @ 7..=14 => {
                let boundary = a * 10 + if buddies_plus_or_later { 6 } else { 7 };
                if self.plus {
                    boundary..(a + 1) * 10
                } else {
                    a * 10..boundary
                }
            }
            15 => 150..151,
            _ => unreachable!(),
        };
        range.map(ScoreConstant)
    }

    /// x..=y
    pub fn range_inclusive(x: ScoreLevel, y: ScoreLevel) -> impl Iterator<Item = ScoreLevel> {
        successors(Some(x), |x| {
            let (level, plus) = match (x.level, x.plus) {
                (15, _) => return None,
                (x @ 7..=14, false) => (x, true),
                (x, _) => (x + 1, false),
            };
            Some(ScoreLevel::new(level, plus).unwrap())
        })
        .filter(move |&x| x <= y)
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

impl Display for ScoreLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.level, if self.plus { "+" } else { "" })
    }
}

impl From<ScoreConstant> for ScoreLevel {
    fn from(value: ScoreConstant) -> Self {
        let value = value.0;
        let level = value / 10;
        Self {
            level,
            plus: (7..=14).contains(&level) && value % 10 >= 6,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use itertools::Itertools;

    use super::ScoreLevel;

    #[test]
    fn test_song_level_range_inclusive() {
        let levels =
            ScoreLevel::range_inclusive("1".parse().unwrap(), "15".parse().unwrap()).collect_vec();
        // 1-15 + 7-14 = 15 + 8 = 23
        assert!(levels.len() == 23);
        // All element are different, elements are sorted
        let set = BTreeSet::from_iter(levels.iter().copied())
            .into_iter()
            .collect_vec();
        assert!(levels == set);
    }
}

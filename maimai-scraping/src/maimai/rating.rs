use std::{
    fmt::{Debug, Display, Write},
    iter::successors,
    str::FromStr,
};

use anyhow::bail;
use derive_more::{From, Into};
use getset::CopyGetters;
use itertools::{chain, iterate, Itertools};
use serde::{Deserialize, Serialize};
use smol_str::SmolStrBuilder;

use super::{
    schema::latest::{AchievementValue, RatingValue},
    version::MaimaiVersion,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, From, Serialize, Deserialize)]
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
    #[allow(clippy::inconsistent_digit_grouping)]
    #[allow(clippy::zero_prefixed_literal)]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Into, Serialize, Deserialize)]
pub struct ScoreConstant(u8);

impl TryFrom<u8> for ScoreConstant {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        #[allow(clippy::inconsistent_digit_grouping)]
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
        let mut buffer = SmolStrBuilder::new();
        write!(buffer, "{}.{:01}", x, y)?;
        f.pad(buffer.finish().as_str())
    }
}

impl ScoreConstant {
    pub fn candidates() -> impl DoubleEndedIterator<Item = Self> {
        #[allow(clippy::inconsistent_digit_grouping)]
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
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

    pub fn all() -> impl Iterator<Item = Self> {
        Self::range_inclusive("1".parse().unwrap(), "15".parse().unwrap())
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
        let mut buffer = SmolStrBuilder::new();
        write!(buffer, "{}{}", self.level, if self.plus { "+" } else { "" })?;
        f.pad(buffer.finish().as_str())
    }
}

// TODO deprecate this method
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
impl ScoreConstant {
    pub fn to_lv(self, version: MaimaiVersion) -> ScoreLevel {
        let boundary = if version >= MaimaiVersion::BuddiesPlus {
            6
        } else {
            7
        };
        let value = self.0;
        let level = value / 10;
        ScoreLevel {
            level,
            plus: (7..=14).contains(&level) && value % 10 >= boundary,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, CopyGetters, Serialize, Deserialize)]
#[getset(get_copy = "pub")]
pub struct InternalScoreLevel {
    offset: ScoreConstant,
    mask: CandidateBitmask,
}
impl Display for InternalScoreLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buffer = SmolStrBuilder::new();
        if self.mask.empty() {
            write!(buffer, "empty")?;
        } else {
            let x = u8::from(self.offset);
            write!(buffer, "{}.", x / 10)?;
            let f = self.mask.bits().map(|i| i + x % 10);
            let f = chain!([128], f, [128]);
            for (w, x, y) in f.tuple_windows() {
                let (wx, xy) = (x.wrapping_sub(w), y.wrapping_sub(x));
                if wx > 1 {
                    if w != 128 {
                        buffer.push(',');
                    }
                    write!(buffer, "{x}")?;
                    if xy == 1 {
                        buffer.push('-');
                    }
                } else if xy > 1 {
                    write!(buffer, "{x}")?;
                }
            }
        }
        f.pad(buffer.finish().as_str())
    }
}
impl InternalScoreLevel {
    pub fn empty() -> Self {
        Self {
            offset: ScoreConstant(10),
            mask: CandidateBitmask(0),
        }
    }
    pub fn new(
        version: MaimaiVersion,
        level: ScoreLevel,
        mask: CandidateBitmask,
    ) -> anyhow::Result<Self> {
        let mut ret = Self::unknown(version, level);
        if mask.0.trailing_zeros() < ret.mask.0.trailing_zeros() {
            bail!("The given {mask:?} exceeds the range of score level {level}");
        }
        ret.mask.0 &= mask.0;
        Ok(ret)
    }

    pub fn known(value: ScoreConstant) -> Self {
        Self {
            offset: value,
            mask: CandidateBitmask(1),
        }
    }
    pub fn unknown(version: MaimaiVersion, level: ScoreLevel) -> Self {
        let (offset, count) = offset_and_count(version, level);
        Self {
            offset: ScoreConstant::try_from(offset).unwrap(),
            mask: CandidateBitmask((1 << count) - 1),
        }
    }

    pub fn get_if_unique(self) -> Option<ScoreConstant> {
        self.is_unique().then(|| self.candidates().next().unwrap())
    }

    pub fn is_unique(self) -> bool {
        self.mask.count_bits() == 1
    }

    pub fn is_empty(&self) -> bool {
        self.mask.count_bits() == 0
    }

    pub fn into_level(self, version: MaimaiVersion) -> ScoreLevel {
        self.offset.to_lv(version)
    }

    pub fn candidates(self) -> impl Iterator<Item = ScoreConstant> + Clone {
        self.mask
            .bits()
            .map(move |x| ScoreConstant(self.offset.0 + x))
    }

    pub fn count_candidates(self) -> usize {
        self.mask.count_bits() as _
    }

    pub fn retain(&mut self, mut f: impl FnMut(ScoreConstant) -> bool) {
        for (i, lv) in self.mask.bits().zip(self.candidates()) {
            if !f(lv) {
                self.mask.0 &= !(1 << i);
            }
        }
    }

    pub fn intersection(self, other: Self) -> Self {
        if self.mask.empty() || other.mask.empty() {
            Self::empty()
        } else {
            let [x, y] = [self, other].map(Self::canonicaliize);
            let merge = |x: Self, y: Self| {
                //        v x.offset
                // x 110101
                // y    10101
                //          ^ y.offset
                //         ^^  diff
                //      ^^^^^ y_span
                //      ^^^  y_span - diff
                // Only the least significant (y_span - diff) bits of `x.mask` is valid
                let _: u16 = x.mask.0; // make sure that mask is 16 bit
                let diff = x.offset.0 - y.offset.0;
                let y_span = (16 - y.mask.0.leading_zeros()) as u8;
                let x_mask = (1 << (y_span.saturating_sub(diff))) - 1;
                let x_masked = x.mask.0 & x_mask;
                if x_masked == 0 {
                    Self::empty()
                } else {
                    Self {
                        offset: y.offset,
                        mask: CandidateBitmask((x_masked << diff) & y.mask.0),
                    }
                    .canonicaliize()
                }
            };
            if x.offset.0 > y.offset.0 {
                merge(x, y)
            } else {
                merge(y, x)
            }
        }
    }

    pub fn canonicaliize(self) -> Self {
        if self.mask.empty() {
            Self::empty()
        } else {
            let offset = self.mask.0.trailing_zeros();
            Self {
                offset: ScoreConstant(self.offset.0 + offset as u8),
                mask: CandidateBitmask(self.mask.0 >> offset),
            }
        }
    }

    pub fn in_lv_mask(self, version: MaimaiVersion) -> CandidateBitmask {
        let level = self.into_level(version);
        let (offset, _) = offset_and_count(version, level);
        CandidateBitmask(self.mask.0 << (self.offset.0 - offset))
    }

    pub fn contains(self, level: ScoreConstant) -> bool {
        (level.0)
            .checked_sub(self.offset.0)
            .is_some_and(|diff| self.mask.has(diff))
    }
}

fn offset_and_count(version: MaimaiVersion, level: ScoreLevel) -> (u8, u8) {
    match level.level {
        a @ 1..=6 => (a * 10, 10),
        a @ 7..=14 => {
            let boundary = if version >= MaimaiVersion::BuddiesPlus {
                6
            } else {
                7
            };
            if level.plus {
                (a * 10 + boundary, 10 - boundary)
            } else {
                (a * 10, boundary)
            }
        }
        15 => (150, 1),
        _ => unreachable!(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateBitmask(u16);
impl CandidateBitmask {
    pub fn get(self) -> u16 {
        self.0
    }
}
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
    pub fn empty(self) -> bool {
        self.0 == 0
    }
    pub fn has(self, x: u8) -> bool {
        (x as u32) < u8::BITS && ((1 << x) & self.0) > 0
    }
    pub fn bits(self) -> impl Iterator<Item = u8> + Clone {
        iterate(self.0, |x| x >> 1)
            .enumerate()
            .take_while(|&(_, x)| x > 0)
            .filter_map(|(i, x)| ((x & 1) > 0).then_some(i as u8))
    }
    pub fn count_bits(self) -> usize {
        self.0.count_ones() as _
    }
}
impl Debug for CandidateBitmask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CandidateBitmask")
            .field(&format_args!("{:#b}", self.0))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use itertools::Itertools;
    use rand::{thread_rng, Rng};

    use crate::maimai::version::MaimaiVersion;

    use super::{CandidateBitmask, InternalScoreLevel, ScoreConstant, ScoreLevel};

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

    #[test]
    #[allow(clippy::unusual_byte_groupings)]
    fn test_intersection() {
        let x = InternalScoreLevel {
            offset: ScoreConstant(130),
            mask: CandidateBitmask(0b_110_100),
        };
        let y = InternalScoreLevel {
            offset: ScoreConstant(130),
            mask: CandidateBitmask(0b_101_110),
        };
        let z = InternalScoreLevel {
            offset: ScoreConstant(132),
            mask: CandidateBitmask(0b_100_1),
        };
        assert_eq!(x.intersection(y), z);

        let x = InternalScoreLevel {
            offset: ScoreConstant(50),
            mask: CandidateBitmask(0b_110_100),
        };
        let y = InternalScoreLevel {
            offset: ScoreConstant(130),
            mask: CandidateBitmask(0b_101_110),
        };
        let z = InternalScoreLevel::empty();
        assert_eq!(x.intersection(y), z);
    }

    #[test]
    fn test_intersection_stress() {
        let mut rng = thread_rng();
        let mut gen = |bias| {
            let offset = rng.gen_range(10..=150);
            let max = (150 - offset + 1).min(bias);
            let mask = rng.gen_range(0..(1 << max));
            InternalScoreLevel {
                offset: ScoreConstant(offset),
                mask: CandidateBitmask(mask),
            }
        };
        let mut run = |x, y| {
            // Clippy false positive!!!!!
            #[allow(clippy::redundant_closure)]
            let [x, y] = [x, y].map(|x| gen(x));
            let [xs, ys] = [x, y].map(|x| x.candidates().collect::<BTreeSet<_>>());
            let mut expected = xs.intersection(&ys).peekable();
            let z = match expected.peek() {
                None => InternalScoreLevel::empty(),
                Some(&&offset) => {
                    let mut mask = 0;
                    for lv in expected {
                        mask |= 1u16.checked_shl((lv.0 - offset.0) as u32).unwrap();
                    }
                    InternalScoreLevel {
                        offset,
                        mask: CandidateBitmask(mask),
                    }
                }
            };
            assert_eq!(x.intersection(y), z, "While merging {x:?} and {y:?}");
        };
        for _ in 0..100_000 {
            for x in [1, 5, 10] {
                for y in [1, 5, 10] {
                    run(x, y);
                }
            }
        }
    }

    #[test]
    #[allow(clippy::unusual_byte_groupings)]
    pub fn test_in_lv_mask() {
        let x = InternalScoreLevel {
            offset: ScoreConstant(130),
            mask: CandidateBitmask(0b_110_100),
        };
        assert_eq!(
            x.in_lv_mask(MaimaiVersion::Buddies),
            CandidateBitmask(0b_110_100)
        );

        let x = InternalScoreLevel {
            offset: ScoreConstant(56),
            mask: CandidateBitmask(0b_1011),
        };
        assert_eq!(
            x.in_lv_mask(MaimaiVersion::Buddies),
            CandidateBitmask(0b_1011_000_000)
        );
    }

    #[test]
    #[allow(clippy::unusual_byte_groupings)]
    pub fn test_in_lv_display() {
        let x = InternalScoreLevel {
            offset: ScoreConstant(130),
            mask: CandidateBitmask(0b_110_100),
        };
        assert_eq!(x.to_string(), "13.2,4-5");

        let x = InternalScoreLevel {
            offset: ScoreConstant(60),
            mask: CandidateBitmask(0b_111_111),
        };
        assert_eq!(x.to_string(), "6.0-5");

        let x = InternalScoreLevel {
            offset: ScoreConstant(60),
            mask: CandidateBitmask(0b_100_000),
        };
        assert_eq!(x.to_string(), "6.5");

        let x = InternalScoreLevel {
            offset: ScoreConstant(60),
            mask: CandidateBitmask(0b_010_101),
        };
        assert_eq!(x.to_string(), "6.0,2,4");
    }
}

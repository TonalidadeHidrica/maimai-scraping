use anyhow::bail;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use enum_iterator::Sequence;
use enum_map::Enum;
use serde::{Deserialize, Serialize};
use strum::EnumString;

#[non_exhaustive]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Debug,
    EnumString,
    Serialize,
    Deserialize,
    Sequence,
    Enum,
)]
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
    BuddiesPlus,
    Prism,
    PrismPlus,
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
            22 => BuddiesPlus,
            23 => Prism,
            24 => PrismPlus,
            _ => bail!("Unexpected version: {v}"),
        })
    }
}
impl From<MaimaiVersion> for i8 {
    fn from(v: MaimaiVersion) -> i8 {
        use MaimaiVersion::*;
        match v {
            Maimai => 0,
            MaimaiPlus => 1,
            Green => 2,
            GreenPlus => 3,
            Orange => 4,
            OrangePlus => 5,
            Pink => 6,
            PinkPlus => 7,
            Murasaki => 8,
            MurasakiPlus => 9,
            Milk => 10,
            MilkPlus => 11,
            Finale => 12,
            Deluxe => 13,
            DeluxePlus => 14,
            Splash => 15,
            SplashPlus => 16,
            Universe => 17,
            UniversePlus => 18,
            Festival => 19,
            FestivalPlus => 20,
            Buddies => 21,
            BuddiesPlus => 22,
            Prism => 23,
            PrismPlus => 24,
        }
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
            BuddiesPlus => NaiveDate::from_ymd_opt(2024, 3, 21).unwrap(),
            Prism => NaiveDate::from_ymd_opt(2024, 9, 12).unwrap(),
            PrismPlus => NaiveDate::from_ymd_opt(2024, 3, 13).unwrap(),
        }
    }
    pub fn start_time(self) -> NaiveDateTime {
        self.start_date()
            .and_time(NaiveTime::from_hms_opt(6, 0, 0).unwrap())
    }
    pub fn end_time(self) -> NaiveDateTime {
        match self.next() {
            Some(next) => next
                .start_date()
                .and_time(NaiveTime::from_hms_opt(6, 0, 0).unwrap()),
            None => NaiveDateTime::MAX,
        }
    }
    pub fn of_time(time: NaiveDateTime) -> Option<MaimaiVersion> {
        enum_iterator::all()
            .find(|v: &MaimaiVersion| (v.start_time()..v.end_time()).contains(&time))
    }
    pub fn of_date(x: NaiveDate) -> Option<MaimaiVersion> {
        enum_iterator::all().find(|v: &MaimaiVersion| {
            v.start_date() <= x && v.next().is_none_or(|v| x < v.start_date())
        })
    }
    pub fn latest() -> Self {
        Self::Prism
    }
}

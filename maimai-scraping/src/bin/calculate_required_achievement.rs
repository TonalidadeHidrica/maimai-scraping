use std::fmt::Display;

use anyhow::anyhow;
use inquire::{CustomType, InquireError};
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::AchievementValue,
};

macro_rules! check {
    ($e: expr) => {{
        let e = $e;
        if let Err(InquireError::OperationInterrupted) = e {
            return Ok(());
        }
        e
    }};
}
fn main() -> anyhow::Result<()> {
    let level: u8 = CustomType::new("Internal Lv.").prompt()?;
    let level = ScoreConstant::try_from(level).map_err(|v| anyhow!("Invalid level: {v}"))?;
    let rating: u16 = check!(CustomType::new("Single song rating").prompt())?;
    let mut start = None;
    for a in 0..101_0001 {
        let ok = match AchievementValue::try_from(a) {
            Ok(a) => single_song_rating(level, a, rank_coef(a)).get() == rating,
            _ => false,
        };
        if ok {
            start.get_or_insert(a);
        } else if let Some(start) = start.take() {
            println!("[{}, {})", show_achievement(start), show_achievement(a));
        }
    }
    Ok(())
}

fn show_achievement(a: u32) -> impl Display {
    lazy_format!("{}.{:04}", a / 10000, a % 10000)
}

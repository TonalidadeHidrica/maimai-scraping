use anyhow::anyhow;
use inquire::{CustomType, InquireError};
use itertools::Itertools;
use joinery::JoinableIterator;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::ver_20230914_1328::AchievementValue,
};

fn main() {
    loop {
        match run() {
            Ok(false) => break,
            Ok(true) => continue,
            Err(e) => eprintln!("{e}"),
        }
    }
}

fn run() -> anyhow::Result<bool> {
    macro_rules! check {
        ($e: expr) => {{
            let e = $e;
            if let Err(InquireError::OperationInterrupted) = e {
                return Ok(false);
            }
            e
        }};
    }

    let achievement: u32 = check!(CustomType::new("Achievement").prompt())?;
    let achievement =
        AchievementValue::try_from(achievement).map_err(|v| anyhow!("Invalid achievement: {v}"))?;
    let rating: u16 = check!(CustomType::new("Single song rating").prompt())?;

    let res = ScoreConstant::candidates()
        .filter(|&level| {
            single_song_rating(level, achievement, rank_coef(achievement)).get() == rating
        })
        .collect_vec();
    println!("Candidates: {}", res.iter().join_with(", "));
    Ok(true)
}

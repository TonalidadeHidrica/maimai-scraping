use anyhow::anyhow;
use inquire::CustomType;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating_precise, ScoreConstant},
    schema::latest::AchievementValue,
};

fn main() -> anyhow::Result<()> {
    run()?;
    Ok(())
}

fn run() -> anyhow::Result<()> {
    let achievement: u32 = CustomType::new("Achievement").prompt()?;
    let achievement =
        AchievementValue::try_from(achievement).map_err(|v| anyhow!("Invalid achievement: {v}"))?;
    let level: u8 = CustomType::new("Internal Lv.").prompt()?;
    let level = ScoreConstant::try_from(level).map_err(|v| anyhow!("Invalid level: {v}"))?;
    let res = single_song_rating_precise(level, achievement, rank_coef(achievement));
    let factor = 10 * 100_0000 * 10;
    println!("{}.{:08}", res / factor, res % factor);
    Ok(())
}

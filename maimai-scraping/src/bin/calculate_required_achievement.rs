use std::fmt::Display;

use anyhow::anyhow;
use clap::Parser;
use inquire::{CustomType, InquireError};
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::AchievementValue,
};

#[derive(Parser)]
struct Opts {
    #[arg(long)]
    all: bool,
}

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
    let opts = Opts::parse();
    let rating: u16 = check!(CustomType::new("Single song rating").prompt())?;
    if opts.all {
        for level in ScoreConstant::candidates() {
            let res = calculate(level, rating);
            if !res.is_empty() {
                print!("{level}");
                for res in res {
                    print!(" {}", show_segment(res));
                }
                println!();
            }
        }
    } else {
        let level: u8 = CustomType::new("Internal Lv.").prompt()?;
        let level = ScoreConstant::try_from(level).map_err(|v| anyhow!("Invalid level: {v}"))?;
        for res in calculate(level, rating) {
            println!("{}", show_segment(res));
        }
    }
    Ok(())
}

fn calculate(level: ScoreConstant, rating: u16) -> Vec<[u32; 2]> {
    let mut start = None;
    let mut res = vec![];
    for a in 0..101_0001 {
        let ok = match AchievementValue::try_from(a) {
            Ok(a) => single_song_rating(level, a, rank_coef(a)).get() == rating,
            _ => false,
        };
        if ok {
            start.get_or_insert(a);
        } else if let Some(start) = start.take() {
            res.push([start, a]);
        }
    }
    res
}

fn show_achievement(a: u32) -> impl Display {
    lazy_format!("{}.{:04}", a / 10000, a % 10000)
}
fn show_segment([x, y]: [u32; 2]) -> impl Display {
    lazy_format!("[{}, {})", show_achievement(x), show_achievement(y))
}

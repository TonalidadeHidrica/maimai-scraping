use std::fmt::{Display, Write};

use anyhow::{anyhow, bail, Context, Result};
use chrono::NaiveDateTime;
use itertools::Itertools;
use maimai_scraping_utils::{regex, selector};
use scraper::{ElementRef, Html};
use serde::{Deserialize, Serialize};
use smol_str::SmolStrBuilder;

use crate::maimai::{parser::play_record::parse_achievement_as_num, schema::latest::UserName};

pub fn parse(html: &Html) -> anyhow::Result<Ranking> {
    let as_of = parse_as_of(
        html.select(selector!("div.ranking_title_block > span"))
            .next()
            .context("Ranking update time span not found")?,
    )?;
    let entries = html
        .select(selector!("div.ranking_top_block, div.ranking_block"))
        .map(parse_entry)
        .try_collect::<_, Vec<_>, _>()?;
    for (expected_rank, entry) in (1..).zip(&entries) {
        if expected_rank != entry.rank {
            bail!("Rank mismatch at {entry:?}");
        }
    }
    Ok(Ranking { as_of, entries })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Ranking {
    pub as_of: NaiveDateTime,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub rank: u32,
    pub player_name: UserName,
    pub play_time: NaiveDateTime,
    pub achievement: AchievementSum,
    pub deluxscore: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct AchievementSum(u32);
impl TryFrom<u32> for AchievementSum {
    type Error = anyhow::Error;
    fn try_from(v: u32) -> Result<Self> {
        match v {
            0..=303_0000 => Ok(Self(v)),
            _ => bail!("Achievement value out of range: {v}"),
        }
    }
}
impl Display for AchievementSum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buffer = SmolStrBuilder::new();
        let x = self.0 / 10000;
        let y = self.0 % 10000;
        write!(buffer, "{}.{:04}%", x, y)?;
        f.pad(buffer.finish().as_str())
    }
}
impl AchievementSum {
    pub fn get(&self) -> u32 {
        self.0
    }
}

fn parse_as_of(span: scraper::ElementRef) -> Result<NaiveDateTime> {
    Ok(parse_date_time(
        &regex!(r"^(\d\d\d\d/\d\d/\d\d \d\d:\d\d) 更新$")
            .captures(&span.text().collect::<String>())
            .with_context(|| format!("Unexpected span: {}", span.html()))?[1],
    )?)
}

fn parse_date_time(text: &str) -> chrono::ParseResult<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y/%m/%d %H:%M")
}

fn parse_entry(div: ElementRef) -> Result<Entry> {
    let rank = div
        .select(selector!("div.ranking_rank_block img"))
        .map(parse_rank_digit)
        .collect_vec();
    let rank = rank
        .into_iter()
        .rev()
        .fold(anyhow::Ok(0), |x, y| Ok(x? * 10 + y?))?;

    let player_name = div
        .select(selector!("div.f_l.f_15"))
        .next()
        .context("Player name div not fonud")?
        .text()
        .collect::<String>()
        .trim_ascii()
        .to_owned()
        .into();

    let play_time_div = div
        .select(selector!("div.ranking_music_date_1day, div.ranking_music_date_7day, div.ranking_music_date"))
        .next()
        .context("Play time div not found")?;
    let play_time = parse_date_time(&play_time_div.text().collect::<String>())?;

    let achievement_div = play_time_div
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .next()
        .context("Score div not found")?;
    let achievement = parse_achievement_as_num(
        achievement_div
            .text()
            .next()
            .context("No text node for achievement found")?,
    )?
    .try_into()
    .map_err(|e| anyhow!("Achievement out of range: {e}"))?;
    let deluxscore = achievement_div
        .select(selector!("span"))
        .next()
        .context("Deluxscore span not found")?
        .text()
        .collect::<String>()
        .replace(',', "")
        .parse()?;

    Ok(Entry {
        rank,
        player_name,
        play_time,
        achievement,
        deluxscore,
    })
}

fn parse_rank_digit(img: ElementRef) -> Result<u32> {
    let src = img
        .attr("src")
        .with_context(|| format!("src not found in {}", img.html()))?;
    let captures =
        regex!(r"^https://maimaidx.jp/maimai-mobile/img/ranking/([a-z0-9_]+)\.png(\?ver=1.\d\d)?$")
            .captures(src)
            .with_context(|| format!("Invalid rank-digit img src (bad format): {src:?}"))?;
    Ok(match &captures[1] {
        "rank_first" => 1,
        "rank_second" => 2,
        "rank_third" => 3,
        "rank_num_0" => 0,
        "rank_num_1" => 1,
        "rank_num_2" => 2,
        "rank_num_3" => 3,
        "rank_num_4" => 4,
        "rank_num_5" => 5,
        "rank_num_6" => 6,
        "rank_num_7" => 7,
        "rank_num_8" => 8,
        "rank_num_9" => 9,
        _ => bail!("Invalid rank-digit img src (bad name): {src:?}"),
    })
}

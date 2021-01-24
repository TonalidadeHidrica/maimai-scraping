use crate::schema::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::convert::TryInto;

macro_rules! selector {
    ($e: expr) => {{
        static SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse($e).unwrap());
        &*SELECTOR
    }};
}

macro_rules! regex {
    ($e: expr) => {{
        static PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new($e).unwrap());
        &*PATTERN
    }};
}

pub fn parse(html: Html) -> anyhow::Result<PlayRecord> {
    let playlog_top_container = html
        .select(selector!(".playlog_top_container"))
        .next()
        .ok_or_else(|| anyhow!("Playlog top container was not found."))?;
    parse_playlog_top_conatiner(playlog_top_container)?;

    unimplemented!()
}

fn parse_playlog_top_conatiner(div: ElementRef) -> anyhow::Result<(ScoreDifficulty, TrackIndex, NaiveDateTime)> {
    let difficulty = parse_playlog_diff(
        div.select(selector!("img.playlog_diff"))
            .next()
            .ok_or_else(|| anyhow!("Difficulty image was not found."))?,
    )?;

    let mut spans = div.select(selector!("div.sub_title > span"));

    let track_index = parse_track_index(
        spans
            .next()
            .ok_or_else(|| anyhow!("Track index span was not found."))?,
    )?;
    let play_date = parse_play_date(
        spans
            .next()
            .ok_or_else(|| anyhow!("Play date span was not found."))?,
    )?;
    Ok((difficulty, track_index, play_date))
}

fn parse_playlog_diff(img: ElementRef) -> anyhow::Result<ScoreDifficulty> {
    use ScoreDifficulty::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/diff_basic.png") => Ok(Basic),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_advanced.png") => Ok(Advanced),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_expert.png") => Ok(Expert),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_master.png") => Ok(Master),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_remaster.png") => Ok(ReMaster),
        url => Err(anyhow!("Unexpected difficulty image: {:?}", url)),
    }
}

fn parse_track_index(span: ElementRef) -> anyhow::Result<TrackIndex> {
    Ok(regex!(r"TRACK 0([1-4])")
        .captures(&span.text().collect::<String>())
        .ok_or_else(|| anyhow!("The format of track index was invalid."))?
        .get(1)
        .expect("There is a group in the pattern")
        .as_str()
        .parse::<u8>()
        .expect("The captured pattern is an integer")
        .try_into()
        .expect("The value is within the range of 1-4"))
}

fn parse_play_date(span: ElementRef) -> anyhow::Result<NaiveDateTime> {
    Ok(NaiveDateTime::parse_from_str(
        &span.text().collect::<String>(),
        "%Y/%m/%d %H:%M",
    )?)
}

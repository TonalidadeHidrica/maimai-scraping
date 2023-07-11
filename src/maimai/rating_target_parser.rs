use anyhow::Context;
use itertools::{Itertools, PeekingNext};
use scraper::ElementRef;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    play_record_parser::{parse_playlog_diff, parse_score_generation_img},
    song_score_parser::{
        find_and_parse_achievement_value, find_and_parse_score_level, find_and_parse_song_name,
    },
};

use super::{
    rating::ScoreLevel,
    schema::latest::{AchievementValue, ScoreMetadata},
    song_score_parser::{find_and_parse_score_idx, ScoreIdx},
};

pub fn parse(html: &scraper::Html) -> anyhow::Result<RatingTargetList> {
    let mut divs = html
        .select(selector!("div.see_through_block"))
        .next()
        .context("div.see_through_block not found")?
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .peekable();
    Ok(RatingTargetList {
        target_new: parse_entries(&mut divs)?,
        target_old: parse_entries(&mut divs)?,
        candidates_new: parse_entries(&mut divs)?,
        candidates_old: parse_entries(&mut divs)?,
    })
}

pub fn parse_entries<'a, I: PeekingNext<Item = ElementRef<'a>>>(
    mut elems: I,
) -> anyhow::Result<Vec<RatingTargetEntry>> {
    let next = elems.next().context("No next element")?;
    assert!(selector!("div.screw_block").matches(&next));
    elems
        .peeking_take_while(|e| selector!("div.pointer").matches(e))
        .map(parse_entry)
        .collect()
}

pub fn parse_entry(div: ElementRef) -> anyhow::Result<RatingTargetEntry> {
    let difficulty = parse_playlog_diff(
        div.select(selector!("img.h_20"))
            .next()
            .context("Difficulty img not found")?,
    )?;
    let generation = parse_score_generation_img(
        div.select(selector!("img.music_kind_icon"))
            .next()
            .context("Generation img not found")?,
    )?;
    let score_metadata = ScoreMetadata::builder()
        .difficulty(difficulty)
        .generation(generation)
        .build();

    let song_name = find_and_parse_song_name(div)?;
    let level = find_and_parse_score_level(div)?;
    let achievement =
        find_and_parse_achievement_value(div)?.context("Achievement value not found")?;
    let idx = find_and_parse_score_idx(div)?;

    Ok(RatingTargetEntry {
        score_metadata,
        song_name,
        level,
        achievement,
        idx,
    })
}

#[allow(unused)]
#[derive(Debug, Serialize, Deserialize)]
pub struct RatingTargetList {
    target_new: Vec<RatingTargetEntry>,
    target_old: Vec<RatingTargetEntry>,
    candidates_new: Vec<RatingTargetEntry>,
    candidates_old: Vec<RatingTargetEntry>,
}
#[allow(unused)]
#[derive(Debug, Serialize, Deserialize)]
pub struct RatingTargetEntry {
    score_metadata: ScoreMetadata,
    song_name: String,
    level: ScoreLevel,
    achievement: AchievementValue,
    idx: ScoreIdx,
}

use std::collections::BTreeMap;

use anyhow::Context;
use getset::{CopyGetters, Getters};
use itertools::{Itertools, PeekingNext};
use scraper::ElementRef;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    parser::{
        play_record::{parse_playlog_diff, parse_score_generation_img},
        song_score::{
            find_and_parse_achievement_value, find_and_parse_score_idx, find_and_parse_score_level,
            find_and_parse_song_name, ScoreIdx,
        },
    },
    rating::ScoreLevel,
    schema::latest::{AchievementValue, PlayTime, RatingValue, ScoreMetadata, SongName},
};

pub fn parse(html: &scraper::Html) -> anyhow::Result<RatingTargetList> {
    let mut divs = html
        .select(selector!("div.see_through_block"))
        .next()
        .context("div.see_through_block not found")?
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .peekable();

    let rating = html
        .select(selector!("div.rating_block"))
        .next()
        .context("Rating block not found")?
        .text()
        .collect::<String>()
        .parse::<u16>()?
        .into();

    Ok(RatingTargetList {
        rating,
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

#[derive(Debug, Getters, CopyGetters, Serialize, Deserialize)]
pub struct RatingTargetList {
    #[getset(get_copy = "pub")]
    rating: RatingValue,
    #[getset(get = "pub")]
    target_new: Vec<RatingTargetEntry>,
    #[getset(get = "pub")]
    target_old: Vec<RatingTargetEntry>,
    #[getset(get = "pub")]
    candidates_new: Vec<RatingTargetEntry>,
    #[getset(get = "pub")]
    candidates_old: Vec<RatingTargetEntry>,
}
#[derive(Debug, Getters, CopyGetters, Serialize, Deserialize)]
pub struct RatingTargetEntry {
    #[getset(get_copy = "pub")]
    score_metadata: ScoreMetadata,
    #[getset(get = "pub")]
    song_name: SongName,
    #[getset(get_copy = "pub")]
    level: ScoreLevel,
    #[getset(get_copy = "pub")]
    achievement: AchievementValue,
    #[getset(get = "pub")]
    idx: ScoreIdx,
}

pub type RatingTargetFile = BTreeMap<PlayTime, RatingTargetList>;

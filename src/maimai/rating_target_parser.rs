use anyhow::Context;
use itertools::{Itertools, PeekingNext};
use scraper::ElementRef;

use super::{
    rating::ScoreLevel,
    schema::latest::{AchievementValue, ScoreMetadata},
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

pub fn parse_entries<'a>(
    mut elems: impl PeekingNext<Item = ElementRef<'a>>,
) -> anyhow::Result<Vec<RatingTargetEntry>> {
    let next = elems.next().context("No next element")?;
    assert!(selector!("div.screw_block").matches(&next));
    elems
        .peeking_take_while(|e| selector!("div.pointer").matches(e))
        .map(parse_entry)
        .collect()
}

pub fn parse_entry(div: ElementRef) -> anyhow::Result<RatingTargetEntry> {
    todo!()
}

#[allow(unused)]
#[derive(Debug)]
pub struct RatingTargetList {
    target_new: Vec<RatingTargetEntry>,
    target_old: Vec<RatingTargetEntry>,
    candidates_new: Vec<RatingTargetEntry>,
    candidates_old: Vec<RatingTargetEntry>,
}
#[allow(unused)]
#[derive(Debug)]
pub struct RatingTargetEntry {
    score_metadata: ScoreMetadata,
    song_name: String,
    level: ScoreLevel,
    achievement: AchievementValue,
}

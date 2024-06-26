use anyhow::Context;
use maimai_scraping_utils::selector;
use scraper::{ElementRef, Html};

use crate::sega_trait::AimeEntry;

pub fn parse(html: &Html) -> anyhow::Result<Vec<AimeEntry>> {
    html.select(selector!("div.aime_main_block"))
        .map(parse_aime_block)
        .collect()
}

pub fn parse_aime_block(div: ElementRef) -> anyhow::Result<AimeEntry> {
    let aime_idx = div
        .select(selector!(r#"input[name="idx"]"#))
        .next()
        .context("Aime idx input not found")?
        .value()
        .attr("value")
        .context("Aime idx input does not have `value` attribute")?
        .parse::<u8>()?
        .into();
    let player_name = div
        .select(selector!("div.name_block > span"))
        .next()
        .context("Player name block not found")?
        .text()
        .collect::<String>()
        .into();
    // TODO: is it paid????
    Ok(AimeEntry {
        idx: aime_idx,
        player_name,
        paid: false,
    })
}

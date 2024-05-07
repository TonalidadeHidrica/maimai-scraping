use anyhow::Context;
use itertools::Itertools;
use maimai_scraping_utils::selector;
use scraper::{selectable::Selectable, ElementRef, Html};
use serde::Serialize;

use crate::{cookie_store::FriendCode, sega_trait::AimeEntry};

pub const DIV: &str = "div.charge_aime_block,div.see_through_block";

pub fn parse(html: &Html) -> anyhow::Result<Vec<AimeEntry>> {
    html.select(selector!(DIV)).map(parse_aime_block).collect()
}

fn parse_aime_block(div: ElementRef) -> anyhow::Result<AimeEntry> {
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
        .select(selector!("div.name_block"))
        .next()
        .context("Player name block not found")?
        .text()
        .collect::<String>()
        .into();
    let paid = div.value().classes().contains(&"charge_aime_block");
    Ok(AimeEntry {
        idx: aime_idx,
        player_name,
        paid,
    })
}

#[derive(Serialize)]
pub struct ResetChargedAimeForm {
    idx: FriendCode,
    // This field is not exposed externally,
    // so we just use String instead of newtype
    token: String,
    // Empty string
    change: &'static str,
}

pub fn parse_paid_confirmation(html: &Html) -> anyhow::Result<ResetChargedAimeForm> {
    let idx = html
        .select(selector!(r#"input[name="idx"]"#))
        .next()
        .context("Aime idx input not found")?
        .attr("value")
        .context("Aime idx input does not have `value` attribute")?
        .to_owned()
        .into();
    let token = html
        .select(selector!(r#"input[name="token"]"#))
        .next()
        .context("Token input not found")?
        .value()
        .attr("value")
        .context("Token input does not have `value` attribute")?
        .to_owned();
    Ok(ResetChargedAimeForm {
        idx,
        token,
        change: "",
    })
}

use anyhow::Context;
use maimai_scraping_utils::selector;
use scraper::Html;

use crate::cookie_store::FriendCode;

pub const DIV: &str = "img.friend_code_icon + div";

pub fn parse(html: &Html) -> anyhow::Result<FriendCode> {
    Ok(html
        .select(selector!(DIV))
        .next()
        .context("Friend code div not found")?
        .text()
        .collect::<String>()
        .into())
}

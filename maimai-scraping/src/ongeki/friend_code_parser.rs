use anyhow::Context;
use scraper::Html;

use crate::cookie_store::FriendCode;

pub fn parse(html: &Html) -> anyhow::Result<FriendCode> {
    Ok(html
        .select(selector!("div.friendcode_block"))
        .next()
        .context("Friend code div not found")?
        .text()
        .collect::<String>()
        .into())
}

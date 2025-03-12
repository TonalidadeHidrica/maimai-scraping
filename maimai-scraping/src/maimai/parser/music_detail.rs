use anyhow::Context;
use derive_more::Into;
use getset::Getters;
use maimai_scraping_utils::selector;
use scraper::Html;

use crate::maimai::schema::latest::SongIcon;

#[derive(Getters, Into)]
#[getset(get = "pub")]
pub struct MusicDetails {
    icon: SongIcon,
}

pub fn parse(html: &Html) -> anyhow::Result<MusicDetails> {
    let icon = html
        .select(selector!("img.w_180"))
        .next()
        .context("Cover img not found in music detail page")?
        .attr("src")
        .context("Cover image has no src in music details page")?
        .parse()?;
    Ok(MusicDetails { icon })
}

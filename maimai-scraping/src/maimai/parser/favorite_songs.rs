use anyhow::Context;
use derive_more::{AsRef, Display, From};
use itertools::Itertools;
use maimai_scraping_utils::selector;
use scraper::ElementRef;
use serde::Serialize;

use crate::maimai::{official_song_list::Category, schema::latest::SongName};

pub fn parse(html: &scraper::Html) -> anyhow::Result<Page> {
    let token = html
        .select(selector!(r#"input[name="token"]"#))
        .next()
        .context("token not found")?
        .value()
        .attr("value")
        .context("Attribute `value` not found in the <input> for token")?
        .to_owned()
        .into();
    let songs = html
        .select(selector!("#list > div.m_t_10"))
        .map(parse_category)
        .flatten_ok()
        .map(|e| e?)
        .try_collect()?;
    Ok(Page { token, songs })
}

#[derive(Debug)]
pub struct Page {
    pub token: Token,
    pub songs: Vec<Song>,
}
#[derive(Debug, From, AsRef, Display, Serialize)]
#[as_ref(forward)]
pub struct Token(String);
#[derive(Debug)]
pub struct Song {
    pub category: Category,
    pub name: SongName,
    pub idx: Idx,
    pub checked: bool,
}
#[derive(Debug, From)]
pub struct GenreName(#[allow(unused)] String);
#[derive(Clone, PartialOrd, Ord, PartialEq, Eq, Debug, Hash, From, Serialize)]
pub struct Idx(String);

fn parse_category(
    element: ElementRef,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Song>> + '_> {
    let category = element
        .prev_siblings()
        .filter_map(ElementRef::wrap)
        .find(|e| selector!(".favorite_p_s").matches(e))
        .context("Genre name div not found")?
        .text()
        .collect::<String>()
        .parse()?;
    Ok(element
        .select(selector!("div.favorite_checkbox"))
        .map(move |element| parse_song(category, element)))
}

fn parse_song(category: Category, element: ElementRef) -> anyhow::Result<Song> {
    let name = element
        .select(selector!("div.favorite_music_name"))
        .next()
        .context("Song name div not found")?
        .text()
        .collect::<String>()
        .into();
    let checkbox = element
        .select(selector!("input"))
        .next()
        .context("Checkbox not found")?
        .value();
    let idx = checkbox
        .attr("value")
        .context("Attribute `value` does not exist")?
        .to_owned()
        .into();
    let checked = checkbox.attr("checked").is_some();
    Ok(Song {
        category,
        name,
        idx,
        checked,
    })
}

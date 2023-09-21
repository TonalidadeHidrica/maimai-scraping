use anyhow::Context;
use derive_more::From;
use scraper::ElementRef;
use serde::Serialize;

use crate::maimai::schema::latest::SongName;

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
    let genres = html
        .select(selector!("#list > div.m_t_10"))
        .map(parse_genre)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Page { token, genres })
}

#[derive(Debug)]
pub struct Page {
    pub token: Token,
    pub genres: Vec<Genre>,
}
#[derive(Debug)]
pub struct Genre {
    pub name: GenreName,
    pub songs: Vec<Song>,
}
#[derive(Debug, From, Serialize)]
pub struct Token(String);
#[derive(Debug)]
pub struct Song {
    pub name: SongName,
    pub idx: Idx,
    pub checked: bool,
}
#[derive(Debug, From)]
pub struct GenreName(String);
#[derive(Clone, Debug, From, Serialize)]
pub struct Idx(String);

fn parse_genre(element: ElementRef) -> anyhow::Result<Genre> {
    let name = element
        .prev_siblings()
        .filter_map(ElementRef::wrap)
        .find(|e| selector!(".favorite_p_s").matches(e))
        .context("Genre name div not found")?
        .text()
        .collect::<String>()
        .into();
    let songs = element
        .select(selector!("div.favorite_checkbox"))
        .map(parse_song)
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Genre { name, songs })
}

fn parse_song(element: ElementRef) -> anyhow::Result<Song> {
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
    Ok(Song { name, idx, checked })
}

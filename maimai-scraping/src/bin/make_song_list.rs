use std::{collections::HashMap, path::PathBuf};

use anyhow::{bail, Context};
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::{
    fs_json_util::{read_json, write_json},
    maimai::{
        load_score_level::{self, MaimaiVersion, SongRaw},
        rating::ScoreLevel,
        schema::latest::{SongIcon, SongName},
    },
};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use url::Url;

#[derive(Parser)]
struct Args {
    songs_json: PathBuf,
    output_json: PathBuf,

    #[clap(long)]
    levels_json: Option<PathBuf>,
    #[clap(long)]
    dictionary_json: Option<PathBuf>,
}

#[allow(unused)]
#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SongListEntry {
    title: String,
    title_kana: String,
    artist: String,
    /// Category (in Japanese, can be enum)
    catcode: String,
    image_url: String,

    /// Release date? (Can be "000000", unclear if it's reliable)
    release: String,
    /// Integer that decides default song order
    #[serde_as(as = "DisplayFromStr")]
    sort: u64,
    /// Five-digit integer that seeminlgy corresponds to the release date of score
    version: String,

    /// "NEW" if new song (or score?)
    date: Option<String>,
    dx_lev_adv: Option<String>,
    dx_lev_bas: Option<String>,
    dx_lev_exp: Option<String>,
    dx_lev_mas: Option<String>,
    dx_lev_remas: Option<String>,
    /// "○" if unlocking song is required
    key: Option<String>,
    lev_adv: Option<String>,
    lev_bas: Option<String>,
    lev_exp: Option<String>,
    lev_mas: Option<String>,
    lev_remas: Option<String>,

    /// Succeeded by "？" if utage
    lev_utage: Option<String>,
    /// Comment for utage score (perhaps)
    comment: Option<String>,
    /// Utage kanji
    kanji: Option<String>,
    /// "○" if the score is buddy
    buddy: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut songs: Vec<SongListEntry> = read_json(args.songs_json)?;
    songs.sort_by_key(|song| song.sort);

    let mut res = vec![];
    for song in &songs {
        let negate_version = if song.title == "前前前世" { -1 } else { 1 };
        let version = i8::from(convert_version(&song.version)?) * negate_version;

        let icon = song
            .image_url
            .strip_suffix(".png")
            .with_context(|| format!("Url does not end with .png: {:?}", song.image_url))?
            .to_owned();

        let song_raw = |dx, lv| SongRaw {
            dx,
            v: version,
            lv,
            n: song.title.clone(),
            nn: None,
            ico: icon.clone(),
        };

        let lvs_std = levels_to_lv([
            &song.lev_bas,
            &song.lev_adv,
            &song.lev_exp,
            &song.lev_mas,
            &song.lev_remas,
        ])?;
        let lvs_dx = levels_to_lv([
            &song.dx_lev_bas,
            &song.dx_lev_adv,
            &song.dx_lev_exp,
            &song.dx_lev_mas,
            &song.dx_lev_remas,
        ])?;
        if lvs_std.is_none() && lvs_dx.is_none() {
            match (&song.lev_utage, &song.comment, &song.kanji) {
                (Some(_), Some(_), Some(_)) => {} // utage
                _ => {
                    println!("Unexpected type of song: {song:?}");
                }
            }
        } else {
            let std = lvs_std.map(|lv| song_raw(0, lv));
            let dx = lvs_dx.map(|lv| song_raw(1, lv));
            res.extend([std, dx].into_iter().flatten());
        }
    }

    write_json(args.output_json, &res)?;

    if let Some(levels_json) = args.levels_json {
        let Some(dictionary_json) = args.dictionary_json else {
            bail!("Also specify --dictionary_json.");
        };

        #[derive(Serialize)]
        struct SuggestionEntry<'a> {
            song_name: &'a SongName,
            abbrev_name: &'a str,
            kana: &'a str,
        }

        let levels = load_score_level::load(levels_json)?;
        let mut icon_to_name = HashMap::new();
        for song in levels {
            icon_to_name.insert(song.icon().clone(), song);
        }
        let mut res = vec![];
        for song in &songs {
            let icon = SongIcon::from(Url::parse(&format!(
                "https://maimaidx.jp/maimai-mobile/img/Music/{}",
                song.image_url
            ))?);
            let entry = icon_to_name
                .get(&icon)
                .with_context(|| format!("No corresponding entry was found for {song:?}"))?;
            res.push(SuggestionEntry {
                song_name: entry.song_name(),
                abbrev_name: entry.song_name_abbrev(),
                kana: &song.title_kana,
            });
        }

        write_json(dictionary_json, &res)?;
    }

    Ok(())
}

fn convert_version(version: &str) -> anyhow::Result<MaimaiVersion> {
    use MaimaiVersion::*;
    let maimai_version = match version
        .get(..3)
        .with_context(|| format!("The legnth of version string is less than 3: {version:?}"))?
    {
        "100" => Maimai,
        "110" => MaimaiPlus,
        "120" => Green,
        "130" => GreenPlus,
        "140" => Orange,
        "150" => OrangePlus,
        "160" => Pink,
        "170" => PinkPlus,
        "180" => Murasaki,
        "185" => MurasakiPlus,
        "190" => Milk,
        "195" => MilkPlus,
        "199" => Finale,
        "200" => Deluxe,
        "205" => DeluxePlus,
        "210" => Splash,
        "215" => SplashPlus,
        "220" => Universe,
        "225" => UniversePlus,
        "230" => Festival,
        "235" => FestivalPlus,
        "240" => Buddies,
        "245" => BuddiesPlus,
        _ => bail!("Unrecognized version: {version:?}"),
    };
    Ok(maimai_version)
}

fn levels_to_lv(levels: [&Option<String>; 5]) -> anyhow::Result<Option<Vec<f64>>> {
    let levels_converted: Vec<Option<f64>> = levels
        .iter()
        .map(|lv| {
            lv.as_ref()
                .map(|lv| anyhow::Ok(-score_level_to_unknown_float(lv.parse::<ScoreLevel>()?)))
                .transpose()
        })
        .try_collect()?;
    match levels_converted
        .iter()
        .enumerate()
        .map(|(i, v)| (v.is_some() as u8) << i)
        .sum::<u8>()
    {
        0b01111 | 0b11111 => {}
        0 => return Ok(None),
        _ => bail!("Invalid levels: {levels:?} => {levels_converted:?}"),
    }
    Ok(Some(
        levels_converted
            .into_iter()
            .map(|v| v.unwrap_or(0.0))
            .collect(),
    ))
}

fn score_level_to_unknown_float(level: ScoreLevel) -> f64 {
    (level.level * 10 + level.plus as u8 * 6) as f64 / 10.
}

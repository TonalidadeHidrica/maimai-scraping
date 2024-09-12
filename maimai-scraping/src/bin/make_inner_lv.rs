use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::maimai::{
    load_score_level::{MaimaiVersion, SongRaw},
    rating::ScoreLevel,
    schema::latest::ScoreGeneration,
    song_list::{database::SongDatabase, Song},
};
use maimai_scraping_utils::fs_json_util::{read_json, write_json};

#[derive(Parser)]
struct Opts {
    database: PathBuf,
    in_lv_output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database)?;
    let database = SongDatabase::new(&songs)?;
    let version = MaimaiVersion::latest();

    let mut res = vec![];
    for song in database.songs() {
        if !song.song().remove_state.exist_for_version(version) {
            continue;
        }
        let song_name = AsRef::<str>::as_ref(song.latest_song_name());
        // let negate_version = if song_name == "前前前世" { -1 } else { 1 };
        // let version = if song_name == "ジングルベル" &&
        //     i8::from(convert_version(&song.version)?) * negate_version;
        let song_raw = |dx, lv, v| {
            anyhow::Ok(SongRaw {
                dx,
                v,
                lv,
                n: song_name.to_owned(),
                nn: None,
                ico: song
                    .song()
                    .icon
                    .as_ref()
                    .context("Song name absent")?
                    .standard_part()
                    .context("Nonstandard icon URL")?
                    .to_owned(),
            })
        };
        for scores in song.scoreses() {
            let dx = match scores.generation() {
                ScoreGeneration::Standard => 0,
                ScoreGeneration::Deluxe => 1,
            };
            let v = i8::from(scores.scores().version.context("Missing version")?);
            let v = if song_name == "前前前世" {
                -v
            } else if (song_name, scores.generation())
                == ("ジングルベル", ScoreGeneration::Standard)
            {
                0
            } else if version == MaimaiVersion::Maimai {
                1
            } else {
                v
            };
            let lv = scores
                .all_scores()
                // Unwrapping here because all the songs enumerated here should exist at this point
                // as we are filtering by `exist_for_version` in prior
                .filter_map(|score| score.for_version(version).unwrap().level())
                .map(|v| score_level_to_unknown_float(v.into_level(version)))
                .collect_vec();
            if !(lv.len() == 4 || lv.len() == 5) {
                bail!("Unexpected length");
            }
            res.push(song_raw(dx, lv, v)?);
        }
    }
    write_json(opts.in_lv_output, &res)?;

    Ok(())
}

fn score_level_to_unknown_float(level: ScoreLevel) -> f64 {
    -((level.level * 10 + level.plus as u8 * 6) as f64 / 10.)
}

use std::{iter::zip, path::PathBuf};

use clap::Parser;
use itertools::Itertools;
use maimai_scraping::maimai::song_list::{database::SongDatabase, maimai_char_order, Song};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let song_names = database
        .songs()
        .iter()
        .filter(|song| !song.song().removed())
        .filter_map(|song| Some((song, song.song().latest_pronunciation()?)))
        .sorted_by_key(|x| x.1)
        .tuple_windows()
        .filter_map(|(x, y)| {
            let [s, t] = [x, y].map::<_, &str>(|x| x.1.as_ref()).map(|x| x.chars());
            let (i, (s, t)) = zip(s, t).enumerate().find(|(_, (s, t))| s != t)?;
            let [s, t] = [s, t].map(|c| maimai_char_order(c).0);
            (s != t).then_some((i, [s, t], [x, y]))
        })
        .sorted();
    for (position, chars, songs) in song_names {
        println!(
            "{position} {chars:?} {:?}",
            songs.map(|song| (song.0.latest_song_name(), song.1))
        );
    }

    Ok(())
}

use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::bail;
use clap::Parser;
use fs_err::File;
use hashbrown::HashMap;
use itertools::Itertools;
use maimai_scraping::{
    maimai::{
        associated_user_data,
        schema::latest::SongName,
        song_list::{database::SongDatabase, Song},
        version::MaimaiVersion,
        MaimaiUserData,
    },
    sega_trait::PlayRecordTrait,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    user_data_path: PathBuf,
    list_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let user_data: MaimaiUserData = read_json(opts.user_data_path)?;
    let data = associated_user_data::UserData::annotate(&database, &user_data)?;

    let mut songs = HashMap::new();
    for line in BufReader::new(File::open(opts.list_path)?).lines() {
        let song_name = SongName::from(line?.to_owned());
        let song = match database.song_from_name(&song_name).collect_vec()[..] {
            [song] => song,
            ref songs => bail!("Song not unique for {song_name:?}: {songs:?}"),
        };
        songs.insert(song, None);
    }

    for record in data.ordinary_data_associated()?.ordinary_records() {
        if record.record().played_at().time().get() >= MaimaiVersion::Prism.start_time() {
            if let Some(date) = songs.get_mut(&record.score().score().scores().song()) {
                *date = Some(record.record().idx().timestamp_jst())
            }
        }
    }

    for (song, date) in songs {
        println!("{date:?} {}", song.latest_song_name());
    }

    Ok(())
}

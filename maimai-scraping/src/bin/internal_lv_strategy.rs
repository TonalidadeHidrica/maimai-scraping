use std::path::PathBuf;

use anyhow::{anyhow, bail};
use clap::Parser;
use maimai_scraping::{
    api::SegaClient,
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{ScoreConstantsStore, ScoreKey},
        favorite_songs::{fetch_favorite_songs_form, song_name_to_idx_map, SetFavoriteSong},
        load_score_level::{self, Song},
        rating::ScoreConstant,
        Maimai, MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    old_json: PathBuf,
    new_json: PathBuf,
    level: u8,
    datas: Vec<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let opts = Opts::parse();

    let old = load_score_level::load(&opts.old_json)?;
    let old = ScoreConstantsStore::new(&old, &[])?;
    let new = load_score_level::load(&opts.new_json)?;
    let mut new = ScoreConstantsStore::new(&new, &[])?;
    for data in opts.datas {
        let data: MaimaiUserData = read_json(data)?;
        new.do_everything(data.records.values(), &data.rating_targets)?;
    }

    let (mut client, _) =
        SegaClient::<Maimai>::new(&opts.credentials_path, &opts.cookie_store_path).await?;
    let page = fetch_favorite_songs_form(&mut client).await?;
    let map = song_name_to_idx_map(&page);
    let songs = songs(&old, &new, opts.level)?;
    let mut idxs = vec![];
    for (song, _) in songs {
        match &map.get(song.song_name()).map_or(&[][..], |x| &x[..]) {
            [] => bail!("Song not found: {song:?}"),
            [idx] => idxs.push(*idx),
            idxs => bail!("Multiple candidates are found: {song:?} {idxs:?}"),
        }
    }

    if idxs.len() > 30 {
        println!("Only 30 of the candidates were added.");
        idxs.drain(30..);
    }

    SetFavoriteSong::builder()
        .token(&page.token)
        .music(idxs)
        .build()
        .send(&mut client)
        .await?;

    Ok(())
}

fn songs<'os, 'ns>(
    old: &ScoreConstantsStore<'os, '_>,
    new: &ScoreConstantsStore<'ns, '_>,
    level: u8,
) -> anyhow::Result<Vec<(&'os Song, ScoreKey<'ns>)>> {
    let level = ScoreConstant::try_from(level).map_err(|e| anyhow!("Bad: {e}"))?;

    let mut ret = vec![];
    for (&key, entry) in new.scores() {
        let Ok(Some((song, candidates))) = old.get(key) else {
            continue;
        };
        if candidates == [level] && entry.candidates().len() != 1 {
            println!(
                "{} ({:?} {:?})",
                song.song_name(),
                key.generation,
                key.difficulty,
            );
            ret.push((song, key));
        }
    }
    ret.sort_by_key(|x| x.1.icon);
    Ok(ret)
}

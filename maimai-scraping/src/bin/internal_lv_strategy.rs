use std::{fmt::Display, path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail};
use clap::Parser;
use lazy_format::lazy_format;
use maimai_scraping::{
    api::SegaClient,
    cookie_store::UserIdentifier,
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{EstimatorConfig, ScoreConstantsStore, ScoreKey},
        favorite_songs::{fetch_favorite_songs_form, song_name_to_idx_map, SetFavoriteSong},
        load_score_level::{self, Song},
        rating::{ScoreConstant, ScoreLevel},
        schema::latest::SongName,
        Maimai, MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    old_json: PathBuf,
    new_json: PathBuf,
    levels: Levels,
    #[clap(long)]
    current: Option<ScoreLevel>,
    datas: Vec<PathBuf>,
    #[clap(long)]
    dry_run: bool,
    #[clap(flatten)]
    estimator_config: EstimatorConfig,
    #[clap(flatten)]
    user_identifier: UserIdentifier,
}
#[derive(Clone)]
struct Levels(Vec<ScoreConstant>);
impl FromStr for Levels {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            s.split(',')
                .map(|s| ScoreConstant::try_from(s.parse::<u8>()?).map_err(|e| anyhow!("Bad: {e}")))
                .collect::<anyhow::Result<Vec<ScoreConstant>>>()?,
        ))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let opts = Opts::parse();

    let old = load_score_level::load(&opts.old_json)?;
    let old = ScoreConstantsStore::new(&old, &[])?;
    let new = load_score_level::load(&opts.new_json)?;
    let mut new = ScoreConstantsStore::new(&new, &[])?;
    for data in &opts.datas {
        let data: MaimaiUserData = read_json(data)?;
        new.do_everything(
            opts.estimator_config,
            data.records.values(),
            &data.rating_targets,
        )?;
    }

    let songs = songs(&old, &new, &opts)?;

    for (i, &(song, key)) in songs.iter().enumerate() {
        println!("{i:>4} {}", display_song(song.song_name(), key));
    }

    if !opts.dry_run {
        let (mut client, _) = SegaClient::<Maimai>::new(
            &opts.credentials_path,
            &opts.cookie_store_path,
            &opts.user_identifier,
        )
        .await?;
        let page = fetch_favorite_songs_form(&mut client).await?;
        let map = song_name_to_idx_map(&page);
        let mut idxs = vec![];
        for (song, key) in songs {
            match &map.get(song.song_name()).map_or(&[][..], |x| &x[..]) {
                [] => println!("Song not found: {}", display_song(song.song_name(), key)),
                [idx] => idxs.push(*idx),
                idxs => bail!("Multiple candidates are found: {song:?} {idxs:?}"),
            }
        }
        if idxs.len() > 30 {
            println!("Only the first 30 of the candidates will be added.");
            idxs.drain(30..);
        }
        SetFavoriteSong::builder()
            .token(&page.token)
            .music(idxs)
            .build()
            .send(&mut client)
            .await?;
    } else {
        println!("WARNING: DRY-RUN!");
    }

    Ok(())
}

fn songs<'os, 'ns>(
    old: &ScoreConstantsStore<'os, '_>,
    new: &ScoreConstantsStore<'ns, '_>,
    opts: &Opts,
) -> anyhow::Result<Vec<(&'os Song, ScoreKey<'ns>)>> {
    let mut ret = vec![];
    for (&key, entry) in new.scores() {
        let Ok(Some((song, candidates))) = old.get(key) else {
            continue;
        };
        let old_level = (opts.levels.0.iter()).any(|&x| candidates.iter().any(|&y| x == y));
        let new_level = opts.current.map_or(true, |level| {
            level
                .score_constant_candidates()
                .any(|x| entry.candidates().iter().any(|&y| x == y))
        });
        let undetermined = entry.candidates().len() != 1;
        if old_level && new_level && undetermined {
            ret.push((song, key));
        }
    }
    ret.sort_by_key(|x| x.1.icon);
    Ok(ret)
}

fn display_song<'a>(name: &'a SongName, key: ScoreKey) -> impl Display + 'a {
    lazy_format!("{name} ({:?} {:?})", key.generation, key.difficulty)
}

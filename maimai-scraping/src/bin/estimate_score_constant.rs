use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{analyze_new_songs, guess_from_rating_target_order, ScoreConstantsStore},
        load_score_level::{self, RemovedSong},
        rating_target_parser::RatingTargetFile,
        schema::latest::PlayRecord,
    },
};
use tap::Tap;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    rating_target_file: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
    #[clap(long)]
    details: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    let records: Vec<PlayRecord> = read_json(opts.input_file)?;
    let rating_targets: RatingTargetFile = read_json(opts.rating_target_file)?;

    let levels = load_score_level::load(opts.level_file)?;
    let song_name_to_icon = HashMap::<_, HashSet<_>>::new().tap_mut(|map| {
        for song in &levels {
            map.entry(song.song_name()).or_default().insert(song.icon());
        }
    });

    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let removed_songs = load_score_level::make_map(&removed_songs, |r| r.icon())?;

    let levels = load_score_level::make_map(&levels, |song| (song.icon(), song.generation()))?;
    let mut levels = ScoreConstantsStore::new(levels, removed_songs, song_name_to_icon);
    levels.show_details = opts.details;
    if opts.details {
        println!("New songs");
    }
    analyze_new_songs(&records, &mut levels)?;
    for i in 1.. {
        if opts.details {
            println!("Iteration {i}");
        }
        levels.reset();
        guess_from_rating_target_order(&rating_targets, &mut levels)?;
        if !levels.updated {
            break;
        }
    }

    Ok(())
}

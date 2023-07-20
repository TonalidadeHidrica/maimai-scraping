use std::path::PathBuf;

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
    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let mut levels = ScoreConstantsStore::new(&levels, &removed_songs)?;

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

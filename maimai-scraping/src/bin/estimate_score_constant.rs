use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    data_collector::load_data_from_file,
    fs_json_util::read_json,
    maimai::{
        estimate_rating::ScoreConstantsStore,
        load_score_level::{self, RemovedSong},
        Maimai,
    },
};

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
    #[clap(long)]
    details: bool,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    let data = load_data_from_file::<Maimai, _>(&opts.maimai_user_data_path)?;

    let levels = load_score_level::load(opts.level_file)?;
    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let mut levels = ScoreConstantsStore::new(&levels, &removed_songs)?;

    levels.show_details = opts.details;
    levels.do_everything(data.records.values(), &data.rating_targets)?;

    Ok(())
}

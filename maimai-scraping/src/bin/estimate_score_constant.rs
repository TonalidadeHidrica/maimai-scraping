use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{EstimatorConfig, PrintResult, ScoreConstantsStore},
        load_score_level::{self, RemovedSong},
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
    #[clap(long)]
    details: bool,
    #[clap(flatten)]
    estimator_config: EstimatorConfig,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(&opts.maimai_user_data_path)?;

    let levels = load_score_level::load(opts.level_file)?;
    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let mut levels = ScoreConstantsStore::new(&levels, &removed_songs)?;

    levels.show_details = if opts.details {
        PrintResult::Detailed
    } else {
        PrintResult::Summarize
    };
    levels.do_everything(
        opts.estimator_config,
        None,
        data.records.values(),
        &data.rating_targets,
    )?;

    Ok(())
}

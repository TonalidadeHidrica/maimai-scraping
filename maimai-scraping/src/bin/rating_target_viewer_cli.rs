use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{visualize_rating_targets, EstimatorConfig, ScoreConstantsStore},
        load_score_level::{self, MaimaiVersion},
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    maimai_data_path: PathBuf,
    levels_json: PathBuf,
    #[clap(flatten)]
    estimator_config: EstimatorConfig,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();
    let data: MaimaiUserData = read_json(args.maimai_data_path)?;
    let levels = load_score_level::load(&args.levels_json)?;
    let mut constants = ScoreConstantsStore::new(&levels, &[])?;
    constants.do_everything(
        args.estimator_config,
        None,
        data.records.values(),
        &data.rating_targets,
    )?;

    for (time, file) in &data.rating_targets {
        let version = args
            .estimator_config
            .version
            .unwrap_or(MaimaiVersion::latest());
        if !(version.start_time()..version.end_time()).contains(&time.get()) {
            continue;
        }
        println!("{time}");
        println!("  New songs");
        visualize_rating_targets(&constants, file.target_new(), 0)?;
        println!("  =========");
        visualize_rating_targets(&constants, file.candidates_new(), file.target_new().len())?;
        println!("  Old songs");
        visualize_rating_targets(&constants, file.target_old(), 0)?;
        println!("  =========");
        visualize_rating_targets(&constants, file.candidates_old(), file.target_old().len())?;
        println!();
    }

    Ok(())
}

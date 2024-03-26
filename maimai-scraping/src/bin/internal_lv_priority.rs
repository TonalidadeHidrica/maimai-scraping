use std::path::PathBuf;

use clap::Parser;
use fs_err::read_to_string;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::ScoreConstantsStore, estimator_config_multiuser, load_score_level,
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    levels_json: PathBuf,
    config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();
    let config: estimator_config_multiuser::Root = toml::from_str(&read_to_string(args.config)?)?;
    let levels = load_score_level::load(&args.levels_json)?;
    let mut constants = ScoreConstantsStore::new(&levels, &[])?;
    while {
        println!("Iteration");
        let mut changed = false;
        for config in config.users() {
            let data: MaimaiUserData = read_json(config.data_path())?;
            changed |= constants.do_everything(
                config.estimator_config(),
                data.records.values(),
                &data.rating_targets,
            )?;
        }
        changed
    } {}

    Ok(())
}

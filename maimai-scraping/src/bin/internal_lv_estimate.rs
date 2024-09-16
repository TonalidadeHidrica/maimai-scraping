use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::{
    internal_lv_estimator::{self, multi_user::update_all, Estimator},
    load_score_level::MaimaiVersion,
    song_list::{self, database::SongDatabase},
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database: PathBuf,
    estimator_config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let songs: Vec<song_list::Song> = read_json(opts.database)?;
    let database = SongDatabase::new(&songs)?;

    let config: internal_lv_estimator::multi_user::Config =
        toml::from_str(&fs_err::read_to_string(opts.estimator_config)?)?;
    let datas = config.read_all()?;

    let mut estimator = Estimator::new(&database, MaimaiVersion::latest())?;
    let before_len = estimator.event_len();
    update_all(&database, &datas, &mut estimator)?;
    for event in &estimator.events()[before_len..] {
        println!("{event}");
    }

    Ok(())
}

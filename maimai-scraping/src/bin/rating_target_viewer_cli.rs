use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::{
    associated_user_data,
    internal_lv_estimator::{multi_user, visualize_rating_target, Estimator},
    load_score_level::MaimaiVersion,
    song_list::{database::SongDatabase, Song},
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    estimator_config_path: PathBuf,
    user_data_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let config: multi_user::Config =
        toml::from_str(&fs_err::read_to_string(opts.estimator_config_path)?)?;
    let datas = config.read_all()?;

    let version = MaimaiVersion::latest();
    let mut estimator = Estimator::new(&database, version)?;
    multi_user::update_all(&database, &datas, &mut estimator)?;

    let data: MaimaiUserData = read_json(opts.user_data_path)?;
    let data = associated_user_data::UserData::annotate(&database, &data)?;

    for (time, file) in data.rating_target() {
        if !(version.start_time()..version.end_time()).contains(&time.get()) {
            continue;
        }
        println!("{time}");
        println!("  New songs");
        for (entry, i) in file.target_new().iter().zip(0..) {
            println!("    {i:4} {}", visualize_rating_target(&estimator, entry));
        }
        println!("  =========");
        for (entry, i) in file.candidates_new().iter().zip(file.target_new().len()..) {
            println!("    {i:4} {}", visualize_rating_target(&estimator, entry));
        }
        println!("  Old songs");
        for (entry, i) in file.target_old().iter().zip(0..) {
            println!("    {i:4} {}", visualize_rating_target(&estimator, entry));
        }
        println!("  =========");
        for (entry, i) in file.candidates_old().iter().zip(file.target_old().len()..) {
            println!("    {i:4} {}", visualize_rating_target(&estimator, entry));
        }
        println!();
    }

    Ok(())
}

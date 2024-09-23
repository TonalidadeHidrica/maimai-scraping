use std::{collections::BTreeMap, path::PathBuf};

use clap::Parser;
use itertools::Itertools;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    internal_lv_estimator::multi_user, rating::ScoreLevel, MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    estimator_config: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let mut score_levels = ScoreLevel::range_inclusive(
        ScoreLevel::new(1, false).unwrap(),
        ScoreLevel::new(15, false).unwrap(),
    )
    .collect_vec();
    score_levels.reverse();

    println!(
        "                                {}",
        score_levels
            .iter()
            .map(|s| format!("{:3}", s.to_string()))
            .join(" ")
    );

    let config: multi_user::Config =
        toml::from_str(&fs_err::read_to_string(opts.estimator_config)?)?;
    for user in config.users() {
        let user_data: MaimaiUserData = read_json(user.data_path())?;
        if let Some((date, list)) = user_data.rating_targets.last_key_value() {
            let mut map = BTreeMap::<_, usize>::new();
            for entry in [list.target_old(), list.candidates_old()]
                .into_iter()
                .flatten()
                .filter(|x| x.achievement().get() >= 80_0000)
            {
                *map.entry(entry.level()).or_default() += 1;
            }
            let map = &map;
            println!(
                "{:10} {:?} {}",
                user.name().to_string(),
                date.get(),
                score_levels
                    .iter()
                    .map(|level| lazy_format!(match (map.get(level)) {
                        Some(v) => "{v:3}",
                        None => "   ",
                    }))
                    .join(" ")
            );
        }
    }

    Ok(())
}

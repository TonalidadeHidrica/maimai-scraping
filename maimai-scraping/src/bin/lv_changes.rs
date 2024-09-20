use std::{collections::BTreeMap, path::PathBuf};

use anyhow::Context;
use clap::Parser;
use enum_iterator::Sequence;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    internal_lv_estimator::{multi_user, Estimator},
    load_score_level::MaimaiVersion,
    rating::ScoreLevel,
    song_list::{database::SongDatabase, Song},
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    estimator_config: PathBuf,
    level: ScoreLevel,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let version = MaimaiVersion::latest();
    let previous_version = version
        .previous()
        .context("Given version has no previous version")?;

    let mut estimator = Estimator::new(&database, version)?;
    let estimator_config: multi_user::Config =
        toml::from_str(&fs_err::read_to_string(opts.estimator_config)?)?;
    let datas = estimator_config.read_all()?;
    multi_user::update_all(&database, &datas, &mut estimator)?;

    let mut res = BTreeMap::<_, BTreeMap<_, Vec<_>>>::new();
    for entry in estimator.get_scores() {
        let score = entry.score();
        if score.scores().scores().version == Some(version) {
            continue;
        }

        let previous = score
            .for_version(previous_version)
            .context("Previous version not found")?
            .level()
            .context("Previous version not found")?;
        let current = entry.candidates();

        if ![
            previous.into_level(previous_version),
            current.into_level(version),
        ]
        .iter()
        .any(|&x| x == opts.level)
        {
            continue;
        }

        let current = current
            .get_if_unique()
            .with_context(|| format!("Current level not unique: {score}"))?;
        let previous = previous
            .get_if_unique()
            .with_context(|| format!("Previous level not unique: {score}"))?;

        if current != previous {
            let diff = u8::from(current) as i16 - u8::from(previous) as i16;
            res.entry(diff)
                .or_default()
                .entry((previous, current))
                .or_default()
                .push(score);
        }
    }

    for (diff, scores) in res.into_iter().rev() {
        let label = if diff > 0 { "昇格" } else { "降格" };
        println!("{}段階{label}", diff.abs());
        for ((previous, current), mut scores) in scores {
            scores.sort();
            let lv_change =
                lazy_format!(match ((previous.to_lv(version), current.to_lv(version))) {
                    (x, y) if x != y => " ({x} → {y})",
                    _ => "",
                });
            println!("  - {previous} → {current}{lv_change}");
            for score in scores {
                println!("    - {score}");
            }
        }
    }

    Ok(())
}

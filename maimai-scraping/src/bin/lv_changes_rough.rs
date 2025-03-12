use std::{collections::BTreeMap, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use enum_iterator::Sequence;
use maimai_scraping::maimai::{
    rating::ScoreLevel,
    song_list::{database::SongDatabase, Song},
    version::MaimaiVersion,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let level_to_idx: BTreeMap<_, _> =
        ScoreLevel::range_inclusive("1".parse().unwrap(), "15".parse().unwrap())
            .zip(0i8..)
            .collect();

    let mut changed = BTreeMap::<_, BTreeMap<_, Vec<_>>>::new();
    let mut no_current = vec![];
    let mut no_previous = vec![];

    let current_version = MaimaiVersion::latest();
    let previous_version = current_version.previous().context("No previous version")?;

    for score in database.all_scores_for_version(current_version) {
        let Some(current) = score.level() else {
            no_current.push(score);
            continue;
        };
        let Some(previous) = score
            .score()
            .for_version(previous_version)
            .and_then(|x| x.level())
        else {
            no_previous.push(score);
            continue;
        };
        let previous = previous.into_level(previous_version);
        let current = current.into_level(current_version);
        if previous != current {
            changed
                .entry(previous)
                .or_default()
                .entry(current)
                .or_default()
                .push(score);
        }
    }

    println!("・過去のレベルが不明な譜面 (おそらく新曲)");
    for score in no_previous {
        println!("  ・{}", score.score());
    }
    println!();
    println!("・現在のレベルが不明な譜面");
    for score in no_current {
        println!("  ・{}", score.score());
    }
    println!();

    for (previous, map) in changed.iter().rev() {
        println!("・Lv.{previous}");
        for (current, scores) in map.iter().rev() {
            let diff = level_to_idx[current] - level_to_idx[previous];
            let label = if diff > 0 { "昇格" } else { "降格" };
            println!("  ➡️ Lv.{current}  [{}段階{label}]", diff.abs());
            for score in scores {
                println!("    ・{}", score.score());
            }
        }
        println!();
    }

    Ok(())
}

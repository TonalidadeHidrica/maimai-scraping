use std::{collections::BTreeSet, iter::repeat, path::PathBuf};

use anyhow::bail;
use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{
            single_song_rating_for_target_entry, EstimatorConfig, ScoreConstantsStore,
        },
        load_score_level::{self, MaimaiVersion},
        parser::rating_target::RatingTargetEntry,
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
        data.records.values(),
        &data.rating_targets,
    )?;

    for (time, file) in &data.rating_targets {
        if time.get() < MaimaiVersion::latest().start_time() {
            continue;
        }
        println!("{time}");
        println!("  New songs");
        display(&constants, file.target_new())?;
        println!("  =========");
        display(&constants, file.candidates_new())?;
        println!("  Old songs");
        display(&constants, file.target_old())?;
        println!("  =========");
        display(&constants, file.candidates_old())?;
        println!();
    }

    Ok(())
}

fn display(constants: &ScoreConstantsStore, entries: &[RatingTargetEntry]) -> anyhow::Result<()> {
    for entry in entries {
        let Some((_, levels)) = constants.levels_from_target_entry(entry)? else {
            bail!("Song unexpectedly removed!")
        };
        let levels = BTreeSet::from_iter(levels.iter().copied());
        print!("    {:<3} ", format!("{}", entry.level()));
        let constants = || entry.level().score_constant_candidates();
        let fill = repeat(None).take(6usize.saturating_sub(constants().count()));
        for constant in constants().map(Some).chain(fill) {
            match constant {
                Some(constant) if levels.contains(&constant) => {
                    let value = single_song_rating_for_target_entry(constant, entry);
                    print!("[{:>3}] ", value.get())
                }
                Some(_) => print!("[   ] "),
                None => print!("      "),
            }
        }
        let s = entry.score_metadata();
        print!(
            "{:9} {} ({:?} {:?})",
            entry.achievement(),
            entry.song_name(),
            s.generation(),
            s.difficulty()
        );
        println!();
    }
    Ok(())
}

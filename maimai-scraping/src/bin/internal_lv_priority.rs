use std::{collections::BTreeSet, path::PathBuf};

use clap::Parser;
use fs_err::read_to_string;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{KeyFromTargetEntry, PrintResult, ScoreConstantsStore},
        estimator_config_multiuser,
        load_score_level::{self, MaimaiVersion},
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    old_levels_json: PathBuf,
    levels_json: PathBuf,
    config: PathBuf,

    #[clap(default_value = "10")]
    level_update_factor: f64,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();
    let config: estimator_config_multiuser::Root = toml::from_str(&read_to_string(args.config)?)?;
    let datas = (config.users().iter())
        .map(|config| anyhow::Ok((config, read_json::<_, MaimaiUserData>(config.data_path())?)))
        .collect::<Result<Vec<_>, _>>()?;

    let old_levels = load_score_level::load(&args.old_levels_json)?;
    let old_store = ScoreConstantsStore::new(&old_levels, &[])?;

    let levels = load_score_level::load(&args.levels_json)?;
    let mut store = ScoreConstantsStore::new(&levels, &[])?;
    store.show_details = PrintResult::Quiet;

    update_all(&datas, &mut store)?;

    let undetermined_song_in_list = datas
        .iter()
        .flat_map(|(_, data)| &data.rating_targets)
        .filter_map(|(k, v)| (MaimaiVersion::latest().start_time() <= k.get()).then_some(v))
        .flat_map(|r| {
            [
                r.target_new(),
                r.target_old(),
                r.candidates_new(),
                r.candidates_old(),
            ]
        })
        .flatten()
        .filter_map(|entry| match store.key_from_target_entry(entry) {
            KeyFromTargetEntry::Unique(key) => Some(key),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    for key in undetermined_song_in_list {
        let Ok(Some((song, constants))) = store.get(key) else {
            continue;
        };
        if constants.len() <= 1 {
            continue;
        }
        let old_constants = if let Ok(Some((_, old_constants))) = old_store.get(key) {
            old_constants
        } else {
            println!("Warning: constans cannot be rertrieved: {key:?}");
            &[]
        };
        let mut factor_sum = 0f64;
        let mut weighted_count_sum = 0f64;
        for &constant in constants {
            let factor = if old_constants.is_empty() {
                1.
            } else {
                old_constants
                    .iter()
                    .map(|&c| {
                        args.level_update_factor
                            .powi((u8::from(c)).abs_diff(u8::from(constant)) as _)
                    })
                    .sum()
            };

            let mut store = store.clone();
            let count_before = store.num_determined_songs();
            // Error shuold not occur at this stage
            store.set(key, [constant], "assumption")?;
            update_all(&datas, &mut store)?;
            let count_determined_anew = store.num_determined_songs() - count_before;
            factor_sum += factor;
            weighted_count_sum += factor * count_determined_anew as f64;
        }
        let expected_count = weighted_count_sum / factor_sum;
        println!(
            "{} {:?} {:?} {expected_count:.3} more songs",
            song.song_name(),
            key.generation,
            key.difficulty,
        );
    }

    Ok(())
}

fn update_all(
    datas: &[(&estimator_config_multiuser::User, MaimaiUserData)],
    constants: &mut ScoreConstantsStore,
) -> anyhow::Result<()> {
    while {
        let mut changed = false;
        for (config, data) in datas {
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

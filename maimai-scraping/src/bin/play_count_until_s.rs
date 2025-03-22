use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chrono::Duration;
use clap::Parser;
use hashbrown::HashMap;
use itertools::Itertools;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    associated_user_data::{self, OrdinaryPlayRecordAssociated},
    rating::ScoreConstant,
    schema::latest::AchievementValue,
    song_list::{database::SongDatabase, Song},
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    user_data_path: PathBuf,

    #[arg(long, default_value = "10")]
    level_lower_bound: u8,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let level_lower_bound = ScoreConstant::try_from(opts.level_lower_bound)
        .map_err(|e| anyhow!("Invalid lower bound {e}"))?;

    let database: Vec<Song> = read_json(&opts.database_path)?;
    let database = SongDatabase::new(&database)?;
    let user_data: MaimaiUserData = read_json(&opts.user_data_path)?;
    let data = associated_user_data::UserData::annotate(&database, &user_data)?;

    let mut map = HashMap::<_, State>::new();
    let mut print_index = 0usize;
    #[derive(Default)]
    struct State<'d, 's> {
        results: Vec<OrdinaryPlayRecordAssociated<'d, 's>>,
        is_s: bool,
    }
    for &record in data.ordinary_data_associated()?.ordinary_records() {
        let state = map.entry(record.score().score()).or_default();
        if state.is_s {
            continue;
        }
        state.results.push(record);
        if record.record().achievement_result().value()
            >= AchievementValue::try_from(97_0000).unwrap()
        {
            state.is_s = true;

            let score_level = (record.score().score().score().levels.values().flatten()).last();
            let matches =
                score_level.is_none_or(|x| x.candidates().any(|x| x >= level_lower_bound));
            if !matches {
                continue;
            }
            let to_days = |results: &[OrdinaryPlayRecordAssociated]| {
                results
                    .iter()
                    .map(|x| x.record().played_at().time().to_natural_date())
                    .dedup()
                    .collect_vec()
            };
            let days = to_days(&state.results);
            let last_interval = days
                .iter()
                .tuple_windows()
                .filter(|(&x, &y)| y - x >= Duration::days(14))
                .map(|x| *x.1)
                .last();
            let results_serious = state
                .results
                .iter()
                .skip_while(|x| {
                    last_interval
                        .is_some_and(|y| x.record().played_at().time().to_natural_date() < y)
                })
                .copied()
                .collect_vec();
            let days_serious = to_days(&results_serious);
            println!(
                "{print_index:3} {first_date} {serious_date} {last_date} {play_count:3} {day_count:3} {serious_play_count:3} {serious_day_count:3} {score_level} {score}",
                first_date = days[0],
                serious_date = days_serious[0],
                last_date = days.last().unwrap(),
                play_count = state.results.len(),
                day_count = days.len(),
                serious_play_count = results_serious.len(),
                serious_day_count = days_serious.len(),
                score_level = lazy_format!(match (score_level) {
                    Some(l) => "{l:4>}",
                    None => " ?? ",
                }),
                score = record.score().score(),
            );
            print_index += 1;
        }
    }

    Ok(())
}

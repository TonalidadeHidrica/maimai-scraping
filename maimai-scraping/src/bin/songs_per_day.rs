use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use chrono::Duration;
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::maimai::MaimaiUserData;
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    #[clap(long)]
    sort_by_date: bool,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(opts.input_file)?;
    let mut count = BTreeMap::<_, (usize, BTreeSet<_>)>::new();
    for record in data.records.values() {
        let time = record.played_at().time();
        let date = (time.get() - Duration::hours(5)).date();
        // println!("{time} {date}");
        let (count, players) = count.entry(date).or_default();
        *count += 1;
        players.extend(record.matching_result().iter().flat_map(|x| {
            let x: &[_] = x.other_players().as_ref();
            x.iter().map(|x| x.user_name())
        }));
    }
    let mut count = count.iter().map(|(x, y)| (y, x)).collect_vec();
    if !opts.sort_by_date {
        count.sort_by_key(|x| x.0 .0);
    }
    for ((count, names), date) in count {
        println!("{count:3}  {date} with {names:?}");
    }

    Ok(())
}

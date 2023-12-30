use std::{collections::BTreeMap, path::PathBuf};

use chrono::Duration;
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::{fs_json_util::read_json, maimai::MaimaiUserData};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(opts.input_file)?;
    let mut count = BTreeMap::<_, usize>::new();
    for (_, record) in data.records {
        let time = record.played_at().time();
        let date = (time.get() - Duration::hours(5)).date();
        println!("{time} {date}");
        *count.entry(date).or_default() += 1;
    }
    let mut count = count.iter().map(|(&x, &y)| (y, x)).collect_vec();
    count.sort();
    for (count, date) in count {
        println!("{count:3}  {date}");
    }

    Ok(())
}

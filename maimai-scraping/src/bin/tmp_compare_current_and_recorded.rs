use std::{collections::BTreeMap, path::PathBuf};

use clap::Parser;
use maimai_scraping::{fs_json_util::read_json, maimai::schema::latest::PlayRecord};

#[derive(Parser)]
struct Opts {
    recorded: PathBuf,
    current: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let recorded: Vec<PlayRecord> = read_json(&opts.recorded)?;
    let current: Vec<PlayRecord> = read_json(&opts.current)?;
    let recorded = recorded
        .into_iter()
        .map(|x| (x.played_at().time(), x))
        .collect::<BTreeMap<_, _>>();
    for current_record in current {
        let time = current_record.played_at().time();
        let recorded_record = &recorded[&time];
        let old_r = recorded_record.rating_result();
        let new_r = current_record.rating_result();
        println!(
            "{}\t{}\t{}({:+})\t{}({:+})",
            time,
            &current_record == recorded_record,
            old_r.rating(),
            old_r.delta(),
            new_r.rating(),
            new_r.delta()
        );
    }

    Ok(())
}

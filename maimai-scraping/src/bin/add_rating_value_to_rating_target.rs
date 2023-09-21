// Only used for migration

use std::{collections::BTreeMap, path::PathBuf};

use clap::Parser;
use maimai_scraping::{
    fs_json_util::{read_json, write_json},
    maimai::{parser::rating_target::RatingTargetFile, schema::latest::PlayRecord},
};

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let records: Vec<PlayRecord> = read_json(opts.records_file)?;
    let mut rating_targets: RatingTargetFile = read_json(opts.rating_target_file)?;
    let records: BTreeMap<_, _> = records
        .into_iter()
        .map(|r| (r.played_at().time(), r))
        .collect();
    #[allow(unused)]
    for (time, list) in rating_targets.iter_mut() {
        let rating = records[time].rating_result().rating();
        // list.rating = Some(rating);
    }

    write_json(opts.records_file_new, &rating_targets)?;
    Ok(())
}

#[derive(Parser)]
struct Opts {
    records_file: PathBuf,
    rating_target_file: PathBuf,
    records_file_new: PathBuf,
}

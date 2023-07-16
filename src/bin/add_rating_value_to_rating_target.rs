// Only used for migration

use std::{
    collections::BTreeMap,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::{
    rating_target_parser::RatingTargetList,
    schema::latest::{PlayRecord, PlayTime},
};

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.records_file)?))?;
    let mut rating_targets: BTreeMap<PlayTime, RatingTargetList> =
        serde_json::from_reader(BufReader::new(File::open(opts.rating_target_file)?))?;
    let records: BTreeMap<_, _> = records
        .into_iter()
        .map(|r| (r.played_at().time(), r))
        .collect();
    #[allow(unused)]
    for (time, list) in rating_targets.iter_mut() {
        let rating = records[time].rating_result().rating();
        // list.rating = Some(rating);
    }

    serde_json::to_writer(
        BufWriter::new(File::create(opts.records_file_new)?),
        &rating_targets,
    )?;
    Ok(())
}

#[derive(Parser)]
struct Opts {
    records_file: PathBuf,
    rating_target_file: PathBuf,
    records_file_new: PathBuf,
}

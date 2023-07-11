use std::{
    collections::BTreeMap,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::bail;
use chrono::NaiveDateTime;
use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::rating_target_parser::{self, RatingTargetList};
use scraper::Html;

#[derive(Parser)]
struct Opts {
    json: PathBuf,
    date: NaiveDateTime,
    html: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut rating_targets: BTreeMap<NaiveDateTime, RatingTargetList> =
        serde_json::from_reader(BufReader::new(File::open(&opts.json)?))?;
    let list =
        rating_target_parser::parse(&Html::parse_document(&fs_err::read_to_string(opts.html)?))?;
    if rating_targets.insert(opts.date, list).is_some() {
        bail!("Entry on {} is already present", opts.date);
    }
    let file = BufWriter::new(File::create(opts.json)?);
    serde_json::to_writer(file, &rating_targets)?;
    Ok(())
}

use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;
use maimai_scraping::maimai::{
    parser::{self, rating_target::RatingTargetFile},
    schema::latest::PlayTime,
};
use maimai_scraping_utils::fs_json_util::{read_json, write_json};
use scraper::Html;

#[derive(Parser)]
struct Opts {
    json: PathBuf,
    date: PlayTime,
    html: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut rating_targets: RatingTargetFile = read_json(&opts.json)?;
    let list =
        parser::rating_target::parse(&Html::parse_document(&fs_err::read_to_string(opts.html)?))?;
    if rating_targets.insert(opts.date, list).is_some() {
        bail!("Entry on {} is already present", opts.date);
    }
    write_json(opts.json, &rating_targets)?;
    Ok(())
}

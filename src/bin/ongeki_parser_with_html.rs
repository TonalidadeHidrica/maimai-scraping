use std::path::PathBuf;

use clap::Parser;
use fs_err::read_to_string;
use maimai_scraping::ongeki::{self, schema::latest::Idx};
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&read_to_string(&opts.input_file)?);
    let result = ongeki::play_record_parser::parse(&html, Idx::try_from(0)?)?;
    dbg!(&result);
    Ok(())
}

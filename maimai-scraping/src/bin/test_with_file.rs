use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::schema::latest::{Idx, PlayRecord};
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input_file)?);

    let result = maimai_scraping::maimai::parser::play_record::parse(&html, Idx::default())?;
    dbg!(&result);
    let serialized = serde_json::to_string_pretty(&result)?;
    println!("{}", &serialized);
    let deserialized: PlayRecord = serde_json::from_str(&serialized)?;
    dbg!(&deserialized);

    assert_eq!(result, deserialized);

    Ok(())
}

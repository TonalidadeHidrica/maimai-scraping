use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::ongeki::{check_no_loss, schema::latest::PlayRecord};
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input_file)?);

    let result = maimai_scraping::ongeki::play_record_parser::parse(&html, 0.try_into().unwrap())?;
    dbg!(&result);
    let serialized = serde_json::to_string_pretty(&result)?;
    println!("{}", &serialized);
    let deserialized: PlayRecord = serde_json::from_str(&serialized)?;
    dbg!(&deserialized);

    assert_eq!(result, deserialized);

    check_no_loss(&html, &result)?;

    Ok(())
}

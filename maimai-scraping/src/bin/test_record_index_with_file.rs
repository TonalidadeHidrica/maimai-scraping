use std::path::PathBuf;

use clap::Parser;
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input_file)?);

    let result = maimai_scraping::maimai::parser::play_record::parse_record_index(&html)?;
    dbg!(&result);

    Ok(())
}

use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai;
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input)?);
    let res = maimai::parser::rating_target::parse(&html)?;
    println!("{res:?}");
    Ok(())
}

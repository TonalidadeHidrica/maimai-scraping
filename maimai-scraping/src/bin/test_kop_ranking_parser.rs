use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::parser;
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input_file)?);
    let ranking = parser::kop_ranking::parse(&html)?;
    println!("as_of = {:?}", ranking.as_of);
    for entry in &ranking.entries {
        println!("    {entry:?}");
    }
    Ok(())
}

use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai;
use scraper::Html;

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&fs_err::read_to_string(opts.input_file)?);
    let res = maimai::parser::song_score::parse(&html)?;
    for entry in res {
        println!("{entry:#?}");
    }
    Ok(())
}

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

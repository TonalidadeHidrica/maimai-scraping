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
    let page = parser::favorite_songs::parse(&html)?;
    println!("token = {:?}", page.token);
    for genre in page.genres {
        println!("  {:?}", genre.name);
        for song in genre.songs {
            println!("    {song:?}");
        }
    }
    Ok(())
}

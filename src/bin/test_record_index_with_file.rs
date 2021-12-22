use std::{
    io::{BufReader, Read},
    path::PathBuf,
};

use clap::Parser;
use fs_err::File;
use scraper::Html;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = {
        let mut file = BufReader::new(File::open(&opts.input_file)?);
        let mut html = String::new();
        file.read_to_string(&mut html)?;
        Html::parse_document(&html)
    };

    let result = maimai_scraping::play_record_parser::parse_record_index(&html)?;
    dbg!(&result);

    Ok(())
}

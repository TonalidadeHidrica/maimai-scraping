use clap::Parser;
use maimai_scraping::schema::PlayRecord;
use scraper::Html;
use std::{fs::File, io::BufReader, io::Read, path::PathBuf};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = {
        let mut file = BufReader::new(File::open(&opts.input_file)?);
        let mut html = String::new();
        file.read_to_string(&mut html)?;
        Html::parse_document(&html)
    };

    let result = maimai_scraping::play_record_parser::parse(html)?;
    dbg!(&result);
    let serialized = serde_json::to_string_pretty(&result)?;
    println!("{}", &serialized);
    let deserialized: PlayRecord = serde_json::from_str(&serialized)?;
    dbg!(&deserialized);

    assert_eq!(result, deserialized);

    Ok(())
}

use clap::{App, Arg};
use maimai_scraping::schema::PlayRecord;
use scraper::Html;
use std::{fs::File, io::BufReader, io::Read};

fn main() -> anyhow::Result<()> {
    let matches = App::new("hoge")
        .arg(Arg::with_name("file").required(true))
        .get_matches();
    let html = {
        let mut file = BufReader::new(File::open(matches.value_of("file").unwrap())?);
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

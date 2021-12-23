use std::{
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::schema::latest::PlayRecord;
use serde_json::Value;

#[derive(Parser)]
struct Opts {
    json_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let path = &opts.json_file;
    let mut value: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    for (obj, i) in value.as_array_mut().unwrap().iter_mut().zip(12..62) {
        let obj = obj.as_object_mut().unwrap();
        let played_at = obj.get_mut("played_at").unwrap().as_object_mut().unwrap();
        played_at.insert("idx".to_owned(), (i % 50).into());
    }
    let records: Vec<PlayRecord> = serde_json::from_value(value)?;
    serde_json::to_writer(BufWriter::new(File::create(path)?), &records)?;

    Ok(())
}

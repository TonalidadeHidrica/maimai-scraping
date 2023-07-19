use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{
    fs_json_util::{read_json, write_json},
    maimai::schema::latest::PlayRecord,
};
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
    let mut value: Value = read_json(path)?;
    for (obj, i) in value.as_array_mut().unwrap().iter_mut().zip(12..62) {
        let obj = obj.as_object_mut().unwrap();
        let played_at = obj.get_mut("played_at").unwrap().as_object_mut().unwrap();
        played_at.insert("idx".to_owned(), (i % 50).into());
    }
    let records: Vec<PlayRecord> = serde_json::from_value(value)?;
    write_json(path, &records)?;

    Ok(())
}

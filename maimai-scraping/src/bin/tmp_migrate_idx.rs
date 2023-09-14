use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::fs_json_util::{read_json, write_json};
use serde_json::{Map, Value};

#[derive(Parser)]
struct Opts {
    old_file: PathBuf,
    new_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut data: Value = read_json(opts.old_file)?;
    for record in data
        .as_object_mut()
        .unwrap()
        .get_mut("records")
        .unwrap()
        .as_array_mut()
        .unwrap()
    {
        let old_idx = record
            .as_object_mut()
            .unwrap()
            .get_mut("played_at")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .get_mut("idx")
            .unwrap();
        let mut new_idx = Map::new();
        new_idx.insert(
            "index".to_owned(),
            Value::Number(old_idx.as_u64().unwrap().into()),
        );
        *old_idx = Value::Object(new_idx);
    }

    write_json(&opts.new_file, &data)?;
    Ok(())
}

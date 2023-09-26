use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::fs_json_util::{read_json, write_json};
use serde_json::Value;

#[derive(Parser)]
struct Opts {
    old_file: PathBuf,
    new_file: PathBuf,
    count: usize,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut data: Value = read_json(opts.old_file)?;
    let records = data
        .as_object_mut()
        .unwrap()
        .get_mut("records")
        .unwrap()
        .as_array_mut()
        .unwrap();
    records.drain(records.len().saturating_sub(opts.count)..);
    write_json(&opts.new_file, &data)?;
    Ok(())
}

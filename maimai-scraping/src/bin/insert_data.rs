use std::{collections::VecDeque, path::PathBuf};

use anyhow::bail;
use clap::Parser;
use maimai_scraping::{
    fs_json_util::{read_json, write_json},
    maimai::schema::latest::PlayRecord,
};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    insert_file: PathBuf,
    insert_pos: usize,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let read = |path: &PathBuf| anyhow::Ok(read_json::<_, Vec<PlayRecord>>(path)?);
    let mut records = read(&opts.input_file)?;
    let mut inserted = VecDeque::from_iter(read(&opts.insert_file)?);
    let pos = opts.insert_pos;
    let [before, after, ..] = &records[pos - 1 ..] else {
        bail!("Can only insert between two elements")
    };
    let Some(first) = inserted.pop_front() else {
        bail!("Inserted json must have at least two elements")
    };
    let Some(last) = inserted.pop_back() else {
        bail!("Inserted json must have at least two elements")
    };
    assert_eq!(before, &first);
    assert_eq!(after, &last);
    records.splice(pos..pos, inserted);

    for (i, records) in records.iter().enumerate() {
        assert_eq!((i % 50) as u8, u8::from(records.played_at().idx()));
    }
    println!("Done!");

    write_json(&opts.input_file, &records)?;

    Ok(())
}

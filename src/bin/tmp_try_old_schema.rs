use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use maimai_scraping::schema::ver_20210316_2338::PlayRecord;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.input_file)?))?;
    dbg!(&records);

    Ok(())
}

use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::schema::ver_20210316_2338::PlayRecord as OldPlayRecord;

#[derive(Parser)]
struct Opts {
    old_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let old_records: Vec<OldPlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.old_file)?))?;
    let (old_record_lost, _old_record_overlapping) = old_records.split_at(12);

    for record in old_record_lost {
        let rat = record.rating_result();
        println!(
            "{}\t{:<12?} {}({:+})\t{}",
            record.played_at().time(),
            record.score_metadata().generation(),
            rat.rating(),
            rat.delta(),
            rat.rating().get() * 8 / 5,
        );
    }

    Ok(())
}

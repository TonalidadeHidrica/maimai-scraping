use std::{io::BufReader, iter::once, path::PathBuf};

use clap::Parser;
use fs_err::File;
use itertools::zip;
use maimai_scraping::schema::{
    latest::PlayRecord as NewPlayRecord, ver_20210316_2338::PlayRecord as OldPlayRecord,
};

#[derive(Parser)]
struct Opts {
    old_file: PathBuf,
    new_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let old_records: Vec<OldPlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.old_file)?))?;
    let (old_record_lost, old_record_overlapping) = old_records.split_at(12);
    let new_records: Vec<NewPlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.new_file)?))?;

    for record in old_record_lost {
        let rat = record.rating_result();
        println!(
            "{}\t{:<12} {}({:+})\t{}",
            record.played_at().time(),
            format!("{:?}", record.score_metadata().generation()),
            rat.rating(),
            rat.delta(),
            rat.rating().get() * 8 / 5,
        );
    }

    for (old, new) in zip(old_record_overlapping, new_records.iter()) {
        assert_eq!(old.played_at().time(), new.played_at().time());
        let old_r = old.rating_result();
        let new_r = new.rating_result();
        println!(
            "{}\t{:<12} {}({:+})\t{}({:+})",
            old.played_at().time(),
            format!("{:?}", old.score_metadata().generation()),
            old_r.rating(),
            old_r.delta(),
            new_r.rating(),
            new_r.delta()
        );
    }

    let ratings = new_records
        .iter()
        .map(|x| x.rating_result().rating().get() as i16);
    let deltas = new_records.iter().map(|x| *x.rating_result().delta());
    for ((bef, aft), delta) in once(0).chain(ratings.clone()).zip(ratings).zip(deltas) {
        if bef + delta != aft {
            println!("{} {:+} {}", bef, delta, aft);
        }
    }

    Ok(())
}

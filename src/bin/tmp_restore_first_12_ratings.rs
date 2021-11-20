use std::{io::BufReader, iter::once, path::PathBuf};

use clap::Parser;
use fs_err::File;
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
    let (old_record_lost, _old_record_overlapping) = old_records.split_at(12);
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

    for (old, new) in once(None)
        .chain(new_records.iter().map(Some))
        .zip(&new_records)
    {
        let bef = old.map_or(0, |x| x.rating_result().rating().get() as i16);
        let aft = new.rating_result().rating().get() as i16;
        let delta = new.rating_result().delta();
        let bef_date = match old {
            Some(old) => format!("{}", old.played_at().time()),
            _ => "Initial".into(),
        };
        if bef + delta != aft {
            println!(
                "{}({}) {:+} {}({})",
                bef,
                bef_date,
                delta,
                aft,
                new.played_at().time()
            );
        }
    }

    Ok(())
}

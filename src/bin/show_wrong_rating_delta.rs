use std::{io::BufReader, iter::once, path::PathBuf};

use clap::Parser;
use fs_err::File;
use maimai_scraping::maimai::schema::latest::PlayRecord;

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.input_file)?))?;

    for (old, new) in once(None).chain(records.iter().map(Some)).zip(&records) {
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

use std::{iter::once, path::PathBuf};

use clap::Parser;
use maimai_scraping::{fs_json_util::read_json, maimai::schema::latest::PlayRecord};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> = read_json(opts.input_file)?;

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

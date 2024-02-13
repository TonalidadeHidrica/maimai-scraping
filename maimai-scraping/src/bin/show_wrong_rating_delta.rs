use std::{iter::once, path::PathBuf};

use clap::Parser;
use maimai_scraping::{fs_json_util::read_json, maimai::MaimaiUserData};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    #[clap(long)]
    within_specific_date: bool,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(opts.input_file)?;

    for (old, new) in once(None)
        .chain(data.records.values().map(Some))
        .zip(data.records.values())
    {
        let bef = old.map_or(0, |x| x.rating_result().rating().get() as i16);
        let aft = new.rating_result().rating().get() as i16;
        let delta = new.rating_result().delta();
        let bef_date = match old {
            Some(old) => format!("{}", old.played_at().time()),
            _ => "Initial".into(),
        };
        let within_specific_date = old.is_some_and(|old| {
            use chrono::NaiveDate;
            let date = old.played_at().time().get().date();
            let start = NaiveDate::from_ymd_opt(2023, 10, 25).unwrap();
            let end = NaiveDate::from_ymd_opt(2023, 11, 5).unwrap();
            (start..end).contains(&date)
        });
        if bef + delta != aft || (opts.within_specific_date && within_specific_date) {
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

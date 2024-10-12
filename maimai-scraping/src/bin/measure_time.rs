use std::{io::Write, path::PathBuf};

use clap::Parser;
use enum_map::EnumMap;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::MaimaiUserData;
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    user_data_paths: Vec<PathBuf>,
    #[arg(long)]
    output_dir: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let mut durations = EnumMap::<bool, Vec<_>>::default();
    for path in opts.user_data_paths {
        let data: MaimaiUserData = read_json(path)?;

        for (within_credit, pair) in data
            .records
            .values()
            .tuple_windows()
            .map(|(x, y)| {
                let within_credit = u8::from(y.played_at().track()) == 1;
                (within_credit, [x, y])
            })
            // Skip while being in the first credit
            .skip_while(|x| x.0)
            // Skip the next interval
            .skip(1)
        {
            // We assume that play time is retrieved in second precision;
            // otherwise we panic (for shorthand)
            let [x, y] = pair.map(|x| x.played_at().idx().timestamp_jst().unwrap().get());
            let duration = (y - x).num_seconds();
            durations[within_credit].push(duration);
            println!("{within_credit}\t{duration}");
        }
    }

    fs_err::create_dir_all(&opts.output_dir)?;
    for (within_credit, mut durations) in durations {
        durations.sort();
        let mut file = File::create(opts.output_dir.join(within_credit.to_string()))?;
        for duration in durations {
            writeln!(file, "{duration}")?;
        }
    }

    Ok(())
}

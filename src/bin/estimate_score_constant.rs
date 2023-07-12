use std::{io::BufReader, path::PathBuf};

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::{
    load_score_level,
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::PlayRecord,
};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    level_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    // TODO: use rank coeffieicnts for appropriate versions
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(opts.input_file)?))?;

    let levels = load_score_level::load(opts.level_file)?;
    println!("{levels:?}");

    return Ok(());
    for (i, record) in records.iter().enumerate() {
        let &delta = record.rating_result().delta();
        let res = ScoreConstant::candidates()
            .filter_map(|score_const| {
                let &achievement_value = record.achievement_result().value();
                let rank_coef = rank_coef(achievement_value);
                let res = single_song_rating(score_const, achievement_value, rank_coef);
                (res.get() as i16 == delta).then(|| {
                    format!(
                        "{:?} x {} ({:?}) x {} = {}",
                        score_const,
                        achievement_value,
                        record.achievement_result().rank(),
                        rank_coef,
                        res
                    )
                })
            })
            .collect_vec();
        println!(
            "{:>2} {} ({:?}) => {:?}",
            i,
            record.song_metadata().name(),
            record.score_metadata().difficulty(),
            res
        );
    }

    Ok(())
}

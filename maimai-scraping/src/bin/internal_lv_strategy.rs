use std::path::PathBuf;

use anyhow::anyhow;
use clap::Parser;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::ScoreConstantsStore, load_score_level, rating::ScoreConstant,
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    old_json: PathBuf,
    new_json: PathBuf,
    level: u8,
    datas: Vec<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let old = load_score_level::load(&opts.old_json)?;
    let old = ScoreConstantsStore::new(&old, &[])?;
    let new = load_score_level::load(&opts.new_json)?;
    let mut new = ScoreConstantsStore::new(&new, &[])?;
    for data in opts.datas {
        let data: MaimaiUserData = read_json(data)?;
        new.do_everything(data.records.values(), &data.rating_targets)?;
    }

    let level = ScoreConstant::try_from(opts.level).map_err(|e| anyhow!("Bad: {e}"))?;
    for (&key, entry) in new.scores() {
        let Ok(Some((song, candidates))) = old.get(key) else {
            continue;
        };
        if candidates == [level] && entry.candidates().len() != 1 {
            println!(
                "{} ({:?} {:?})",
                song.song_name(),
                key.generation,
                key.difficulty,
            );
        }
    }

    Ok(())
}

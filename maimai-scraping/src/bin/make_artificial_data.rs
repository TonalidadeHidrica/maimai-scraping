use std::{io::stdin, path::PathBuf};

use anyhow::{anyhow, bail, Context};
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::{
    chrono_util::jst_now,
    maimai::{
        parser::rating_target::{RatingTargetEntry, RatingTargetList},
        rating::ScoreLevel,
        schema::latest::{ScoreDifficulty, ScoreGeneration, ScoreMetadata},
        MaimaiUserData,
    },
};
use maimai_scraping_utils::fs_json_util::write_json;
use maplit::btreemap;

#[derive(Parser)]
struct Opts {
    rating: u16,
    output_json: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = Opts::parse();

    let mut res: [_; 4] = std::array::from_fn(|_| vec![]);
    for line in stdin().lines() {
        let line = line?; // TODO: why this bind is needed when introducing the if let later?
        let cells = line.split('\t').collect_vec();
        let [i, song_name, achievement, dx, diff, lv] = cells[..] else {
            bail!("Wrong number of rows: {line:?}");
        };
        let i: usize = i.parse().context("Failed to parse i")?;
        let song_name = song_name.parse()?;
        let achievement = achievement
            .parse::<u32>()?
            .try_into()
            .map_err(|e| anyhow!("Failed to parse achievement value: found {e}"))?;
        let generation = match dx {
            "d" => ScoreGeneration::Deluxe,
            "s" => ScoreGeneration::Standard,
            e => bail!("Invalid generation: {e:?}"),
        };
        let difficulty = match diff {
            "b" => ScoreDifficulty::Basic,
            "a" => ScoreDifficulty::Advanced,
            "e" => ScoreDifficulty::Expert,
            "m" => ScoreDifficulty::Master,
            "r" => ScoreDifficulty::ReMaster,
            e => bail!("Invalid difficulty: {e:?}"),
        };
        let level = {
            let lv: u8 = lv.parse()?;
            let (x, y) = (lv / 10, lv % 10);
            let plus = matches!((x, y), (7..=14, 6..));
            ScoreLevel::new(x, plus)?
        };
        let idx = "dummy".to_owned().into();
        res[i].push(
            RatingTargetEntry::builder()
                .score_metadata(
                    ScoreMetadata::builder()
                        .generation(generation)
                        .difficulty(difficulty)
                        .build(),
                )
                .song_name(song_name)
                .level(level)
                .achievement(achievement)
                .idx(idx)
                .build(),
        );
    }
    let date = jst_now().into();
    let targets = {
        let [a, b, c, d] = res;
        RatingTargetList::builder()
            .rating(args.rating.into())
            .target_new(a)
            .target_old(b)
            .candidates_new(c)
            .candidates_old(d)
            .build()
    };
    let data = MaimaiUserData {
        records: Default::default(),
        rating_targets: btreemap![date => targets],
        idx_to_icon_map: Default::default(),
    };
    write_json(args.output_json, &data)?;

    Ok(())
}

use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::{
    schema::latest::{AchievementValue, JudgeCount, JudgeCountWithoutCP, PlaceName},
    MaimaiUserData,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    data_path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let data: MaimaiUserData = read_json(opts.data_path)?;
    for record in data.records.values() {
        let judges = record.judge_result();
        if record.utage_metadata().as_ref().is_some_and(|u| u.buddy()) {
            continue;
        }
        if record.played_at().place() == &Some(PlaceName::from("不明".to_owned())) {
            continue;
        }

        let counts_tuple = |j: JudgeCountWithoutCP| {
            (
                j.perfect() as u64,
                j.great() as u64,
                j.good() as u64,
                j.miss() as u64,
            )
        };
        let (note_score, note_sum) = {
            let (mut score, mut sum) = (0, 0);
            for (weight, j) in [
                (1, judges.tap()),
                (1, judges.touch()),
                (2, judges.hold()),
                (3, judges.slide()),
            ] {
                let (cp, others) = match j {
                    JudgeCount::Nothing => continue,
                    JudgeCount::JudgeCountWithCP(j) => (j.critical_perfect() as u64, j.others()),
                    JudgeCount::JudgeCountWithoutCP(j) => (0, j),
                };
                let (p, g, o, m) = counts_tuple(others);
                let p = p + cp;
                score += (p * 10 + g * 8 + o * 5) * weight;
                sum += (p + g + o + m) * 10 * weight;
            }
            (score, sum)
        };
        let (break_score, break_sum, bonus, bonus_sum, break_perfect, break_great) = {
            let judges = judges.break_();
            let cp = judges.critical_perfect() as u64;
            let (p, g, o, m) = counts_tuple(judges.others());
            let total = cp + p + g + o + m;
            (
                (cp + p) * 50 + g * 25 + o * 20, // + g' * 5 + g'' * 15
                total * 50,
                cp * 20 + p * 10 + g * 8 + o * 6, // + p' * 5
                total * 20,
                p,
                g,
            )
        };
        let mut combinations = 0;
        for p in 0..=break_perfect {
            for gg in 0..=break_great {
                for g in 0..=break_great - gg {
                    let score = note_score + break_score + g * 5 + gg * 15;
                    let note_sum = note_sum + break_sum;
                    let bonus = bonus + p * 5;
                    // (score / note_sum * 100 + bonus / bonus_sum) * 10000
                    let num = (score * 100 * bonus_sum + bonus * note_sum) * 10000;
                    let den = note_sum * bonus_sum;
                    let a: AchievementValue = u32::try_from(num / den)?.try_into().unwrap();
                    if a == record.achievement_result().value() {
                        combinations += 1;
                    }
                }
            }
        }
        match combinations {
            0 => println!("Not found: {record:?}"),
            1 => {}
            _ => {
                // println!("{c} combs were found")
            }
        }
    }

    Ok(())
}

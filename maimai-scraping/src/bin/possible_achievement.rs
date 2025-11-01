use std::{cmp::Reverse, fmt::Display, str::FromStr};

use anyhow::{bail, Context, Result};
use clap::Parser;
use itertools::Itertools;
use lazy_format::lazy_format;
use nalgebra::Vector2 as V;

#[derive(Parser)]
struct Opts {
    profiles: Vec<Profile>,
    #[arg(long, default_value = "10000")]
    cutoff: u64,
}
#[derive(Clone, Copy)]
struct Profile {
    tap: u64,
    hold: u64,
    slide: u64,
    touch: u64,
    break_: u64,
}
impl FromStr for Profile {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        if let Some((tap, touch, hold, slide, break_)) = s
            .split_whitespace()
            .map(|x| x.parse().with_context(|| format!("Invalid value: {x:?}")))
            .collect_tuple()
        {
            Ok(Profile {
                tap: tap?,
                touch: touch?,
                hold: hold?,
                slide: slide?,
                break_: break_?,
            })
        } else {
            bail!("Should contain space-separated five integers, found {s:?}")
        }
    }
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    for p in &opts.profiles {
        let note_count = p.tap + p.hold * 2 + p.slide * 3 + p.touch + p.break_ * 5;
        let note_sum = note_count * 10;
        let break_count = p.break_;
        let break_sum = break_count * 20;
        let deductions = [
            ("tap great", V::new(2, 0), false),
            ("tap good", V::new(5, 0), false),
            ("tap miss", V::new(10, 0), false),
            ("break perfect close", V::new(0, 5), true),
            // ("break perfect far", V::new(0, 10), true),
            ("break great close", V::new(2 * 5, 12), true),
            ("break great mid", V::new(4 * 5, 12), true),
            ("break great far", V::new(5 * 5, 12), true),
            ("break great far", V::new(6 * 5, 14), true),
            ("break miss", V::new(10 * 5, 20), true),
        ];
        let mut mistakes = Vec::new();
        let mut results = Vec::new();
        dfs(
            &deductions,
            opts.cutoff,
            V::new(note_sum, break_sum),
            V::new(note_sum, break_sum),
            V::new(note_count, break_count),
            &mut mistakes,
            &mut results,
        );
        results.sort_by_key(|x| Reverse(x.0));
        for result in results {
            println!("{}: {:?}", show_achievement(result.0), result.1);
        }
    }
    Ok(())
}

fn dfs<'s>(
    deductions: &[(&'s str, V<u64>, bool)],
    cutoff: u64,
    sum: V<u64>,
    remaining_score: V<u64>,
    remaining_count: V<u64>,
    mistakes: &mut Vec<(&'s str, u64)>,
    results: &mut Vec<(u64, Vec<(&'s str, u64)>)>,
) {
    if let Some((&(kind, deduction, is_break), deductions)) = deductions.split_first() {
        let count_delta = if is_break { V::new(0, 1) } else { V::new(1, 0) };
        let count_max = if is_break {
            remaining_count.y
        } else {
            remaining_count.x
        };
        for i in 0..count_max {
            let count = remaining_count - count_delta * i;
            let score = remaining_score - deduction * i;
            let achievement = calc(sum, score);
            if achievement < 101_0000 - cutoff {
                break;
            }
            if i > 0 {
                mistakes.push((kind, i));
            }
            dfs(deductions, cutoff, sum, score, count, mistakes, results);
            if i > 0 {
                mistakes.pop();
            }
        }
    } else {
        let achievement = calc(sum, remaining_score);
        results.push((achievement, mistakes.clone()));
    }
}

fn calc(sum: V<u64>, score: V<u64>) -> u64 {
    // (note_score / note_sum * 100 + break_score / break_sum) * 10000
    let (note_sum, break_sum) = (sum.x, sum.y);
    let (note_score, break_score) = (score.x, score.y);
    (note_score * 100 * break_sum + break_score * note_sum) * 10000 / (note_sum * break_sum)
}

fn show_achievement(achievement: u64) -> impl Display {
    lazy_format!("{}.{:04}%", achievement / 10000, achievement % 10000)
}

use std::{
    convert::TryInto,
    io::{BufReader, BufWriter},
    ops::Range,
    path::PathBuf,
};

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::schema::{
    latest::{PlayRecord, ScoreDifficulty},
    ver_20210316_2338::AchievementValue,
};
use svg::{
    node::element::{Circle, Line, Rectangle},
    Document,
};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
    song_name: String,
    difficulty: ScoreDifficulty,
    output_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let records: Vec<PlayRecord> =
        serde_json::from_reader(BufReader::new(File::open(&opts.input_file)?))?;
    let filtered = records
        .iter()
        .filter(|x| {
            x.song_metadata().name() == &opts.song_name
                && *x.score_metadata().difficulty() == opts.difficulty
        })
        .collect_vec();
    println!("Found {} record(s)", filtered.len());

    let (w, h) = (640.0, 480.0);
    let mut document = Document::new().set("viewBox", (0, 0, w, h));
    let margin = 30.0;
    let x_range = margin..w - margin;
    let x = |i: usize| map_float(i as f64, -1.0..filtered.len() as _, x_range.clone());
    let y_range = h - margin..margin;
    let y = |y: AchievementValue| map_float(y.get() as f64 / 1e4, 75.0..101.0, y_range.clone());
    document = document.add(
        Rectangle::new()
            .set("x", x_range.start)
            .set("y", y_range.end)
            .set("width", x_range.end - x_range.start)
            .set("height", y_range.start - y_range.end)
            .set("stroke", "black")
            .set("fill", "none"),
    );

    for (i, record) in filtered.iter().enumerate() {
        document = document.add(
            Circle::new()
                .set("cx", x(i))
                .set("cy", y(*record.achievement_result().value()))
                .set("r", 3.0)
                .set("fill", "blue"),
        )
    }

    for achi in (75_0000..=101_0000).step_by(1_0000) {
        let y = y(achi.try_into().unwrap());
        let color = if achi == 97_0000 {
            "#aa0000"
        } else if achi % 5_0000 == 0 {
            "#888"
        } else {
            "#bbb"
        };
        document = document.add(
            Line::new()
                .set("x1", x_range.start)
                .set("x2", x_range.end)
                .set("y1", y)
                .set("y2", y)
                .set("stroke", color)
                .set("stroke-width", 0.5),
        )
    }

    for ((i, record_i), (j, record_j)) in filtered.iter().enumerate().tuple_windows() {
        if record_i.played_at().time().date() != record_j.played_at().time().date() {
            let x = (x(i) + x(j)) / 2.;
            document = document.add(
                Line::new()
                    .set("y1", y_range.start)
                    .set("y2", y_range.end)
                    .set("x1", x)
                    .set("x2", x)
                    .set("stroke", "#bbb")
                    .set("stroke-width", 0.5),
            )
        }
    }

    svg::write(BufWriter::new(File::create(&opts.output_file)?), &document)?;

    Ok(())
}

fn map_float(a: f64, src: Range<f64>, dst: Range<f64>) -> f64 {
    dst.start + (dst.end - dst.start) * (a - src.start) / (src.end - src.start)
}

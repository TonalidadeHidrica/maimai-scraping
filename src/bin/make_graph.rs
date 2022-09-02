use std::{
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
use svg::{node::element::Rectangle, Document};

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
    let x = |i: usize| map_float(i as f64, -1.0..records.len() as _, x_range.clone());
    let y_range = h - margin..margin;
    let y = |y: AchievementValue| map_float(y.get() as f64, 90.0..101.0, y_range.clone());
    document = document.add(
        Rectangle::new()
            .set("x", x_range.start)
            .set("y", y_range.end)
            .set("width", x_range.end - x_range.start)
            .set("height", y_range.start - y_range.end)
            .set("stroke", "black")
            .set("fill", "none"),
    );
    svg::write(BufWriter::new(File::create(&opts.output_file)?), &document)?;

    Ok(())
}

fn map_float(a: f64, src: Range<f64>, dst: Range<f64>) -> f64 {
    dst.start + (dst.end - dst.start) * (a - src.start) / (src.end - src.start)
}

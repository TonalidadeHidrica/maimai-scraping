use std::{io::BufWriter, ops::Range, path::PathBuf};

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::AchievementValue,
};
use svg::{node::element::Rectangle, Document};

#[derive(Parser)]
struct Opts {
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let (w, h) = (640.0, 960.0);
    let mut document = Document::new().set("viewBox", (0, 0, w, h));

    let margin = 30.0;
    let x_range = margin..w - margin;
    let x = |x: AchievementValue| map_float(x.get() as f64 / 10000., 90.0..101., x_range.clone());
    let y_range = h - margin..margin;
    let y = |y: u16| map_float(y as f64, 200.0..337., y_range.clone());

    for score_constant in ScoreConstant::candidates() {
        let v = (90_0000..=101_0000)
            .map(|i| {
                let a = AchievementValue::try_from(i).unwrap();
                (a, single_song_rating(score_constant, a, rank_coef(a)))
            })
            .collect_vec();
        for (val, range) in &v.iter().group_by(|x| x.1) {
            let mut range = range.peekable();
            let start = range.peek().unwrap().0;
            let end = AchievementValue::try_from((range.last().unwrap().0.get() + 1).min(101_0000))
                .unwrap();
            if (127..130).contains(&u8::from(score_constant)) && start.get() >= 100_0000 {
                println!("{score_constant}  [{start}, {end}) {val}");
            }
            document = document.add(
                Rectangle::new()
                    .set("x", x(start))
                    .set("y", y(val.get() + 1))
                    .set("width", x(end) - x(start))
                    .set("height", y(val.get()) - y(val.get() + 1))
                    .set(
                        "style",
                        format!(
                            "fill: hsl({}, 50%, 50%)",
                            (u8::from(score_constant) % 10) as f64 * 36.
                        ),
                    )
                    .set("fill-opacity", 0.2),
            );
        }
    }

    svg::write(BufWriter::new(File::create(opts.output)?), &document)?;

    Ok(())
}

fn map_float(a: f64, src: Range<f64>, dst: Range<f64>) -> f64 {
    dst.start + (dst.end - dst.start) * (a - src.start) / (src.end - src.start)
}

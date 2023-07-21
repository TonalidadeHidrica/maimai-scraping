use std::{collections::BTreeMap, io::BufWriter, mem::replace, ops::Range, path::PathBuf};

use clap::Parser;
use fs_err::File;
use itertools::Itertools;
use maimai_scraping::maimai::{
    rating::{rank_coef, single_song_rating, ScoreConstant},
    schema::latest::AchievementValue,
};
use svg::{
    node::{
        element::{Line, Rectangle, Text as TextElement},
        Text as TextNode,
    },
    Document,
};

#[derive(Parser)]
struct Opts {
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let (w, h) = (2560., 1920.0);
    let mut document = Document::new().set("viewBox", (0, 0, w, h));

    let margin = 30.0;
    let x_range = margin..w - margin;
    let x = |x: AchievementValue| map_float(x.get() as f64 / 10000., 80.0..101., x_range.clone());
    let y_range = h - margin..margin;
    let y = |y: u16| map_float(y as f64, 100.0..337., y_range.clone());

    let mut value_drawn = vec![false; 350];
    let mut percent_drawn = vec![BTreeMap::<usize, f64>::new(); 350];

    for score_constant in ScoreConstant::candidates().rev() {
        let v = (80_0000..=101_0000)
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
            let y_bottom = y(val.get());
            let y_top = y(val.get() + 1);
            let y_center = (y_bottom + y_top) / 2.;
            let percent_index = {
                let m = &mut percent_drawn[val.get() as usize];
                // println!("{m:?} {}", x(start));
                m.retain(|_, xs| x(start) <= *xs + 20.0);
                let new = m
                    .keys()
                    .enumerate()
                    .find_map(|(i, &j)| (i < j).then_some(i))
                    .unwrap_or(m.len());
                m.insert(new, x(start));
                new
            };

            let hue = (u8::from(score_constant) % 10) as f64 * 36.;
            document = document
                .add(
                    Rectangle::new()
                        .set("x", x(start))
                        .set("y", y_top)
                        .set("width", x(end) - x(start))
                        .set("height", y_bottom - y_top)
                        .set("style", format!("fill: hsl({hue}, 50%, 50%)"))
                        .set("fill-opacity", 0.2),
                )
                .add(
                    TextElement::new()
                        .add(TextNode::new(start.to_string()))
                        .set("x", x(start))
                        .set("y", y_bottom - percent_index as f64 * 2.)
                        .set("font-size", 3),
                );
            if x(end) - x(start) >= 15. {
                document = document.add(
                    TextElement::new()
                        .add(TextNode::new(score_constant.to_string()))
                        .set("x", (x(start) + x(end)) / 2.)
                        .set("y", y_center)
                        .set("text-anchor", "middle")
                        .set("alignment-baseline", "middle")
                        .set("style", format!("fill: hsl({hue}, 100%, 40%)"))
                        .set("font-size", 6)
                        .set("fill-opacity", 0.2),
                );
            }
            if !replace(&mut value_drawn[val.get() as usize], true) {
                document = document.add(
                    TextElement::new()
                        .add(TextNode::new(val.to_string()))
                        .set("x", x(start))
                        .set("y", y_center)
                        .set("text-anchor", "end")
                        .set("alignment-baseline", "middle")
                        .set("font-size", 6),
                );
            }
            if val.get() % 5 == 0 {
                document = document.add(
                    Line::new()
                        .set("x1", 0.)
                        .set("x2", w)
                        .set("y1", y_center)
                        .set("y2", y_center)
                        .set("stroke", "gray")
                        .set("stroke-width", 0.1),
                );
            }
        }
    }

    svg::write(BufWriter::new(File::create(opts.output)?), &document)?;

    Ok(())
}

fn map_float(a: f64, src: Range<f64>, dst: Range<f64>) -> f64 {
    dst.start + (dst.end - dst.start) * (a - src.start) / (src.end - src.start)
}

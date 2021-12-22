use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use fs_err::read_to_string;
use maimai_scraping::{
    compare_htmls::elements_are_equivalent,
    ongeki::{self, schema::latest::Idx},
    selector,
};
use scraper::{ElementRef, Html};

#[derive(Parser)]
struct Opts {
    input_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let html = Html::parse_document(&read_to_string(&opts.input_file)?);
    let result = ongeki::play_record_parser::parse(&html, Idx::try_from(0)?)?;
    dbg!(&result);

    let html_reconstructed = ongeki::play_record_reconstructor::reconstruct(&result);
    let html_reconstructed = Html::parse_fragment(&html_reconstructed.to_string());
    let html_reconstructed = ElementRef::wrap(
        html_reconstructed
            .root_element()
            .first_child()
            .context("Reconstructed HTML does not have a child")?,
    )
    .context("Reconstructed HTML is not an element")?;
    let html_actual = html
        .select(selector!(".container3"))
        .next()
        .context(".container3 not found")?;
    println!("{}", html_reconstructed.html());
    println!("{}", html_actual.html());
    elements_are_equivalent(html_reconstructed, html_actual)?;
    println!("Horray, these HTMLs are equivalent!");
    Ok(())
}

// I may end up not using this script at all

use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::{parser::play_record, schema::latest::Idx, MaimaiUserData};
use maimai_scraping_utils::fs_json_util::read_json;
use scraper::Html;

#[derive(Parser)]
struct Opts {
    maimai_user_data: PathBuf,
    record_html: PathBuf,
    idx: Idx,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut user_data: MaimaiUserData = read_json(opts.maimai_user_data)?;
    let _records = &mut user_data.records;
    let _record = play_record::parse(
        &Html::parse_document(&fs_err::read_to_string(opts.record_html)?),
        opts.idx,
        true,
    );
    Ok(())
}

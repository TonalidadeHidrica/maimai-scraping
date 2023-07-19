use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{load_targets_from_file, update_targets},
    fs_json_util::write_json,
    maimai::Maimai,
};

#[derive(Parser)]
struct Opts {
    rating_target_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut rating_targets = load_targets_from_file(&opts.rating_target_file)?;
    let (mut client, index) = SegaClient::<Maimai>::new().await?;
    let last_played = index.first().context("There is no play yet.")?.0;
    update_targets(&mut client, &mut rating_targets, last_played).await?;
    write_json(&opts.rating_target_file, &rating_targets)?;
    println!("Successfully saved data to {:?}", opts.rating_target_file);

    Ok(())
}

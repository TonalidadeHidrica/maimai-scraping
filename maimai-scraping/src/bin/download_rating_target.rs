use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{load_data_from_file, update_targets},
    fs_json_util::write_json,
    maimai::Maimai,
};

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut data = load_data_from_file::<Maimai, _>(&opts.maimai_user_data_path)?;
    let (mut client, index) = SegaClient::<Maimai>::new_with_default_path().await?;
    let last_played = index.first().context("There is no play yet.")?.0;
    update_targets(&mut client, &mut data.rating_targets, last_played).await?;
    write_json(&opts.maimai_user_data_path, &data)?;
    println!("Successfully saved data to {:?}", opts.maimai_user_data_path);
    Ok(())
}

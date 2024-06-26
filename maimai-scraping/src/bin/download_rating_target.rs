use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use maimai_scraping::{
    api::SegaClient, cookie_store::UserIdentifier, data_collector::load_or_create_user_data,
    maimai::data_collector::update_targets, maimai::Maimai,
};
use maimai_scraping_utils::fs_json_util::write_json;

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
    #[clap(short, long)]
    force: bool,
    #[clap(flatten)]
    user_identifier: UserIdentifier,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();

    let opts = Opts::parse();
    let mut data = load_or_create_user_data::<Maimai, _>(&opts.maimai_user_data_path)?;
    // This feature always needs Standard Course, so we force payment here
    let (mut client, index) =
        SegaClient::<Maimai>::new_with_default_path(&opts.user_identifier, true).await?;
    let last_played = index.first().context("There is no play yet.")?.0;
    update_targets(
        &mut client,
        &mut data.rating_targets,
        last_played,
        opts.force,
    )
    .await?;
    write_json(&opts.maimai_user_data_path, &data)?;
    println!(
        "Successfully saved data to {:?}",
        opts.maimai_user_data_path
    );
    Ok(())
}

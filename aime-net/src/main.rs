use std::path::PathBuf;

use aime_net::api::AimeApi;
use clap::Parser;
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let (_client, result) = AimeApi::new(opts.cookie_store_path)?
        .login(&read_json(opts.credentials_path)?)
        .await?;
    println!("{result:?}");

    Ok(())
}

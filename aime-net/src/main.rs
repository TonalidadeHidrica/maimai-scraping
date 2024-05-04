use std::path::PathBuf;

use aime_net::api::AimeApi;
use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    #[command(subcommand)]
    sub: Sub,
}

#[derive(Clone, Subcommand)]
enum Sub {
    RemoveByIndex(RemoveByIndex),
}

#[derive(Clone, Args)]
struct RemoveByIndex {
    index: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let (client, result) = AimeApi::new(opts.cookie_store_path)?
        .login(&read_json(opts.credentials_path)?)
        .await?;
    println!("{result:?}");

    match opts.sub {
        Sub::RemoveByIndex(sub) => {
            let slot = result
                .slots()
                .get(sub.index)
                .with_context(|| format!("Index of out bounds: {}", sub.index))?
                .as_ref()
                .context("This slot is empty")?;
            client.remove(slot.block_id()).await?;
        }
    }

    Ok(())
}

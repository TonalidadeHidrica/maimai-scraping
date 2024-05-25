use std::path::PathBuf;

use aime_net::{
    api::AimeApi,
    parser::AimeSlot,
    schema::{AccessCode, CardName},
};
use anyhow::{bail, Context};
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
    Remove(Remove),
    Add(Add),
}

#[derive(Clone, Args)]
struct Remove {
    index: usize,
}

#[derive(Clone, Args)]
struct Add {
    index: usize,
    access_code: AccessCode,
    card_name: CardName,
    #[arg(long)]
    replace: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let (client, result) = AimeApi::new(opts.cookie_store_path)?
        .login(&read_json(opts.credentials_path)?)
        .await?;
    println!("{result:?}");
    let get_slot = |index| {
        result
            .slots()
            .get(index)
            .with_context(|| format!("Index of out bounds: {index}"))
    };

    match opts.sub {
        Sub::Add(sub) => {
            let slot = match get_slot(sub.index)? {
                AimeSlot::Empty(slot) => *slot,
                AimeSlot::Filled(slot) => {
                    if sub.replace {
                        client.remove(slot).await?
                    } else {
                        bail!("The specified slot is not empty");
                    }
                }
            };
            client.add(&slot, sub.access_code, sub.card_name).await?;
        }
        Sub::Remove(sub) => {
            let AimeSlot::Filled(slot) = get_slot(sub.index)? else {
                bail!("The specified slot is empty");
            };
            client.remove(slot).await?;
        }
    }

    Ok(())
}

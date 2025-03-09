use std::path::PathBuf;

use aime_net::schema::AccessCode;
use anyhow::anyhow;
use clap::{Args, Parser, Subcommand};
use deranged::RangedU16;
use itertools::Itertools;

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    pub sub: Sub,
}
#[derive(Subcommand)]
enum Sub {
    Row(Row),
    FromFile(FromFile),
}
#[derive(Args)]
struct Row {
    row: String,
}
#[derive(Args)]
struct FromFile {
    path: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    match opts.sub {
        Sub::Row(sub) => process(&sub.row)?,
        Sub::FromFile(sub) => {
            for line in fs_err::read_to_string(&sub.path)?.lines() {
                process(line)?;
            }
        }
    }

    Ok(())
}

fn process(row: &str) -> anyhow::Result<()> {
    let [index, _, aime, _, friend] = row
        .split('\t')
        .collect_vec()
        .try_into()
        .map_err(|e| anyhow!("Unexpected: {e:?}"))?;
    let index: RangedU16<0, 999> = index.parse()?;
    let index = format!("{index:03}");
    let index_zen = index
        .chars()
        .map(|c| char::from_u32(c as u32 + '０' as u32 - '0' as u32).unwrap())
        .collect::<String>();
    let aime: AccessCode = aime.parse()?;

    println!(
        include_str!("template.toml"),
        aime = aime,
        index = index,
        index_zen = index_zen,
        friend = friend,
    );

    Ok(())
}

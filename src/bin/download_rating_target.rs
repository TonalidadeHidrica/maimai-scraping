use std::{
    cmp::Ordering,
    collections::BTreeMap,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::{bail, Context};
use chrono::NaiveDateTime;
use clap::Parser;
use fs_err::File;
use maimai_scraping::{
    api::SegaClient,
    maimai::{
        rating_target_parser::{self, RatingTargetList},
        Maimai,
    },
};
use scraper::Html;
use url::Url;

#[derive(Parser)]
struct Opts {
    rating_target_file: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let mut rating_targets = load_from_file(&opts.rating_target_file)?;
    let (mut client, index) = SegaClient::<Maimai>::new().await?;

    let last_saved = rating_targets.last_key_value().map(|x| *x.0);
    let last_played = index.last().context("There is no play yet.")?.0;
    if let Some(date) = last_saved {
        println!("Rating target saved at: {date}");
    } else {
        println!("Rating target: not saved");
    }
    println!("Latest play at: {last_played}");
    match last_saved.cmp(&Some(last_played)) {
        Ordering::Less => println!("Updates needed."),
        Ordering::Equal => {
            println!("Already up to date.");
            return Ok(());
        }
        Ordering::Greater => {
            bail!("What?!  Inconsistent newest records between play records and rating targets!")
        }
    };

    let res = client
        .fetch_authenticated(Url::parse(
            "https://maimaidx.jp/maimai-mobile/home/ratingTargetMusic/",
        )?)
        .await?;
    let res = rating_target_parser::parse(&Html::parse_document(&res.0.text().await?))?;
    rating_targets.insert(last_played, res);

    let file = BufWriter::new(File::create(&opts.rating_target_file)?);
    serde_json::to_writer(file, &rating_targets)?;
    println!("Successfully saved data to {:?}", opts.rating_target_file);

    Ok(())
}

fn load_from_file(
    path: impl Into<PathBuf>,
) -> anyhow::Result<BTreeMap<NaiveDateTime, RatingTargetList>> {
    let path = path.into();
    match File::open(&path) {
        Ok(file) => {
            let res = serde_json::from_reader(BufReader::new(file))?;
            println!("Successfully loaded data from {:?}.", &path);
            Ok(res)
        }
        Err(e) => match e.kind() {
            std::io::ErrorKind::NotFound => {
                println!("The file was not found.");
                println!("We will create a new file for you and save the data there.");
                Ok(BTreeMap::new())
            }
            _ => bail!("Unexpected I/O Error: {:?}", e),
        },
    }
}

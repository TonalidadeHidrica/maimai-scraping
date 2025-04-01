use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use google_sheets4::{
    api::ValueRange,
    hyper_rustls, hyper_util,
    yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator},
    Sheets,
};
use itertools::Itertools;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    schema::latest::ScoreGeneration,
    song_list::{database::SongDatabase, Song},
    version::MaimaiVersion,
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database_path: PathBuf,
    key_path: PathBuf,
    spreadsheet_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();

    let songs: Vec<Song> = read_json(opts.database_path)?;
    let database = SongDatabase::new(&songs)?;

    let mut matrix = {
        let size = 150 - 10 + 1;
        vec![vec![vec![]; size]; size]
    };

    let version = MaimaiVersion::latest();
    for song in database.songs() {
        for scores in song.scoreses() {
            let vec = scores
                .all_scores()
                .filter_map(|x| {
                    let score = x.for_version(version)?;
                    let Some(level) = score.level() else {
                        println!("Warning: level not found for {x}");
                        return None;
                    };
                    let level =
                        level
                            .get_if_unique()
                            .or_else(|| match level.candidates().next() {
                                Some(x) => {
                                    println!("Warning: internal level unknown for {x}");
                                    println!("         (falling back to lowest possible)");
                                    Some(x)
                                }
                                None => {
                                    println!("Warning: no internal level candidate for {x} (!!!)");
                                    None
                                }
                            })?;
                    let difficulty = score.score().difficulty();
                    let level_usize = (150 - u8::from(level)) as usize;
                    Some((difficulty, level_usize))
                })
                .collect_vec();
            for &(a, x) in &vec {
                for &(b, y) in &vec {
                    if (x, a) < (y, b) {
                        matrix[y][x].push(scores);
                    }
                }
            }
        }
    }

    // for (x, (i, j)) in matrix
    //     .iter()
    //     .enumerate()
    //     .flat_map(|(i, a)| a.iter().enumerate().map(move |(j, b)| (b.len(), (i, j))))
    //     .sorted_by_key(|&x| std::cmp::Reverse(x))
    // {
    //     let f = |x| (150 - x + 10) as f64 / 10.;
    //     println!("{x} {:.1} {:.1}", f(i), f(j));
    // }

    let table = matrix
        .iter_mut()
        .map(|x| {
            x.iter_mut()
                .map(|x| {
                    x.sort_by_key(|x| x.song().song().pronunciation.values().flatten().last());
                    x.iter()
                        .map(|x| {
                            let name = x.song().latest_song_name();
                            let has_alternative = x.song().scoreses().count() >= 2;
                            let generation = has_alternative.then_some(x.generation());
                            let generation = lazy_format!(match (generation) {
                                Some(ScoreGeneration::Deluxe) => " (DX)",
                                Some(ScoreGeneration::Standard) => " (Std)",
                                None => "",
                            });
                            format!("{name}{generation}")
                        })
                        .join("\n")
                        .into()
                })
                .collect_vec()
        })
        .collect_vec();

    // println!("{:?}", &table[..5]);

    let key = read_service_account_key(opts.key_path).await?;
    let auth = ServiceAccountAuthenticator::builder(key).build().await?;
    let client = hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
        .build(
            hyper_rustls::HttpsConnectorBuilder::new()
                .with_native_roots()
                .unwrap()
                .https_or_http()
                .enable_http1()
                .build(),
        );
    let hub = Sheets::new(client, auth);

    let (_, res) = hub
        .spreadsheets()
        .values_update(
            ValueRange {
                major_dimension: None,
                range: None,
                values: Some(table),
            },
            &opts.spreadsheet_id,
            "B2",
        )
        .value_input_option("RAW")
        .doit()
        .await?;
    println!("{res:?}");

    Ok(())
}

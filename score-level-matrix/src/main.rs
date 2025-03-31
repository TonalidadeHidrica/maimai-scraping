use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use google_sheets4::{
    api::ValueRange,
    hyper_rustls, hyper_util,
    yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator},
    Sheets,
};
use maimai_scraping::maimai::song_list::{database::SongDatabase, Song};
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
                values: Some(vec![
                    vec!["1".into(), "2".into()],
                    vec!["3".into(), "4".into()],
                ]),
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

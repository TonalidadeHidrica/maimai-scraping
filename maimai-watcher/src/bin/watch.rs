use std::{path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use maimai_scraping::{maimai::Maimai, sega_trait::SegaTrait};
use maimai_watcher::watch::{self, TimeoutConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let handle = watch::watch(watch::Config {
        interval: Duration::from_secs(30),
        maimai_uesr_data_path: opts.maimai_uesr_data_path,
        slack_post_webhook: None,
        levels_path: opts.levels_path,
        removed_songs_path: opts.removed_songs_path,
        credentials_path: PathBuf::from(Maimai::CREDENTIALS_PATH),
        cookie_store_path: PathBuf::from(Maimai::COOKIE_STORE_PATH),
        estimate_internal_levels: true,
        timeout_config: TimeoutConfig::indefinite(),
        report_no_updates: false,
    })
    .await?;

    // FIXME dirty workaround
    let (tx, rx) = mpsc::channel();
    ctrlc::set_handler(move || {
        if let Err(e) = tx.send(()) {
            println!("{e}");
        }
    })?;
    let _ = rx.recv();
    handle.stop().await?;
    Ok(())
}

#[derive(Parser)]
struct Opts {
    maimai_uesr_data_path: PathBuf,
    levels_path: PathBuf,
    removed_songs_path: PathBuf,
}

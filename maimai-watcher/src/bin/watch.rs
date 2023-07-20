use std::{path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use maimai_watcher::watch;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let handle = watch::watch(watch::Config {
        interval: Duration::from_secs(30),
        records_path: opts.records_path,
        rating_target_path: opts.rating_target_path,
        slack_post_webhook: None,
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
    records_path: PathBuf,
    rating_target_path: PathBuf,
}

use std::{path::PathBuf, sync::mpsc, time::Duration};

use clap::Parser;
use maimai_scraping::{cookie_store::UserIdentifier, maimai::Maimai, sega_trait::SegaTrait};
use maimai_watcher::watch::{self, ForcePaidConfig, TimeoutConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let handle = watch::watch(watch::Config {
        user_id: "[[[test]]]".into(),
        interval: Duration::from_secs(30),
        maimai_uesr_data_path: opts.maimai_uesr_data_path,
        slack_post_webhook: None,
        credentials_path: PathBuf::from(Maimai::CREDENTIALS_PATH),
        cookie_store_path: PathBuf::from(Maimai::COOKIE_STORE_PATH),
        estimate_internal_levels: true,
        timeout_config: TimeoutConfig::indefinite(),
        report_no_updates: false,
        user_identifier: opts.user_identifier,
        international: opts.international,
        force_paid_config: opts
            .force_paid
            .then_some(ForcePaidConfig { after_use: None }),
        aime_switch_config: None,

        database_path: opts.database_path,
        estimator_config_path: opts.estimator_config_path,

        finish_flag: None,
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
    #[clap(flatten)]
    user_identifier: UserIdentifier,
    #[clap(long)]
    international: bool,
    #[clap(long)]
    force_paid: bool,
    #[clap(long)]
    database_path: Option<PathBuf>,
    #[clap(long)]
    estimator_config_path: Option<PathBuf>,
}

use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Context;
use clap::Parser;
use log::info;
use maimai_watcher::{
    slack_main::{watch_config, Config},
    watch::{self, TimeoutConfig, UserId},
};
use tokio::time::sleep;

#[derive(Parser)]
struct Opts {
    #[arg(long, default_value = "ignore/maimai-watcher-config.toml")]
    config_path: PathBuf,
    user_id: UserId,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();
    let config: Config = toml::from_str(&fs_err::read_to_string(opts.config_path)?)?;
    let user_config = config
        .users
        .get(&opts.user_id)
        .context("Specified user id not found")?;

    let finish_flag = Arc::new(AtomicBool::new(false));
    let config = watch_config(
        opts.user_id,
        &config,
        user_config,
        TimeoutConfig::single(),
        true,
        Some(finish_flag.clone()),
    );
    let _handler = watch::watch(config).await?;
    while !finish_flag.load(std::sync::atomic::Ordering::Acquire) {
        sleep(tokio::time::Duration::from_secs_f64(0.1)).await
    }
    info!("Done!");

    Ok(())
}

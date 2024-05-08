use std::{path::PathBuf, time::Duration};

use aime_net::{
    api::AimeApi,
    schema::{AccessCode, CardName},
};
use anyhow::Context;
use clap::Parser;
use log::info;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::Maimai,
};
use maimai_scraping_utils::fs_json_util::read_json;
use screenshot_generator::generate;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use tokio::time::sleep;

#[derive(Parser)]
struct Opts {
    config_toml: PathBuf,
    #[arg(long)]
    run_tool: bool,
}
#[derive(Deserialize)]
struct Config {
    #[serde(default)]
    remote_debugging_port: Option<u16>,
    img_save_dir: PathBuf,
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    after_use: UserIdentifier,
    aime_cookie_store_path: PathBuf,
    users: Vec<UserConfig>,
}
#[serde_as]
#[derive(Deserialize)]
struct UserConfig {
    folder_name: String,
    user_identifier: UserIdentifier,
    #[serde_as(as = "DisplayFromStr")]
    access_code: AccessCode,
    card_name: CardName,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();
    let config: Config = toml::from_str(&fs_err::read_to_string(&opts.config_toml)?)?;

    for user_config in &config.users {
        run(&opts, &config, user_config)
            .await
            .with_context(|| format!("While saving {:?}", user_config.folder_name))?;
    }

    info!("Switching back the paid account");
    let _ = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &config.credentials_path,
        cookie_store_path: &config.cookie_store_path,
        user_identifier: &config.after_use,
        force_paid: true,
    })
    .await?;

    Ok(())
}

async fn run(opts: &Opts, config: &Config, user_config: &UserConfig) -> anyhow::Result<()> {
    info!("Processing {:?}", user_config.folder_name);

    info!("Selecting Aime");
    let credentials = read_json(&config.credentials_path)?;
    let (api, aimes) = AimeApi::new(config.aime_cookie_store_path.to_owned())?
        .login(&credentials)
        .await?;
    api.overwrite_if_absent(
        &aimes,
        2,
        user_config.access_code,
        user_config.card_name.clone(),
    )
    .await?;
    sleep(Duration::from_secs(3)).await;

    info!("Choosing user & Forcing paid account");
    let (_, records) = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &config.credentials_path,
        cookie_store_path: &config.cookie_store_path,
        user_identifier: &user_config.user_identifier,
        force_paid: true,
    })
    .await?;
    sleep(Duration::from_secs(3)).await;

    info!("Starting brwoser & retrieval");
    generate(
        &config.img_save_dir.join(&user_config.folder_name),
        credentials,
        user_config.user_identifier.clone(),
        Some(records),
        config.remote_debugging_port,
        opts.run_tool,
    )?;
    Ok(())
}

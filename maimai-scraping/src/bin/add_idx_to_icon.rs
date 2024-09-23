use std::{collections::BTreeMap, path::PathBuf};

use clap::Parser;
use log::info;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{data_collector::update_idx, Maimai, MaimaiUserData},
};
use maimai_scraping_utils::fs_json_util::{read_json, read_toml};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    config_path: PathBuf,
}

#[derive(Clone, Deserialize)]
struct Config {
    users: BTreeMap<String, UserConfig>,
}
#[derive(Clone, Deserialize)]
struct UserConfig {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    user_data_path: PathBuf,
    user_identifier: UserIdentifier,
    // #[serde(default)]
    // aime_switch_config: Option<AimeSwitchConfig>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let config: Config = read_toml(opts.config_path)?;

    for (user, config) in config.users {
        info!("Processing {user:?}");
        let mut data: MaimaiUserData = read_json(config.user_data_path)?;

        let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
            credentials_path: &config.credentials_path,
            cookie_store_path: &config.cookie_store_path,
            user_identifier: &config.user_identifier,
            // There is no need to be Standard member to parse history page
            force_paid: false,
        })
        .await?;

        for rating_targets in data.rating_targets.values() {
            update_idx(&mut client, rating_targets, &mut data.idx_to_icon_map).await?;
        }
    }

    Ok(())
}

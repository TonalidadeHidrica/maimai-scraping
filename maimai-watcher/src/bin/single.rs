use std::path::PathBuf;

use clap::Parser;
use maimai_watcher::slack_main::slash_command;

#[derive(Parser)]
struct Opts {
    #[arg(long, default_value = "ignore/maimai-watcher-config.toml")]
    config_path: PathBuf,
    #[clap(flatten)]
    single: slash_command::Single,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let (user_id, user_config) = get_user_id(&state, &info, &sub_args.user_id)?;
    let config = watch_config(
        user_id.clone(),
        &state.config,
        user_config,
        TimeoutConfig::single(),
        true,
    );
    watch::watch(config).await?;

    Ok(())
}

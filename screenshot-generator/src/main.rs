use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::{cookie_store::UserIdentifier, fs_json_util::read_json};
use screenshot_generator::generate;

#[derive(Parser)]
struct Opts {
    img_save_dir: PathBuf,
    credentials_path: PathBuf,
    #[clap(flatten)]
    user_identifier: UserIdentifier,
    #[clap(long)]
    remote_debugging_port: Option<u16>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let credentials = read_json(opts.credentials_path)?;
    generate(
        &opts.img_save_dir,
        credentials,
        opts.user_identifier,
        opts.remote_debugging_port,
    )?;
    Ok(())
}

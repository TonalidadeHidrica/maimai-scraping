#[tokio::main]
async fn main() -> anyhow::Result<()> {
    maimai_watcher::slack_main::main().await
}

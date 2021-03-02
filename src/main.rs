use maimai_scraping::api::download_page;
use maimai_scraping::api::reqwest_client;
use maimai_scraping::cookie_store::CookieStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cookie_store = CookieStore::load()?;
    let client = reqwest_client()?;
    let result = download_page(&client, &mut cookie_store, 0).await?;
    dbg!(&result);

    Ok(())
}

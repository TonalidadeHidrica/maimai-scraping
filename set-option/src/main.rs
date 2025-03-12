use std::{path::PathBuf, str::FromStr, time::Duration};

use aime_net::{api::AimeApi, schema::AccessCode};
use anyhow::{bail, Context, Result};
use clap::Parser;
use fs_err::read_to_string;
use linked_hash_map::LinkedHashMap;
use log::{debug, error, info};
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::{FriendCode, PlayerName, UserIdentifier},
    maimai::Maimai,
};
use maimai_scraping_utils::{fs_json_util::read_json, selector};
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use scraper::Html;
use tokio::time::sleep;
use url::Url;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    aime_cookie_store_path: PathBuf,
    aime_list: PathBuf,
    #[clap(flatten)]
    after_use: UserIdentifier,

    overwrites: Vec<Overwrite>,
}

#[derive(Clone)]
struct Overwrite(String, String);
impl FromStr for Overwrite {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (x, y) = s
            .split_once('=')
            .with_context(|| format!("Missing equal sign (`=`): {s:?}"))?;
        Ok(Self(x.into(), y.into()))
    }
}

#[derive(Debug)]
struct Account {
    access_code: AccessCode,
    friend_code: FriendCode,
    player_name: PlayerName,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let accounts = read_to_string(&opts.aime_list)?
        .lines()
        .map(|x| match x.split('\t').collect::<Vec<_>>()[..] {
            [_index, _kind, access_code, _count, friend_code, player_name] => Ok(Account {
                access_code: access_code.parse()?,
                friend_code: friend_code.to_owned().into(),
                player_name: player_name.to_owned().into(),
            }),
            ref row => bail!("Invalid row: {row:?}"),
        })
        .collect::<Result<Vec<_>>>()?;

    let mut errors = vec![];
    for account in &accounts {
        let res = run(&opts, account)
            .await
            .with_context(|| format!("While saving {}", account.player_name));
        if let Err(e) = res {
            error!("{e:#}");
            errors.push(account);
        }
    }

    info!("Switching back the paid account");
    let _ = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &opts.credentials_path,
        cookie_store_path: &opts.cookie_store_path,
        user_identifier: &opts.after_use,
        force_paid: true,
    })
    .await?;

    if !errors.is_empty() {
        bail!("The following card failed: {errors:?}");
    }

    Ok(())
}

async fn run(opts: &Opts, account: &Account) -> Result<()> {
    info!("Processing {}", account.player_name);

    // TODO basically copied from generate-screenshot
    info!("Selecting Aime");
    {
        let credentials = read_json(&opts.credentials_path)?;
        let (api, aimes) = AimeApi::new(opts.aime_cookie_store_path.to_owned())?
            .login(&credentials)
            .await?;
        api.overwrite_if_absent(
            &aimes,
            2,
            account.access_code,
            String::from(account.player_name.clone()).into(),
        )
        .await?;
        sleep(Duration::from_secs(3)).await;
    }

    info!("Choosing user & Forcing paid account");
    let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
        credentials_path: &opts.credentials_path,
        cookie_store_path: &opts.cookie_store_path,
        user_identifier: &UserIdentifier {
            friend_code: Some(account.friend_code.clone()),
            player_name: Some(account.player_name.clone()),
        },
        force_paid: true,
    })
    .await?;
    sleep(Duration::from_secs(3)).await;

    let response = client
        .fetch_authenticated("https://maimaidx.jp/maimai-mobile/home/userOption/updateUserOption/");
    let html = Html::parse_document(&response.await?.0.text().await?);
    let mut values = html
        .select(selector!("form select"))
        .map(|x| {
            let name = x
                .attr("name")
                .with_context(|| format!("Missing name: {}", x.html()))?;
            let value = x
                .select(selector!("option[selected]"))
                .next()
                .with_context(|| format!("Missing selected: {}", x.html()))?
                .attr("value")
                .with_context(|| format!("Missing value: {}", x.html()))?;
            anyhow::Ok((name, value))
        })
        .collect::<Result<LinkedHashMap<_, _>>>()?;
    let token = html
        .select(selector!(r#"input[name="token"]"#))
        .next()
        .context("Token input not found")?
        .attr("value")
        .context("Token input does not have value")?;
    values.insert("token", token);
    for Overwrite(key, new_value) in &opts.overwrites {
        *values
            .get_mut(&key[..])
            .with_context(|| format!("Key not found: {key:?}"))? = new_value;
    }
    let (_, _location) = client
        .request_authenticated(
            |client| {
                let url =
                    "https://maimaidx.jp/maimai-mobile/home/userOption/updateUserOption/update/";
                let body = serde_html_form::to_string(values)?;
                debug!("Body: {body}");
                Ok(client
                    .post(url)
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/x-www-form-urlencoded"),
                    )
                    .body(body))
            },
            &format!("; _t={token}"),
        )
        .await?;
    // let expected_url = Url::parse("https://maimaidx.jp/maimai-mobile/home/userOption/").unwrap();
    // if location != Some(expected_url) {
    //     bail!("Unexpected redirect to {location:?}");
    // }

    info!("Operation succeeded");

    Ok(())
}

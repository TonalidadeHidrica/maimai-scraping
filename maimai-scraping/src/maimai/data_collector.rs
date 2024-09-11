use std::cmp::Ordering;

use crate::{
    api::SegaClient,
    chrono_util::jst_now,
    maimai::{
        parser::{self, rating_target::RatingTargetFile},
        schema::latest::PlayTime,
        Maimai,
    },
};
use anyhow::{bail, Context};
use chrono::Timelike;
use hashbrown::HashMap;
use log::{info, warn};
use scraper::Html;
use url::Url;

use super::{
    parser::{rating_target::RatingTargetList, song_score::ScoreIdx},
    schema::latest::SongIcon,
};

pub const RATING_TARGET_URL: &str = "https://maimaidx.jp/maimai-mobile/home/ratingTargetMusic/";

pub async fn update_targets(
    client: &mut SegaClient<'_, Maimai>,
    rating_targets: &mut RatingTargetFile,
    last_played: PlayTime,
    force: bool,
) -> anyhow::Result<()> {
    let last_played = last_played
        .get()
        .with_second(0)
        .with_context(|| format!("The time {last_played:?} cannot have seconds 0"))?
        .with_nanosecond(0)
        .with_context(|| format!("The time {last_played:?} cannot have nanoseconds 0"))?
        .into();
    let last_saved = rating_targets.last_key_value().map(|x| *x.0);
    if let Some(date) = last_saved {
        info!("Rating target saved at: {date}");
    } else {
        info!("Rating target: not saved");
    }
    info!("Latest play at: {last_played}");
    let key_to_store = match last_saved.cmp(&Some(last_played)) {
        _ if force => {
            warn!("Retrieving data regardless of the ordering between the last saved and played times.");
            PlayTime::from(jst_now())
        }
        Ordering::Less => {
            info!("Updates needed.");
            last_played
        }
        Ordering::Equal => {
            info!("Already up to date.");
            return Ok(());
        }
        Ordering::Greater => {
            bail!("What?!  Inconsistent newest records between play records and rating targets!");
        }
    };

    let res = client
        .fetch_authenticated(Url::parse(RATING_TARGET_URL)?)
        .await?;
    let res = parser::rating_target::parse(&Html::parse_document(&res.0.text().await?))?;
    rating_targets.insert(key_to_store, res);
    Ok(())
}

pub async fn update_idx(
    client: &mut SegaClient<'_, Maimai>,
    rating_target: &RatingTargetList,
    map: &mut HashMap<ScoreIdx, SongIcon>,
) -> anyhow::Result<()> {
    // TODO: For now, we assume that only "Link" is problematic
    for entry in [
        rating_target.target_new(),
        rating_target.target_old(),
        rating_target.candidates_new(),
        rating_target.candidates_old(),
    ]
    .into_iter()
    .flatten()
    .filter(|v| AsRef::<str>::as_ref(v.song_name()) == "Link")
    {
        let idx = entry.idx();
        let res = client
            .fetch_authenticated(Url::parse(&format!(
                "https://maimaidx.jp/maimai-mobile/record/musicDetail/?idx={idx}"
            ))?)
            .await?;
        let res = parser::music_detail::parse(&Html::parse_document(&res.0.text().await?))?;
        let icon = res.icon();
        info!("{idx:?} was associated to {icon:?}");
        map.insert(idx.clone(), res.icon().clone());
    }
    Ok(())
}

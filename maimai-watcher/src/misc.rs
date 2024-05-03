use std::path::PathBuf;

use anyhow::bail;
use joinery::JoinableIterator;
use log::error;
use maimai_scraping::{
    data_collector::load_or_create_user_data,
    maimai::{
        estimate_rating::{EstimatorConfig, ScoreConstantsStore},
        load_score_level::{self, RemovedSong},
        Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::read_json;
use url::Url;

use crate::{
    describe_record::{get_song_lvs, make_message},
    slack::webhook_send,
    watch::UserId,
};

#[allow(clippy::too_many_arguments)]
pub async fn recent(
    client: &reqwest::Client,
    slack_post_webhook: &Option<Url>,
    user_id: &UserId,
    user_data_path: &PathBuf,
    levels_path: &PathBuf,
    removed_songs_path: &PathBuf,
    estimator_config: EstimatorConfig,
    count: usize,
) -> Result<(), anyhow::Error> {
    let data = load_or_create_user_data::<Maimai, _>(user_data_path)?;
    let levels = load_score_level::load(levels_path)?;
    let removed_songs: Vec<RemovedSong> = read_json(removed_songs_path)?;
    let mut levels = ScoreConstantsStore::new(&levels, &removed_songs)?;
    if count > 10 {
        bail!("Too many songs are requested!  (This is a safety guard to avoid a flood of message.  Please contact the author if you want more.)");
    }
    if let Err(e) = levels.do_everything(
        estimator_config,
        None,
        data.records.values(),
        &data.rating_targets,
    ) {
        error!("{e:#}");
    }
    let message = (data.records.values().rev().take(count).rev())
        .map(|record| make_message(record, get_song_lvs(record, &levels)))
        .join_with("\n");
    webhook_send(
        client,
        slack_post_webhook,
        Some(user_id),
        message.to_string(),
    )
    .await;
    Ok(())
}

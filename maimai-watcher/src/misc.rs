use std::path::PathBuf;

use anyhow::bail;
use joinery::JoinableIterator;
use maimai_scraping::{
    data_collector::load_or_create_user_data,
    maimai::{
        associated_user_data,
        internal_lv_estimator::{
            multi_user::{self, MultiUserEstimator},
            Estimator,
        },
        rating::InternalScoreLevel,
        song_list::{
            database::{ScoreForVersionRef, SongDatabase},
            Song,
        },
        version::MaimaiVersion,
        Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::{read_json, read_toml};
use url::Url;

use crate::{describe_record::make_message, slack::webhook_send, watch::UserId};

#[allow(clippy::too_many_arguments)]
pub async fn recent(
    client: &reqwest::Client,
    slack_post_webhook: &Option<Url>,
    user_id: &UserId,
    user_data_path: &PathBuf,
    database_path: Option<&PathBuf>,
    estimator_config_path: Option<&PathBuf>,
    count: usize,
) -> Result<(), anyhow::Error> {
    if count > 100 {
        bail!("Too many songs are requested!  (This is a safety guard to avoid a flood of message.  Please contact the author if you want more.)");
    }

    let songs: Option<Vec<Song>> = database_path.map(read_json).transpose()?;
    let database = songs
        .as_ref()
        .map(|songs| SongDatabase::new(songs))
        .transpose()?;
    let mut estimator = database
        .as_ref()
        .map(|database| Estimator::new(database, MaimaiVersion::latest()))
        .transpose()?;
    let estimator_config = estimator_config_path
        .map(read_toml::<_, multi_user::Config>)
        .transpose()?;
    if let Some(((database, estimator_config), estimator)) = (database.as_ref())
        .zip(estimator_config.as_ref())
        .zip(estimator.as_mut())
    {
        multi_user::update_all(database, &estimator_config.read_all()?, estimator)?;
    }

    let data = load_or_create_user_data::<Maimai, _>(user_data_path)?;
    let associated = database
        .map(|database| associated_user_data::UserData::annotate(&database, &data))
        .transpose()?;

    let message = match associated {
        Some(records) => (records.records().values().rev().take(count).rev())
            .map(|record| {
                let level = try_get_level(estimator.as_ref(), Some(record));
                make_message(record.record(), Some(record), level)
            })
            .join_with("\n")
            .to_string(),
        None => (data.records.values().rev().take(count).rev())
            .map(|record| make_message(record, None, None))
            .join_with("\n")
            .to_string(),
    };
    webhook_send(
        client,
        slack_post_webhook,
        Some(user_id),
        message.to_string(),
    )
    .await;
    Ok(())
}

pub fn try_get_level<'s, 'e: 's, 'n>(
    estimator: Option<&'e MultiUserEstimator<'s, 'n>>,
    associated: Option<&associated_user_data::PlayRecord<'_, 's>>,
) -> Option<InternalScoreLevel> {
    let score = match associated?.score().ok()? {
        ScoreForVersionRef::Ordinary(score) => score,
        ScoreForVersionRef::Utage(_) => return None,
    };
    Some(estimator?.get(score.score())?.candidates())
}

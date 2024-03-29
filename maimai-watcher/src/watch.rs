use std::{
    iter::successors,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Context;
use log::error;
use maimai_scraping::{
    api::SegaClient,
    cookie_store::UserIdentifier,
    data_collector::{load_or_create_user_data, update_records},
    fs_json_util::{read_json, write_json},
    maimai::{
        data_collector::update_targets,
        estimate_rating::{EstimatorConfig, ScoreConstantsStore},
        load_score_level::{self, RemovedSong, Song},
        Maimai, MaimaiUserData,
    },
};
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
    time::sleep,
};
use url::Url;

use crate::{
    describe_record::{describe_score_kind, get_song_lvs, make_message},
    slack::webhook_send,
};

// TODO use netype instead of alias!
// #[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
// struct UserId(String);
pub type UserId = String;

#[derive(Debug)]
pub struct Config {
    pub user_id: UserId,
    pub interval: Duration,
    pub credentials_path: PathBuf,
    pub cookie_store_path: PathBuf,
    pub maimai_uesr_data_path: PathBuf,
    pub levels_path: PathBuf,
    pub removed_songs_path: PathBuf,
    pub slack_post_webhook: Option<Url>,
    pub estimate_internal_levels: bool,
    pub timeout_config: TimeoutConfig,
    pub report_no_updates: bool,
    pub estimator_config: EstimatorConfig,
    pub user_identifier: UserIdentifier,
}

#[derive(Debug)]
pub struct TimeoutConfig {
    max_count: usize,
    max_duration: Duration,
}
impl TimeoutConfig {
    pub fn single() -> Self {
        Self {
            max_count: 1,
            ..Self::indefinite()
        }
    }
    pub fn hours(hours: f64) -> Self {
        Self {
            max_duration: Duration::from_secs_f64(hours * 3600.),
            ..Self::indefinite()
        }
    }
    pub fn indefinite() -> Self {
        Self {
            max_count: usize::max_value(),
            max_duration: Duration::MAX,
        }
    }
}

pub async fn watch(config: Config) -> anyhow::Result<WatchHandler> {
    let (tx, mut rx) = mpsc::channel(100);

    let data = load_or_create_user_data::<Maimai, _>(&config.maimai_uesr_data_path)?;

    let levels = load_score_level::load(&config.levels_path)?;
    let removed_songs: Vec<RemovedSong> = read_json(&config.removed_songs_path)?;

    spawn(async move {
        let Ok(mut runner) = report_error(
            &config.slack_post_webhook,
            &config.user_id,
            Runner::new(&config, data, &levels, &removed_songs)
                .await
                .context("Issue in levels or removed_songs"),
        )
        .await
        else {
            return;
        };

        let mut last_update_time = Instant::now();
        let mut count = 0;
        'outer: while let Err(TryRecvError::Empty | TryRecvError::Disconnected) = rx.try_recv() {
            match runner.run().await {
                Err(e) => {
                    error!("{e:#}");
                    webhook_send(
                        &reqwest::Client::new(),
                        &config.slack_post_webhook,
                        &config.user_id,
                        format!("{e:#}"),
                    )
                    .await;
                }
                Ok(updates) => {
                    if updates {
                        last_update_time = Instant::now();
                    } else if config.report_no_updates {
                        webhook_send(
                            &reqwest::Client::new(),
                            &config.slack_post_webhook,
                            &config.user_id,
                            "Already up to date.",
                        )
                        .await;
                    }
                }
            }
            let chunk = Duration::from_millis(250);
            for remaining in successors(Some(config.interval), |x| x.checked_sub(chunk)) {
                sleep(remaining.min(chunk)).await;
                if !matches!(rx.try_recv(), Err(TryRecvError::Empty)) {
                    break 'outer;
                }
            }

            count += 1;
            if count >= config.timeout_config.max_count {
                break;
            } else if (Instant::now() - last_update_time) >= config.timeout_config.max_duration {
                webhook_send(
                    &reqwest::Client::new(),
                    &config.slack_post_webhook,
                    &config.user_id,
                    "There have been no updates for a while.  Stopping automatically.".to_string(),
                )
                .await;
                break;
            }
        }
    });
    Ok(WatchHandler(tx))
}

struct Runner<'c, 's> {
    config: &'c Config,
    data: MaimaiUserData,
    levels_actual: ScoreConstantsStore<'s>,
    levels_naive: ScoreConstantsStore<'s>,
}
impl<'c, 's> Runner<'c, 's> {
    async fn new(
        config: &'c Config,
        data: MaimaiUserData,
        levels: &'s [Song],
        removed_songs: &'s [RemovedSong],
    ) -> anyhow::Result<Runner<'c, 's>> {
        let levels_actual = ScoreConstantsStore::new(levels, removed_songs)?;
        let levels_naive = ScoreConstantsStore::new(levels, removed_songs)?;
        let mut ret = Self {
            config,
            data,
            levels_actual,
            levels_naive,
        };
        ret.update_levels().await;
        Ok(ret)
    }

    async fn update_levels(&mut self) {
        if !self.config.estimate_internal_levels {
            return;
        }
        let _ = report_error(
            &self.config.slack_post_webhook,
            &self.config.user_id,
            self.levels_actual
                .do_everything(
                    self.config.estimator_config,
                    self.data.records.values(),
                    &self.data.rating_targets,
                )
                .context("While estimating levels precisely"),
        )
        .await;
        let _ = report_error(
            &self.config.slack_post_webhook,
            &self.config.user_id,
            self.levels_naive
                .guess_from_rating_target_order(&self.data.rating_targets)
                .context("While estimating levels roughly"),
        )
        .await;
    }

    async fn run(&mut self) -> anyhow::Result<bool> {
        let config = self.config;
        let (mut client, index) = SegaClient::<Maimai>::new(
            &self.config.credentials_path,
            &self.config.cookie_store_path,
            &self.config.user_identifier,
        )
        .await?;
        let last_played = index.first().context("There is no play yet.")?.0;
        let inserted_records = update_records(&mut client, &mut self.data.records, index).await?;
        if inserted_records.is_empty() {
            return Ok(false);
        }
        write_json(&config.maimai_uesr_data_path, &self.data)?; // Save twice just in case
        let update_targets_res = update_targets(
            &mut client,
            &mut self.data.rating_targets,
            last_played,
            false,
        )
        .await
        .context("Rating target not available");
        let update_targets_res = report_error(
            &config.slack_post_webhook,
            &config.user_id,
            update_targets_res,
        )
        .await;
        if update_targets_res.is_ok() {
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                &config.user_id,
                "Rating target updated",
            )
            .await;
        }
        write_json(&config.maimai_uesr_data_path, &self.data)?; // Save twice just in case

        let bef_len = self.levels_actual.events().len();
        self.update_levels().await;

        for time in inserted_records {
            let record = &self.data.records[&time];
            let song_lvs = get_song_lvs(record, &self.levels_naive);
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                &config.user_id,
                make_message(record, song_lvs).to_string(),
            )
            .await;
        }

        for (key, event) in &self.levels_actual.events()[bef_len..] {
            let song_name = if let Ok(Some((song, _))) = self.levels_actual.get(*key) {
                song.song_name().as_ref()
            } else {
                "(Error: unknown song name)"
            };
            let score_kind = describe_score_kind(key.score_metadata());
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                &config.user_id,
                format! {"★ {song_name} ({score_kind}): {event}"},
            )
            .await;
        }

        Ok(true)
    }
}

async fn report_error<T>(
    url: &Option<Url>,
    user_id: &UserId,
    result: anyhow::Result<T>,
) -> anyhow::Result<T> {
    if let Err(e) = &result {
        error!("{e:#}");
        webhook_send(&reqwest::Client::new(), url, user_id, format!("{e:#}")).await;
    }
    result
}

pub struct WatchHandler(mpsc::Sender<()>);
impl WatchHandler {
    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(()).await
    }

    pub fn is_dropped(&self) -> bool {
        self.0.is_closed()
    }
}

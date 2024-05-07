use std::{
    fmt::{Debug, Display},
    iter::successors,
    path::PathBuf,
    time::{Duration, Instant},
};

use aime_net::{
    api::AimeApi,
    parser::AimeSlot,
    schema::{AccessCode, CardName},
};
use anyhow::Context;
use log::{error, info, warn};
use maimai_scraping::{
    api::{SegaClient, SegaClientAndRecordList, SegaClientInitializer},
    cookie_store::UserIdentifier,
    data_collector::{load_or_create_user_data, update_records},
    maimai::{
        data_collector::update_targets,
        estimate_rating::{EstimatorConfig, ScoreConstantsStore},
        load_score_level::{self, MaimaiVersion, RemovedSong, Song},
        parser::rating_target::RatingTargetFile,
        schema::latest::{PlayRecord, PlayTime},
        Maimai, MaimaiIntl, MaimaiUserData,
    },
    sega_trait::{self, Idx, PlayRecordTrait, PlayedAt, SegaTrait},
};
use maimai_scraping_utils::fs_json_util::{read_json, write_json};
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
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
    pub international: bool,
    pub force_paid_config: Option<ForcePaidConfig>,
    pub aime_switch_config: Option<AimeSwitchConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ForcePaidConfig {
    pub after_use: Option<UserIdentifier>,
}
#[serde_as]
#[derive(Clone, Debug, Deserialize)]
pub struct AimeSwitchConfig {
    pub slot_index: usize,
    #[serde_as(as = "DisplayFromStr")]
    pub access_code: AccessCode,
    pub card_name: CardName,
    pub cookie_store_path: PathBuf,
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
            let run = if config.international {
                runner.run::<MaimaiIntl>().await
            } else {
                runner.run::<Maimai>().await
            };
            match run {
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

        if let Some(force_paid) = config.force_paid_config {
            if config.international {
                error!("There is no paid course for maimai interantional!  Skipping the swithcing back process.");
            } else if let Some(after_use) = force_paid.after_use {
                let init = SegaClientInitializer {
                    credentials_path: &config.credentials_path,
                    cookie_store_path: &config.cookie_store_path,
                    user_identifier: &after_use,
                    force_paid: true,
                };
                match Maimai::new_client(init).await {
                    Ok(_) => {
                        webhook_send(
                            &reqwest::Client::new(),
                            &config.slack_post_webhook,
                            &config.user_id,
                            "Standard course has been given back to the original account.",
                        )
                        .await;
                    }
                    Err(e) => {
                        let e = e.context("Failed to switch back the paid account");
                        error!("{e:#}");
                        webhook_send(
                            &reqwest::Client::new(),
                            &config.slack_post_webhook,
                            &config.user_id,
                            format!("{e:#}"),
                        )
                        .await;
                    }
                }
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
                    None,
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
                .guess_from_rating_target_order(
                    MaimaiVersion::latest(),
                    false,
                    None,
                    &self.data.rating_targets,
                )
                .context("While estimating levels roughly"),
        )
        .await;
    }

    async fn run<T>(&mut self) -> anyhow::Result<bool>
    where
        T: MaimaiPossiblyIntl,
    {
        let config = self.config;

        // Select aime if specified
        if let Some(aime) = &config.aime_switch_config {
            let credentials = read_json(&config.credentials_path)?;
            let (api, aimes) = AimeApi::new(aime.cookie_store_path.to_owned())?
                .login(&credentials)
                .await?;
            let slot = match &aimes.slots()[aime.slot_index] {
                AimeSlot::Filled(filled) => api.remove(filled).await?,
                AimeSlot::Empty(empty) => *empty,
            };
            sleep(Duration::from_secs(1)).await;
            api.add(&slot, aime.access_code, "".to_owned().into())
                .await?;
            sleep(Duration::from_secs(1)).await;
            info!("Switched aime.")
        }

        let (force_paid, warn) = T::force_paid(config.force_paid_config.is_some());
        if warn {
            warn!("There is no Standard Course for Maimai International!");
            webhook_send(
                &reqwest::Client::new(),
                &config.slack_post_webhook,
                &config.user_id,
                "There is no Standard Course for Maimai International!",
            )
            .await;
        }
        let init = SegaClientInitializer {
            credentials_path: &self.config.credentials_path,
            cookie_store_path: &self.config.cookie_store_path,
            user_identifier: &self.config.user_identifier,
            force_paid,
        };
        let (mut client, index) = T::new_client(init).await?;
        let last_played = index.first().context("There is no play yet.")?.0;
        let inserted_records = update_records(&mut client, &mut self.data.records, index).await?;
        if inserted_records.is_empty() {
            return Ok(false);
        }
        write_json(&config.maimai_uesr_data_path, &self.data)?; // Save twice just in case
        let update_targets_res = T::update_targets(
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

trait MaimaiPossiblyIntl
where
    Self: SegaTrait<PlayRecord = PlayRecord>,
    // Self::UserData: SegaUserData<Maimai>,
    Idx<Self>: Copy + PartialEq + Display,
    sega_trait::PlayTime<Self>: Copy + Ord + Display,
    PlayedAt<Self>: Debug,
    <Self as SegaTrait>::PlayRecord: PlayRecordTrait<PlayTime = PlayTime>,
{
    fn force_paid(force_paid: bool) -> (Self::ForcePaidFlag, bool);

    async fn new_client<'p>(
        init: SegaClientInitializer<'p, '_, Self>,
    ) -> anyhow::Result<SegaClientAndRecordList<'p, Self>>;

    async fn update_targets(
        client: &mut SegaClient<'_, Self>,
        rating_targets: &mut RatingTargetFile,
        last_played: PlayTime,
        force: bool,
    ) -> anyhow::Result<()>;
}

impl MaimaiPossiblyIntl for Maimai {
    fn force_paid(force_paid: bool) -> (bool, bool) {
        (force_paid, false)
    }

    async fn new_client<'p>(
        init: SegaClientInitializer<'p, '_, Self>,
    ) -> anyhow::Result<SegaClientAndRecordList<'p, Self>> {
        SegaClient::<Maimai>::new(init).await
    }

    async fn update_targets(
        client: &mut SegaClient<'_, Self>,
        rating_targets: &mut RatingTargetFile,
        last_played: PlayTime,
        force: bool,
    ) -> anyhow::Result<()> {
        update_targets(client, rating_targets, last_played, force).await
    }
}

impl MaimaiPossiblyIntl for MaimaiIntl {
    fn force_paid(force_paid: bool) -> ((), bool) {
        ((), !force_paid)
    }

    async fn new_client<'p>(
        init: SegaClientInitializer<'p, '_, Self>,
    ) -> anyhow::Result<SegaClientAndRecordList<'p, Self>> {
        SegaClient::new_maimai_intl(init).await
    }

    async fn update_targets(
        _client: &mut SegaClient<'_, Self>,
        _rating_targets: &mut RatingTargetFile,
        _last_played: PlayTime,
        _force: bool,
    ) -> anyhow::Result<()> {
        info!("Maimai international has rating target available!");
        Ok(())
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

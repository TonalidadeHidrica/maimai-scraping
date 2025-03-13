use std::{
    fmt::{Debug, Display},
    iter::successors,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use aime_net::{
    api::AimeApi,
    schema::{AccessCode, CardName},
};
use anyhow::Context;
use hashbrown::HashMap;
use log::{error, info, warn};
use maimai_scraping::{
    api::{SegaClient, SegaClientAndRecordList, SegaClientInitializer},
    cookie_store::UserIdentifier,
    data_collector::{load_or_create_user_data, update_records},
    maimai::{
        associated_user_data,
        data_collector::update_targets,
        internal_lv_estimator::{
            multi_user::{self, MultiUserEstimator},
            Estimator,
        },
        parser::{
            rating_target::{RatingTargetFile, RatingTargetList},
            song_score::ScoreIdx,
        },
        schema::latest::{PlayRecord, PlayTime, SongIcon},
        song_list::{database::SongDatabase, Song},
        version::MaimaiVersion,
        Maimai, MaimaiIntl, MaimaiUserData,
    },
    sega_trait::{self, Idx, PlayRecordTrait, PlayedAt, SegaTrait},
};
use maimai_scraping_utils::fs_json_util::{read_json, read_toml, write_json};
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
    time::sleep,
};
use url::Url;

use crate::{describe_record::make_message, misc::try_get_level, slack::webhook_send};

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
    pub slack_post_webhook: Option<Url>,
    pub estimate_internal_levels: bool,
    pub timeout_config: TimeoutConfig,
    pub report_no_updates: bool,
    pub user_identifier: UserIdentifier,
    pub international: bool,
    pub force_paid_config: Option<ForcePaidConfig>,
    pub aime_switch_config: Option<AimeSwitchConfig>,

    pub database_path: Option<PathBuf>,
    pub estimator_config_path: Option<PathBuf>,

    pub finish_flag: Option<Arc<AtomicBool>>,
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
            max_count: usize::MAX,
            max_duration: Duration::MAX,
        }
    }
}

pub async fn watch(config: Config) -> anyhow::Result<WatchHandler> {
    let (tx, mut rx) = mpsc::channel(100);

    let data = load_or_create_user_data::<Maimai, _>(&config.maimai_uesr_data_path)?;

    spawn(async move {
        let songs: Option<Vec<Song>> = match &config.database_path {
            None => None,
            Some(database_path) => report_error(
                &config.slack_post_webhook,
                &config.user_id,
                read_json(database_path).context("Failed to load song database"),
            )
            .await
            .ok(),
        };
        let database = match &songs {
            None => None,
            Some(songs) => report_error(
                &config.slack_post_webhook,
                &config.user_id,
                SongDatabase::new(songs).context("Failed to construct song database"),
            )
            .await
            .ok(),
        };
        let mut estimator = match database.as_ref() {
            None => None,
            Some(database) => report_error(
                &config.slack_post_webhook,
                &config.user_id,
                Estimator::new(database, MaimaiVersion::latest())
                    .context("Failed to load estimator"),
            )
            .await
            .ok(),
        };
        let estimator_config = match config.estimator_config_path.as_ref() {
            None => None,
            Some(path) => report_error(
                &config.slack_post_webhook,
                &config.user_id,
                read_toml::<_, multi_user::Config>(path).context("Failed to read estmiator config"),
            )
            .await
            .ok(),
        };
        run_estimator(
            estimator.as_mut(),
            estimator_config.as_ref(),
            database.as_ref(),
            &config,
        )
        .await;

        let mut runner = Runner {
            config: &config,
            data,
            database: database.as_ref(),
            estimator_config: estimator_config.as_ref(),
            estimator,
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
            if config.timeout_config.max_count == 1 {
                break;
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

        if let Some(finish_flag) = config.finish_flag {
            finish_flag.store(true, std::sync::atomic::Ordering::Release);
        }
    });
    Ok(WatchHandler(tx))
}

async fn run_estimator<'s, 'n>(
    estimator: Option<&mut MultiUserEstimator<'s, 'n>>,
    estimator_config: Option<&'n multi_user::Config>,
    database: Option<&SongDatabase<'s>>,
    config: &Config,
) {
    if let Some(((estimator, estimator_config), database)) =
        estimator.zip(estimator_config).zip(database)
    {
        let _ = report_error(
            &config.slack_post_webhook,
            &config.user_id,
            (|| {
                let datas = estimator_config.read_all()?;
                multi_user::update_all(database, &datas, estimator)?;
                anyhow::Ok(())
            })(),
        )
        .await;
    }
}

struct Runner<'c, 's, 'd, 'ec> {
    config: &'c Config,
    data: MaimaiUserData,
    database: Option<&'d SongDatabase<'s>>,
    estimator_config: Option<&'ec multi_user::Config>,
    estimator: Option<MultiUserEstimator<'s, 'ec>>,
}
impl<'c> Runner<'c, '_, '_, '_> {
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
            api.overwrite_if_absent(
                &aimes,
                aime.slot_index,
                aime.access_code,
                aime.card_name.clone(),
            )
            .await?;
            sleep(Duration::from_secs(1)).await;
            info!("Switched aime.")
        }

        // Obtain the `force paid` flag (or `()` if international)
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

        // Initialize sega client
        let init = SegaClientInitializer {
            credentials_path: &self.config.credentials_path,
            cookie_store_path: &self.config.cookie_store_path,
            user_identifier: &self.config.user_identifier,
            force_paid,
        };
        let (mut client, index) = T::new_client(init).await?;

        // For safety, we will be saving data over and over again
        write_json(&config.maimai_uesr_data_path, &self.data)?;

        // Retrieve records
        let last_played = index.first().context("There is no play yet.")?.0;
        let inserted_records = update_records(&mut client, &mut self.data.records, index).await?;
        if inserted_records.is_empty() {
            return Ok(false);
        }
        write_json(&config.maimai_uesr_data_path, &self.data)?;

        // Retrieve rating target list
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

        // Retrieve idx to icon map
        if let Ok(Some(rating_targets)) = update_targets_res {
            T::update_idx(&mut client, rating_targets, &mut self.data.idx_to_icon_map).await?;
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                &config.user_id,
                "Disambiguation list updated",
            )
            .await;
        }
        write_json(&config.maimai_uesr_data_path, &self.data)?;

        // Retrieval ends here.

        // Try to associate user data with the database.
        let associated = match &self.database {
            None => None,
            Some(database) => report_error(
                &config.slack_post_webhook,
                &config.user_id,
                associated_user_data::UserData::annotate(database, &self.data)
                    .context("Failed to associate record with database"),
            )
            .await
            .ok(),
        };

        // Try to update internal level estimator.
        // HACK: to take all data into account, we read user data again from scratch.
        // Can we improve it?
        let before_len = self.estimator.as_ref().map_or(0, |x| x.event_len());
        run_estimator(
            self.estimator.as_mut(),
            self.estimator_config,
            self.database,
            config,
        )
        .await;

        // Now the results are reported to Slack.
        for time in inserted_records {
            let record = &self.data.records[&time];
            let associated = associated.as_ref().and_then(|x| x.records().get(&time));
            let level = try_get_level(self.estimator.as_ref(), associated);
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                &config.user_id,
                make_message(record, associated, level).to_string(),
            )
            .await;
        }

        if let Some(estimator) = &self.estimator {
            for event in &estimator.events()[before_len..] {
                webhook_send(
                    client.reqwest(),
                    &config.slack_post_webhook,
                    &config.user_id,
                    format!("★ {event}"),
                )
                .await;
            }
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

    async fn update_targets<'r>(
        client: &mut SegaClient<'_, Self>,
        rating_targets: &'r mut RatingTargetFile,
        last_played: PlayTime,
        force: bool,
    ) -> anyhow::Result<Option<&'r RatingTargetList>>;

    async fn update_idx(
        client: &mut SegaClient<'_, Self>,
        rating_target: &RatingTargetList,
        map: &mut HashMap<ScoreIdx, SongIcon>,
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

    async fn update_targets<'r>(
        client: &mut SegaClient<'_, Self>,
        rating_targets: &'r mut RatingTargetFile,
        last_played: PlayTime,
        force: bool,
    ) -> anyhow::Result<Option<&'r RatingTargetList>> {
        update_targets(client, rating_targets, last_played, force).await
    }

    async fn update_idx(
        client: &mut SegaClient<'_, Maimai>,
        rating_target: &RatingTargetList,
        map: &mut HashMap<ScoreIdx, SongIcon>,
    ) -> anyhow::Result<()> {
        maimai_scraping::maimai::data_collector::update_idx(client, rating_target, map).await
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

    async fn update_targets<'r>(
        _client: &mut SegaClient<'_, Self>,
        _rating_targets: &'r mut RatingTargetFile,
        _last_played: PlayTime,
        _force: bool,
    ) -> anyhow::Result<Option<&'r RatingTargetList>> {
        info!("Maimai international has rating target available!");
        Ok(None)
    }

    async fn update_idx(
        _client: &mut SegaClient<'_, Self>,
        _rating_target: &RatingTargetList,
        _map: &mut HashMap<ScoreIdx, SongIcon>,
    ) -> anyhow::Result<()> {
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

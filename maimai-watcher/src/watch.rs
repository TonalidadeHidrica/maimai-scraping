use std::{
    fmt::Display,
    iter::successors,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Context;
use itertools::Itertools;
use lazy_format::lazy_format;
use log::error;
use maimai_scraping::{
    api::SegaClient,
    data_collector::{
        load_records_from_file, load_targets_from_file, update_records, update_targets, RecordMap,
    },
    fs_json_util::{read_json, write_json},
    maimai::{
        estimate_rating::{ScoreConstantsStore, ScoreKey},
        load_score_level::{self, RemovedSong, Song},
        rating::{ScoreConstant, ScoreLevel},
        rating_target_parser::RatingTargetFile,
        schema::{
            latest::{LifeResult, PlayRecord, RatingBorderColor, ScoreMetadata},
            ver_20210316_2338::RatingValue,
        },
        Maimai,
    },
};
use tokio::{
    spawn,
    sync::mpsc::{self, error::TryRecvError},
    time::sleep,
};
use url::Url;

use crate::slack::webhook_send;

#[derive(Debug)]
pub struct Config {
    pub interval: Duration,
    pub credentials_path: PathBuf,
    pub cookie_store_path: PathBuf,
    pub records_path: PathBuf,
    pub rating_target_path: PathBuf,
    pub levels_path: PathBuf,
    pub removed_songs_path: PathBuf,
    pub slack_post_webhook: Option<Url>,
    pub estimate_internal_levels: bool,
    pub timeout_config: TimeoutConfig,
    pub report_no_updates: bool,
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

    let records = load_records_from_file::<Maimai, _>(&config.records_path)?;
    let rating_targets = load_targets_from_file(&config.rating_target_path)?;

    let levels = load_score_level::load(&config.levels_path)?;
    let removed_songs: Vec<RemovedSong> = read_json(&config.removed_songs_path)?;

    spawn(async move {
        let Ok(mut runner) = report_error(
            &config.slack_post_webhook,
            Runner::new(&config, records, rating_targets, &levels, &removed_songs)
                .await
                .context("Issue in levels or removed_songs"),
        ).await else {
            return;
        };

        let start_time = Instant::now();
        let mut count = 0;
        'outer: while let Err(TryRecvError::Empty | TryRecvError::Disconnected) = rx.try_recv() {
            match runner.run().await {
                Err(e) => {
                    error!("{e}");
                    webhook_send(
                        &reqwest::Client::new(),
                        &config.slack_post_webhook,
                        e.to_string(),
                    )
                    .await;
                }
                Ok(updates) => {
                    if !dbg!(updates) && dbg!(config.report_no_updates) {
                        webhook_send(
                            &reqwest::Client::new(),
                            &config.slack_post_webhook,
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
            } else if (Instant::now() - start_time) >= config.timeout_config.max_duration {
                webhook_send(
                    &reqwest::Client::new(),
                    &config.slack_post_webhook,
                    "There have been no updates for a while.  Stopping automatically.".to_string(),
                )
                .await;
                break;
            }
        }
    });
    Ok(WatchHandler(tx))
}

struct Runner<'c, 's, 'r> {
    config: &'c Config,
    records: RecordMap<Maimai>,
    rating_targets: RatingTargetFile,
    levels_actual: ScoreConstantsStore<'s, 'r>,
    levels_naive: ScoreConstantsStore<'s, 'r>,
}
impl<'c, 's, 'r> Runner<'c, 's, 'r> {
    async fn new(
        config: &'c Config,
        records: RecordMap<Maimai>,
        rating_targets: RatingTargetFile,
        levels: &'s [Song],
        removed_songs: &'r [RemovedSong],
    ) -> anyhow::Result<Runner<'c, 's, 'r>> {
        let levels_actual = ScoreConstantsStore::new(levels, removed_songs)?;
        let levels_naive = ScoreConstantsStore::new(levels, removed_songs)?;
        let mut ret = Self {
            config,
            records,
            rating_targets,
            levels_actual,
            levels_naive,
        };
        ret.update_levels().await;
        Ok(ret)
    }

    async fn update_levels(&mut self) {
        let _ = report_error(
            &self.config.slack_post_webhook,
            self.levels_actual
                .do_everything(self.records.values(), &self.rating_targets)
                .context("While estimating levels precisely"),
        )
        .await;
        let _ = report_error(
            &self.config.slack_post_webhook,
            self.levels_naive
                .guess_from_rating_target_order(&self.rating_targets)
                .context("While estimating levels roughly"),
        )
        .await;
    }

    async fn run(&mut self) -> anyhow::Result<bool> {
        let config = self.config;
        let (mut client, index) = SegaClient::<Maimai>::new(
            &self.config.credentials_path,
            &self.config.cookie_store_path,
        )
        .await?;
        let last_played = index.first().context("There is no play yet.")?.0;
        let inserted_records = update_records(&mut client, &mut self.records, index).await?;
        if inserted_records.is_empty() {
            return Ok(false);
        }
        write_json(&config.records_path, &self.records.values().collect_vec())?;
        let update_targets_res = update_targets(&mut client, &mut self.rating_targets, last_played)
            .await
            .context("Rating target not available");
        if report_error(&config.slack_post_webhook, update_targets_res)
            .await
            .is_ok()
        {
            write_json(&config.rating_target_path, &self.rating_targets)?;
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                "Rating target updated",
            )
            .await;
        }

        let bef_len = self.levels_actual.events().len();
        if self.config.estimate_internal_levels {
            self.update_levels().await;
        }

        for time in inserted_records {
            let record = &self.records[&time];
            let key = ScoreKey::from(record);
            let song_lvs = if let Ok(Some((_, candidates))) = self.levels_naive.get(key) {
                candidates
            } else {
                &[]
            };
            webhook_send(
                client.reqwest(),
                &config.slack_post_webhook,
                make_message(record, song_lvs),
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
                format! {"★ {song_name} ({score_kind}): {event}"},
            )
            .await;
        }

        Ok(true)
    }
}

async fn report_error<T>(url: &Option<Url>, result: anyhow::Result<T>) -> anyhow::Result<T> {
    if let Err(e) = &result {
        error!("{e}");
        webhook_send(&reqwest::Client::new(), url, e.to_string()).await;
    }
    result
}

fn make_message(record: &PlayRecord, song_lvs: &[ScoreConstant]) -> String {
    use maimai_scraping::maimai::schema::latest::{AchievementRank::*, FullComboKind::*};
    let score_kind = describe_score_kind(record.score_metadata());
    let lv = lazy_format!(match (song_lvs[..]) {
        [] => "?",
        [lv] => "{lv}",
        [lv, ..] => ("{}", ScoreLevel::from(lv)),
    });
    let rank = match record.achievement_result().rank() {
        D => "D",
        C => "C",
        BBB => "BBB",
        BB => "BB",
        B => "B",
        A => "A",
        AA => "AA",
        AAA => "AAA",
        S => "S",
        SPlus => "S+",
        SS => "SS",
        SSPlus => "SS+",
        SSS => "SSS",
        SSSPlus => "SSS+",
    };
    let fc = match record.combo_result().full_combo_kind() {
        Nothing => "",
        FullCombo => "FC",
        FullComboPlus => "FC+",
        AllPerfect => "AP",
        AllPerfectPlus => "AP+",
    };
    let main_line = lazy_format!(
        "{time}　{title} ({score_kind} Lv.{lv})　{rank}({ach})　{fc}\n",
        title = record.song_metadata().name(),
        time = record.played_at().time(),
        ach = record.achievement_result().value(),
    );
    let rating_line = (record.rating_result().delta() > 0).then(|| {
        let new = record.rating_result().rating();
        let delta = record.rating_result().delta();
        let old = RatingValue::from((new.get() as i16 - delta) as u16);
        use RatingBorderColor::*;
        let old_color = match old.get() {
            15000.. => Rainbow,
            14500.. => Platinum,
            14000.. => Gold,
            13000.. => Silver,
            12000.. => Bronze,
            10000.. => Purple,
            7000.. => Red,
            4000.. => Orange,
            2000.. => Green,
            1000.. => Blue,
            ..=999 => Normal,
        };
        let new_color = record.rating_result().border_color();
        let color_change = lazy_format!(
            if old_color != new_color => "　Color changed to {new_color:?}!"
            else => ""
        );
        format!("Rating: {old} => {new} ({delta:+}){color_change}\n")
    });
    let rating_line = rating_line.as_deref().unwrap_or("");
    let life_line = match record.life_result() {
        LifeResult::Nothing => None,
        LifeResult::PerfectChallengeResult(res) => Some(("Perfect challenge", res)),
        LifeResult::CourseResult(res) => Some(("Course", res)),
    }
    .map(|(name, res)| format!("{name} life: {}/{}\n", res.value(), res.max()));
    let life_line = life_line.as_deref().unwrap_or("");
    format!("{main_line}{rating_line}{life_line}")
}
fn describe_score_kind<'a>(metadata: ScoreMetadata) -> impl Display + 'a {
    use maimai_scraping::maimai::schema::latest::{ScoreDifficulty::*, ScoreGeneration::*};
    let gen = match metadata.generation() {
        Standard => "STD",
        Deluxe => "DX",
    };
    let dif = match metadata.difficulty() {
        Basic => "Bas",
        Advanced => "Adv",
        Expert => "Exp",
        Master => "Mas",
        ReMaster => "ReMas",
    };
    lazy_format!("{gen} {dif}")
}

pub struct WatchHandler(mpsc::Sender<()>);
impl WatchHandler {
    pub async fn stop(&self) -> Result<(), mpsc::error::SendError<()>> {
        self.0.send(()).await
    }
}

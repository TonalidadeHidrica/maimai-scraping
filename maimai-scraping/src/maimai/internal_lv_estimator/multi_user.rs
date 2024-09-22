use std::{ops::Range, path::PathBuf};

use anyhow::bail;
use chrono::NaiveDateTime;
use clap::Args;
use derive_more::{Display, From};
use getset::{CopyGetters, Getters};
use maimai_scraping_utils::fs_json_util::read_json;
use serde::Deserialize;

use crate::maimai::{
    associated_user_data::{
        self, OrdinaryPlayRecordAssociated, RatingTargetEntryAssociated,
        RatingTargetListAssociated, UserDataOrdinaryAssociated,
    },
    schema::latest::{AchievementValue, PlayTime, RatingValue},
    song_list::database::{OrdinaryScoreRef, SongDatabase},
    MaimaiUserData,
};

use super::{Estimator, NewOrOld, RatingTargetEntryLike, RatingTargetListLike, RecordLike};

pub type MultiUserEstimator<'s, 'n> = Estimator<'s, RecordLabel<'n>, RatingTargetLabel<'n>>;

#[derive(Deserialize, Getters)]
pub struct Config {
    #[getset(get = "pub")]
    users: Vec<UserConfig>,
}

#[derive(Deserialize, Getters, CopyGetters)]
pub struct UserConfig {
    #[getset(get = "pub")]
    name: UserName,
    #[getset(get = "pub")]
    data_path: PathBuf,
    #[getset(get_copy = "pub")]
    estimator_config: EstimatorConfig,
}

#[derive(Clone, Copy, Debug, Deserialize, Args)]
pub struct EstimatorConfig {
    #[arg(long)]
    pub new_songs_are_complete: bool,
    #[arg(long)]
    pub old_songs_are_complete: bool,
    #[arg(long)]
    #[serde(default)]
    pub ignore_time: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, From, Deserialize, Display)]
pub struct UserName(String);

pub type DataPair<'c> = (&'c UserConfig, MaimaiUserData);

impl Config {
    pub fn read_all(&self) -> anyhow::Result<Vec<DataPair>> {
        (self.users.iter())
            .map(|config| anyhow::Ok((config, read_json::<_, MaimaiUserData>(config.data_path())?)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Display)]
pub enum RecordLabel<'n> {
    FromData(RecordLabelFromData<'n>),
    Additional,
}
#[derive(Clone, Copy, Debug, Display, CopyGetters)]
#[display(fmt = "play record played at {play_time} by {user}")]
#[getset(get_copy = "pub")]
pub struct RecordLabelFromData<'n> {
    play_time: PlayTime,
    user: &'n UserName,
}
#[derive(Clone, Copy, Debug, Display, CopyGetters)]
#[display(fmt = "rating target recorded at {timestamp} by {user} (iteration {iteration})")]
#[getset(get_copy = "pub")]
pub struct RatingTargetLabel<'n> {
    timestamp: PlayTime,
    user: &'n UserName,
    iteration: usize,
}

impl<'c, 'd, 's> RecordLike<'s, RecordLabel<'c>>
    for (&'c UserConfig, OrdinaryPlayRecordAssociated<'d, 's>)
{
    fn played_within(&self, time_range: Range<PlayTime>) -> bool {
        self.0.estimator_config.ignore_time
            || time_range.contains(&self.1.record().played_at().time())
    }
    fn score(&self) -> OrdinaryScoreRef<'s> {
        self.1.score().score()
    }
    fn achievement(&self) -> AchievementValue {
        self.1.record().achievement_result().value()
    }
    fn rating_delta(&self) -> i16 {
        self.1.record().rating_result().delta()
    }
    fn label(&self) -> RecordLabel<'c> {
        RecordLabel::FromData(RecordLabelFromData {
            play_time: self.1.record().played_at().time(),
            user: &self.0.name,
        })
    }
}
impl<'c, 'a, 'd, 's> RatingTargetListLike<'s, RatingTargetLabel<'c>>
    for (
        &'c UserConfig,
        PlayTime,
        &'a RatingTargetListAssociated<'d, 's>,
        usize,
    )
{
    fn played_within(&self, time_range: Range<PlayTime>) -> bool {
        self.0.estimator_config.ignore_time || time_range.contains(&self.1)
    }
    fn play_time(&self) -> NaiveDateTime {
        self.1.get()
    }
    fn rating(&self) -> RatingValue {
        self.2.list().rating()
    }

    type Entry = RatingTargetEntryAssociated<'d, 's>;
    type Entries = std::iter::Copied<std::slice::Iter<'a, RatingTargetEntryAssociated<'d, 's>>>;
    fn target_new(&self) -> Self::Entries {
        self.2.target_new().iter().copied()
    }
    fn target_old(&self) -> Self::Entries {
        self.2.target_old().iter().copied()
    }
    fn candidates_new(&self) -> Self::Entries {
        self.2.candidates_new().iter().copied()
    }
    fn candidates_old(&self) -> Self::Entries {
        self.2.candidates_old().iter().copied()
    }

    fn label(&self) -> RatingTargetLabel<'c> {
        RatingTargetLabel {
            timestamp: self.1,
            user: &self.0.name,
            iteration: self.3,
        }
    }
}
impl<'d, 's> RatingTargetEntryLike<'s> for RatingTargetEntryAssociated<'d, 's> {
    fn score(&self) -> OrdinaryScoreRef<'s> {
        RatingTargetEntryAssociated::score(self).score()
    }
    fn achievement(&self) -> AchievementValue {
        self.data().achievement()
    }
}

pub type AssociatedDataPair<'c, 'd, 's> = (&'c UserConfig, UserDataOrdinaryAssociated<'d, 's>);

pub fn associate_all<'d, 's, 'c>(
    database: &SongDatabase<'s>,
    datas: &'d [DataPair<'c>],
) -> anyhow::Result<Vec<AssociatedDataPair<'c, 'd, 's>>> {
    datas
        .iter()
        .map(|&(config, ref data)| {
            let data = associated_user_data::UserData::annotate(database, data)?;
            let data = data.ordinary_data_associated()?;
            anyhow::Ok((config, data))
        })
        .collect()
}

pub fn update_all<'s, 'c>(
    database: &SongDatabase<'s>,
    datas: &[DataPair<'c>],
    estimator: &mut MultiUserEstimator<'s, 'c>,
) -> anyhow::Result<()> {
    let datas = associate_all(database, datas)?;
    estimate_all(&datas, estimator)
}

pub fn estimate_all<'s, 'c>(
    datas: &[AssociatedDataPair<'c, '_, 's>],
    estimator: &mut MultiUserEstimator<'s, 'c>,
) -> anyhow::Result<()> {
    // It never happens that once "determine by delta" fails,
    // but succeeds afterwards due to additionally determined internal levels.
    for &(config, ref data) in datas {
        let ordinary_records = data.ordinary_records();
        if config.estimator_config.new_songs_are_complete {
            estimator
                .determine_by_delta(ordinary_records.iter().map(|&r| (config, r)), NewOrOld::New)?;
        }
        if config.estimator_config.old_songs_are_complete {
            estimator
                .determine_by_delta(ordinary_records.iter().map(|&r| (config, r)), NewOrOld::Old)?;
        }
    }

    for i in 0.. {
        let before_len = estimator.event_len();
        for &(config, ref data) in datas {
            let rating_targets = data.rating_target();
            estimator.guess_from_rating_target_order(
                rating_targets
                    .iter()
                    .map(|&(time, ref list)| (config, time, list, i)),
            )?;
        }
        if before_len == estimator.event_len() {
            return Ok(());
        }
    }
    bail!("Did not finish after 2^64-1 times (whoa, are humans still there?)");
}

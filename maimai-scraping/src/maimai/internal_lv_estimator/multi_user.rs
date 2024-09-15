use std::path::PathBuf;

use anyhow::anyhow;
use chrono::NaiveDateTime;
use clap::Args;
use derive_more::{Display, From};
use getset::{CopyGetters, Getters};
use maimai_scraping_utils::fs_json_util::read_json;
use serde::Deserialize;

use crate::maimai::{
    associated_user_data::{self, OrdinaryPlayRecordAssociated},
    schema::latest::PlayTime,
    song_list::database::SongDatabase,
    MaimaiUserData,
};

use super::{Estimator, NewOrOld, RecordLike};

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

impl Config {
    pub fn read_all(&self) -> anyhow::Result<Vec<(&UserConfig, MaimaiUserData)>> {
        (self.users.iter())
            .map(|config| anyhow::Ok((config, read_json::<_, MaimaiUserData>(config.data_path())?)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "play record played at {play_time} by {user}")]
pub struct RecordLabel<'n> {
    play_time: PlayTime,
    user: &'n UserName,
}
#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "rating target recorded at {timestamp} by {user}")]
pub struct RatingTargetLabel<'n> {
    timestamp: NaiveDateTime,
    user: &'n UserName,
}

impl<'c, 't, 'd, 's> RecordLike<'s, RecordLabel<'c>>
    for (&'c UserConfig, &'t OrdinaryPlayRecordAssociated<'d, 's>)
{
    fn played_within(&self, time_range: std::ops::Range<PlayTime>) -> bool {
        self.0.estimator_config.ignore_time
            || time_range.contains(&self.1.record().played_at().time())
    }
    fn score(&self) -> crate::maimai::song_list::database::OrdinaryScoreRef<'s> {
        self.1.score().score()
    }
    fn achievement(&self) -> crate::maimai::schema::latest::AchievementValue {
        self.1.record().achievement_result().value()
    }
    fn rating_delta(&self) -> i16 {
        self.1.record().rating_result().delta()
    }
    fn label(&self) -> RecordLabel<'c> {
        RecordLabel {
            play_time: self.1.record().played_at().time(),
            user: &self.0.name,
        }
    }
}

pub fn update_all<'s, 'c>(
    database: &SongDatabase<'s>,
    datas: &[(&'c UserConfig, MaimaiUserData)],
    constants: &mut Estimator<'s, RecordLabel<'c>, RatingTargetLabel<'c>>,
) -> anyhow::Result<()> {
    let datas = datas
        .iter()
        .map(|&(config, ref data)| {
            let data = associated_user_data::UserData::annotate(database, data)?;
            let ordinary_records = data
                .records()
                .values()
                .filter_map(|r| Some(r.as_ordinary()?.into_associated()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| anyhow!("{e:#?}"))?;
            anyhow::Ok((config, data, ordinary_records))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // It never happens that once "determine by delta" fails,
    // but succeeds afterwards due to additionally determined internal levels.
    for &(config, _, ref ordinary_records) in &datas {
        if config.estimator_config.new_songs_are_complete {
            constants
                .determine_by_delta(ordinary_records.iter().map(|r| (config, r)), NewOrOld::New)?;
        }
        if config.estimator_config.old_songs_are_complete {
            constants
                .determine_by_delta(ordinary_records.iter().map(|r| (config, r)), NewOrOld::Old)?;
        }
    }

    while {
        let mut changed = false;
        for (_config, _data, _) in &datas {
            // TODO
            changed = true;
        }
        changed
    } {}
    Ok(())
}

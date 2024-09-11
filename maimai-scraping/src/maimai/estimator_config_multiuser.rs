use std::path::PathBuf;

use derive_more::{Display, From};
use getset::{CopyGetters, Getters};
use maimai_scraping_utils::fs_json_util::read_json;
use serde::Deserialize;

use super::{
    estimate_rating::{EstimatorConfig, ScoreConstantsStore},
    MaimaiUserData,
};

#[derive(Deserialize, Getters)]
pub struct Root {
    #[getset(get = "pub")]
    users: Vec<User>,
}

#[derive(Deserialize, Getters, CopyGetters)]
pub struct User {
    #[getset(get = "pub")]
    name: UserName,
    #[getset(get = "pub")]
    data_path: PathBuf,
    #[getset(get_copy = "pub")]
    estimator_config: EstimatorConfig,
}

#[derive(Clone, PartialEq, Eq, Debug, From, Deserialize, Display)]
pub struct UserName(String);

impl Root {
    pub fn read_all(&self) -> anyhow::Result<Vec<(&User, MaimaiUserData)>> {
        (self.users.iter())
            .map(|config| anyhow::Ok((config, read_json::<_, MaimaiUserData>(config.data_path())?)))
            .collect()
    }
}

pub fn update_all(
    datas: &[(&User, MaimaiUserData)],
    constants: &mut ScoreConstantsStore,
) -> anyhow::Result<()> {
    while {
        let mut changed = false;
        for (config, data) in datas {
            config.name();
            changed |= constants.do_everything(
                config.estimator_config(),
                Some(config.name()),
                data.records.values(),
                &data.rating_targets,
                &data.idx_to_icon_map,
            )?;
        }
        changed
    } {}
    Ok(())
}

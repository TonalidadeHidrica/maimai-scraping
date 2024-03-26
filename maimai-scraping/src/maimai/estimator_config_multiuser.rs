use std::path::PathBuf;

use derive_more::From;
use getset::{CopyGetters, Getters};
use serde::Deserialize;

use super::estimate_rating::EstimatorConfig;

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

#[derive(From, Deserialize)]
pub struct UserName(String);

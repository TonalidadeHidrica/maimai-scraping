use derive_more::{AsRef, Display, From};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

#[derive(Debug, TypedBuilder, Serialize, Deserialize)]
pub struct Credentials {
    pub sega_id: SegaId,
    pub password: Password,
}

#[derive(Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct SegaId(String);

#[derive(Debug, From, AsRef, Display, Serialize, Deserialize)]
#[as_ref(forward)]
pub struct Password(String);

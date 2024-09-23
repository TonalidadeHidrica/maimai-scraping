use std::{
    fmt::Debug,
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use anyhow::Context;
use fs_err::File;
use serde::{Deserialize, Serialize};

pub fn read_json<P: Into<PathBuf> + Debug, T: for<'de> Deserialize<'de>>(
    path: P,
) -> anyhow::Result<T> {
    let path = path.into();
    (|| serde_json::from_reader(BufReader::new(File::open(&path)?)).map_err(anyhow::Error::new))()
        .with_context(|| {
            format!(
                "While trying to parse {path:?} as {}",
                std::any::type_name::<T>()
            )
        })
}
pub fn write_json<P: Into<PathBuf>, T: Serialize>(path: P, value: &T) -> anyhow::Result<()> {
    Ok(serde_json::to_writer(
        BufWriter::new(File::create(path)?),
        value,
    )?)
}

pub fn read_toml<P: Into<PathBuf> + Debug, T: for<'de> Deserialize<'de>>(
    path: P,
) -> anyhow::Result<T> {
    let path = path.into();
    (|| toml::from_str(&fs_err::read_to_string(&path)?).map_err(anyhow::Error::new))().with_context(
        || {
            format!(
                "While trying to parse {path:?} as {}",
                std::any::type_name::<T>()
            )
        },
    )
}

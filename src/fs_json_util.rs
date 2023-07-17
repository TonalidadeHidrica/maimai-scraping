use std::{
    io::{BufReader, BufWriter},
    path::PathBuf,
};

use fs_err::File;
use serde::{Deserialize, Serialize};

pub fn read_json<P: Into<PathBuf>, T: for<'de> Deserialize<'de>>(path: P) -> anyhow::Result<T> {
    Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
}
pub fn write_json<P: Into<PathBuf>, T: Serialize>(path: P, value: &T) -> anyhow::Result<()> {
    Ok(serde_json::to_writer(
        BufWriter::new(File::create(path)?),
        value,
    )?)
}

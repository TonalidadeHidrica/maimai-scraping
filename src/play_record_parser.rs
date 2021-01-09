use std::convert::TryFrom;

use crate::schema::*;
use anyhow::anyhow;
use scraper::Html;

pub fn parse(_html: Html) -> anyhow::Result<PlayRecord> {
    let p = PlayedAt::builder()
        .time("2020-11-30T03:45:21".parse()?)
        .place("hogehoge center".to_owned())
        .track(TrackIndex::try_from(1).map_err(|_| anyhow!("out of bounds"))?)
        .build();
    unimplemented!()
}

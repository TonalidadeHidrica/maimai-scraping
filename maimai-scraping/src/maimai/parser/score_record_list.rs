use scraper::Html;

use crate::maimai::{rating::ScoreLevel, schema::latest::{ScoreDifficulty, SongName}};

pub fn parse(html: &Html)  -> anyhow::Result<SongRecordList> {
}

pub struct SongRecordList {
    difficulty: ScoreDifficulty,
    scores: Vec<Score>,
}

pub struct Score {
    song_name: SongName,
    level: ScoreLevel,
}

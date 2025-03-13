use std::collections::BTreeMap;

use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    parser::song_score::ScoreEntry, rating::ScoreLevel, schema::latest::ScoreDifficulty,
    IdxToIconMap,
};

#[derive(Default, Serialize, Deserialize)]
pub struct SongScoreList {
    pub by_difficulty: EnumMap<ScoreDifficulty, Vec<ScoreEntry>>,
    pub by_level: Vec<(ScoreLevel, Vec<ScoreEntry>)>,
    pub idx_to_icon_map: IdxToIconMap,
}

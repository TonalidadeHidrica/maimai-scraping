use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    parser::song_score::ScoreEntry, schema::latest::ScoreDifficulty, IdxToIconMap,
};

#[derive(Default, Serialize, Deserialize)]
pub struct SongScoreList {
    pub entries: EnumMap<ScoreDifficulty, Vec<ScoreEntry>>,
    pub idx_to_icon_map: IdxToIconMap,
}

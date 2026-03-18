use enum_map::EnumMap;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    parser::song_score::ScoreEntry,
    rating::ScoreLevel,
    schema::latest::{ScoreDifficulty, SongIcon, SongName},
    IdxToIconMap,
};

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct SongScoreList {
    pub by_difficulty: EnumMap<ScoreDifficulty, Vec<EntryGroup>>,
    pub by_level: Vec<(ScoreLevel, Vec<EntryGroup>)>,
    pub idx_to_icon_map: IdxToIconMap,
    pub song_name_to_icon_hint: Vec<(SongName, SongIcon)>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct EntryGroup {
    pub label: String,
    pub entries: Vec<ScoreEntry>,
}

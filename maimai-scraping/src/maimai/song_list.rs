use serde::{Serialize, Deserialize};

use super::{load_score_level::MaimaiVersion, schema::latest::SongName};

pub struct SongList {
    songs: Vec<Song>
}

pub struct Song {
    name: SongName,
    pronunciation: SongPronunciation,
    scores: Scores,
}

#[derive(
    Clone,
    PartialEq,
    Eq,
    Hash,
    Debug,
    derive_more::From,
    derive_more::AsRef,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
#[as_ref(forward)]
pub struct SongPronunciation(String);

#[derive(
    Clone,
    PartialEq,
    Eq,
    Hash,
    Debug,
    derive_more::From,
    derive_more::AsRef,
    derive_more::FromStr,
    derive_more::Display,
    Serialize,
    Deserialize,
)]
#[as_ref(forward)]
pub struct SongAbbrev(String);

pub enum Scores {
    Ordinary(OrdinaryScores),
    Utage(UtageScore),
}

pub struct OrdinaryScores();

pub struct ScoresForGeneration {
    version: MaimaiVersion,
}

pub struct UtageScore {
    // TODO
}

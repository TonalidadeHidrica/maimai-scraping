use std::fmt::Display;

use anyhow::{bail, Context};
use derive_by_key::DeriveByKey;
use getset::{CopyGetters, Getters};
use hashbrown::HashMap;
use itertools::Itertools;

use crate::maimai::{
    load_score_level::MaimaiVersion,
    official_song_list::UtageScore,
    rating::InternalScoreLevel,
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
    song_list::RemoveState,
};

use super::{OrdinaryScore, OrdinaryScores, Song};

#[derive(Getters)]
#[getset(get = "pub")]
pub struct SongDatabase<'s> {
    songs: Vec<SongRef<'s>>,
    icon_map: HashMap<&'s SongIcon, SongRef<'s>>,
    name_map: HashMap<&'s SongName, Vec<SongRef<'s>>>,
}
impl<'s> SongDatabase<'s> {
    pub fn new(songs: &'s [Song]) -> anyhow::Result<Self> {
        verify_songs(songs)?;

        let songs = {
            let mut ret = vec![];
            let mut id = 0;
            for song in songs {
                ret.push(SongRef { song, id });
                id += 10 + song.utage_scores.len();
            }
            ret
        };

        // Make icon map.
        // `verify_songs` guarantees that an icon exists for all unremoved songs.
        let icon_map = songs
            .iter()
            .filter_map(|&x| Some((x.song.icon.as_ref()?, x)))
            .collect();

        // Make song name map.
        let mut name_map = HashMap::<_, Vec<_>>::new();
        for &song in &songs {
            // `verify_songs` guarantees that a song name exists for all songs.
            let name = song.song.name.values().flatten().last().unwrap();
            name_map.entry(name).or_default().push(song);
        }

        Ok(Self {
            songs,
            icon_map,
            name_map,
        })
    }

    pub fn song_from_icon(&self, icon: &SongIcon) -> anyhow::Result<SongRef<'s>> {
        self.icon_map
            .get(icon)
            .copied()
            .with_context(|| format!("No song matches {icon:?}"))
    }

    pub fn song_from_name<'me>(
        &'me self,
        song_name: &SongName,
    ) -> impl Iterator<Item = SongRef<'s>> + 'me {
        self.name_map.get(song_name).into_iter().flatten().copied()
    }

    pub fn all_scores_for_version<'me>(
        &'me self,
        version: MaimaiVersion,
    ) -> impl Iterator<Item = OrdinaryScoreForVersionRef<'s>> + 'me {
        self.songs
            .iter()
            .flat_map(|song| song.scoreses())
            .flat_map(|scores| scores.all_scores())
            .filter_map(move |score| score.for_version(version))
    }
}

#[derive(Clone, Copy, Debug, CopyGetters, DeriveByKey)]
#[derive_by_key(key = "key", PartialEq, Eq, PartialOrd, Ord, Hash)]
#[getset(get_copy = "pub")]
pub struct SongRef<'s> {
    song: &'s Song,
    id: usize,
}
impl<'s> SongRef<'s> {
    fn key(self) -> usize {
        self.id
    }

    pub fn scoreses(self) -> impl Iterator<Item = OrdinaryScoresRef<'s>> {
        [ScoreGeneration::Standard, ScoreGeneration::Deluxe]
            .into_iter()
            .filter_map(move |g| self.scores(g))
    }

    pub fn scores(self, generation: ScoreGeneration) -> Option<OrdinaryScoresRef<'s>> {
        let scores = self.song.scores[generation].as_ref()?;
        Some(OrdinaryScoresRef {
            song: self,
            generation,
            scores,
            id: self.id
                + match generation {
                    ScoreGeneration::Standard => 0,
                    ScoreGeneration::Deluxe => 5,
                },
        })
    }

    pub fn utage_scores(self) -> impl Iterator<Item = UtageScoreRef<'s>> {
        self.song
            .utage_scores
            .iter()
            .map(move |score| UtageScoreRef { song: self, score })
    }

    pub fn latest_song_name(&self) -> &SongName {
        // Guaranteed to be present by `verify_songs`
        self.song.latest_song_name().unwrap()
    }
}

/// A reference to a score for a specific version.
#[derive(Clone, Copy, Debug)]
pub enum ScoreForVersionRef<'s> {
    Ordinary(OrdinaryScoreForVersionRef<'s>),
    Utage(UtageScoreRef<'s>),
}

/// A reference to a set of scores for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoresRef<'s> {
    song: SongRef<'s>,
    generation: ScoreGeneration,
    scores: &'s OrdinaryScores,
    id: usize,
}
impl<'s> OrdinaryScoresRef<'s> {
    pub fn score(self, difficulty: ScoreDifficulty) -> Option<OrdinaryScoreRef<'s>> {
        let score = match difficulty {
            ScoreDifficulty::Basic => &self.scores.basic,
            ScoreDifficulty::Advanced => &self.scores.advanced,
            ScoreDifficulty::Expert => &self.scores.expert,
            ScoreDifficulty::Master => &self.scores.master,
            ScoreDifficulty::ReMaster => self.scores.re_master.as_ref()?,
            ScoreDifficulty::Utage => return None,
        };
        Some(OrdinaryScoreRef {
            scores: self,
            difficulty,
            score,
            id: self.id
                + match difficulty {
                    ScoreDifficulty::Basic => 0,
                    ScoreDifficulty::Advanced => 1,
                    ScoreDifficulty::Expert => 2,
                    ScoreDifficulty::Master => 3,
                    ScoreDifficulty::ReMaster => 4,
                    ScoreDifficulty::Utage => unreachable!(),
                },
        })
    }

    pub fn all_scores(self) -> impl Iterator<Item = OrdinaryScoreRef<'s>> {
        use ScoreDifficulty::*;
        [Basic, Advanced, Expert, Master, ReMaster]
            .into_iter()
            .filter_map(move |d| self.score(d))
    }
}

/// A reference to an ordinary for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters, DeriveByKey)]
#[derive_by_key(key = "key", PartialEq, Eq, PartialOrd, Ord, Hash)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoreRef<'s> {
    scores: OrdinaryScoresRef<'s>,
    difficulty: ScoreDifficulty,
    score: &'s OrdinaryScore,
    id: usize,
}
impl<'s> OrdinaryScoreRef<'s> {
    fn key(self) -> usize {
        self.id
    }

    pub fn for_version(self, version: MaimaiVersion) -> Option<OrdinaryScoreForVersionRef<'s>> {
        if let Some(start_version) = self.scores.scores.version {
            if version < start_version {
                return None;
            }
        }
        if !self
            .scores
            .song
            .song
            .remove_state
            .exist_for_version(version)
        {
            return None;
        }
        Some(OrdinaryScoreForVersionRef {
            score: self,
            version,
            level: self.score.levels[version],
        })
    }
}
impl Display for OrdinaryScoreRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({} {})",
            self.scores.song.latest_song_name(),
            self.scores.generation.abbrev(),
            self.difficulty.abbrev(),
        )
    }
}

/// A reference to an ordinary score for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoreForVersionRef<'s> {
    score: OrdinaryScoreRef<'s>,
    version: MaimaiVersion,
    level: Option<InternalScoreLevel>,
}

/// A refegence to an utage score.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct UtageScoreRef<'s> {
    song: SongRef<'s>,
    score: &'s UtageScore,
}

pub fn verify_songs(songs: &[Song]) -> anyhow::Result<()> {
    // Every song has an associated song name.
    for song in songs {
        if song.latest_song_name().is_none() {
            bail!("Song does not have a song name: {song:#?}");
        }
    }

    // Every song that has not been rmeoved has ...
    for song in songs {
        if song.removed() {
            continue;
        }
        // an icon associated to it
        if song.icon.is_none() {
            bail!("Icon is missing: {song:#?}")
        }

        // Moreover, every score in such a song has ...
        for (generation, scores) in &song.scores {
            let Some(scores) = scores else { continue };
            // a version
            if scores.version.is_none() {
                bail!("Version is missing on generation {generation:?}: {song:#?}");
                // println!(
                //     "Version unknown: {:?} {generation:?}",
                //     song.latest_song_name()
                // );
            }
        }
    }

    // There is no two songs with the same icon.
    {
        let mut icons = songs
            .iter()
            .filter_map(|song| Some((song, song.icon.as_ref()?)))
            .collect_vec();
        icons.sort_by_key(|x| x.1);
        if let Some((x, y)) = icons.iter().tuple_windows().find(|(x, y)| x.1 == y.1) {
            bail!(
                "At least one pair of songs has the same icon {:?}: {:#?}, {:#?}",
                x.0,
                x.1,
                y.1
            );
        }
    }

    // Every remove date and recover date has an associated version.
    for song in songs {
        match song.remove_state {
            RemoveState::Present => {}
            RemoveState::Removed(x) => {
                MaimaiVersion::of_date(x)
                        .with_context(|| format!("The remove date of the following song does not have an associated version: {song:?}"))?;
            }
            RemoveState::Revived(x, y) => {
                MaimaiVersion::of_date(x)
                        .with_context(|| format!("The remove date of the following song does not have an associated version: {song:?}"))?;
                MaimaiVersion::of_date(y)
                        .with_context(|| format!("The recover date of the following song does not have an associated version: {song:?}"))?;
            }
        }
    }

    Ok(())
}

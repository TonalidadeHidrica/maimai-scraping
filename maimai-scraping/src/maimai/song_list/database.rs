use anyhow::{bail, Context};
use derive_by_key::DeriveByKey;
use getset::{CopyGetters, Getters};
use hashbrown::HashMap;
use itertools::Itertools;

use crate::maimai::{
    load_score_level::{InternalScoreLevel, MaimaiVersion},
    official_song_list::UtageScore,
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

        let songs = songs
            .iter()
            .enumerate()
            .map(|(id, song)| SongRef { song, id })
            .collect_vec();

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

    pub fn scores(self, generation: ScoreGeneration) -> Option<OrdinaryScoresRef<'s>> {
        let scores = self.song.scores[generation].as_ref()?;
        Some(OrdinaryScoresRef {
            song: self,
            generation,
            scores,
        })
    }

    pub fn utage_scores(self) -> impl Iterator<Item = UtageScoreRef<'s>> {
        self.song
            .utage_scores
            .iter()
            .map(move |score| UtageScoreRef { song: self, score })
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
        })
    }
}

/// A reference to an ordinary for a specific version.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScoreRef<'s> {
    scores: OrdinaryScoresRef<'s>,
    difficulty: ScoreDifficulty,
    score: &'s OrdinaryScore,
}
impl<'s> OrdinaryScoreRef<'s> {
    pub fn for_version(self, version: MaimaiVersion) -> Option<OrdinaryScoreForVersionRef<'s>> {
        if let Some(start_version) = self.scores.scores.version {
            if version < start_version {
                return None;
            }
            match self.scores.song.song.remove_state {
                RemoveState::Present => {}
                RemoveState::Removed(x) => {
                    let remove_version = MaimaiVersion::of_date(x).unwrap();
                    let removed_at_the_beginning = x == remove_version.start_date();
                    let removed = if removed_at_the_beginning {
                        remove_version <= version
                    } else {
                        remove_version < version
                    };
                    if removed {
                        return None;
                    }
                }
                RemoveState::Revived(x, y) => {
                    let remove_version = MaimaiVersion::of_date(x).unwrap();
                    let recover_version = MaimaiVersion::of_date(y).unwrap();

                    let removed_at_the_beginning = x == remove_version.start_date();
                    let after_removed = if removed_at_the_beginning {
                        remove_version <= version
                    } else {
                        remove_version < version
                    };

                    if after_removed && version < recover_version {
                        return None;
                    }
                }
            }
        }
        Some(OrdinaryScoreForVersionRef {
            score: self,
            version,
            level: self.score.levels[version],
        })
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

/// A reference to an utage score.
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct UtageScoreRef<'s> {
    song: SongRef<'s>,
    score: &'s UtageScore,
}

pub fn verify_songs(songs: &[Song]) -> anyhow::Result<()> {
    // Every song that has not been rmeoved has an icon associated to it.
    for song in songs {
        if !song.removed() && song.icon.is_none() {
            bail!("Icon is missing: {song:#?}")
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

    // Every song has an associated song name.
    for song in songs {
        if song.name.values().flatten().next().is_none() {
            bail!("Song does not have a song name: {song:#?}");
        }
    }

    Ok(())
}

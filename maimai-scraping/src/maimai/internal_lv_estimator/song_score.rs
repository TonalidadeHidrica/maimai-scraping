use std::{cmp::Reverse, collections::BTreeMap};

use anyhow::{bail, Context, Result};
use hashbrown::HashMap;
use itertools::Itertools;

use crate::maimai::{
    parser::song_score::ScoreEntry,
    rating::ScoreLevel,
    schema::latest::ScoreDifficulty,
    song_list::{
        database::{OrdinaryScoreRef, SongDatabase},
        song_score::SongScoreList,
    },
    version::MaimaiVersion,
};

pub struct AssociatedSongScoreList<'s> {
    pub scores_by_level: BTreeMap<ScoreLevel, Vec<ScoreAndOrder<'s>>>,
}
pub struct ScoreAndOrder<'s> {
    pub score: OrdinaryScoreRef<'s>,
    pub order: ScoreOrder,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ScoreOrder {
    genre_index: usize,
    difficulty: Reverse<ScoreDifficulty>,
    score_index_in_genre: usize,
}

impl<'s> AssociatedSongScoreList<'s> {
    pub fn from_song_score_list(
        database: &SongDatabase<'s>,
        version: MaimaiVersion,
        list: &SongScoreList,
    ) -> Result<Self> {
        let entry_to_score = |entry: &ScoreEntry| {
            let song = match list.idx_to_icon_map.get(entry.idx()) {
                Some(icon) => database.song_from_icon(icon),
                None => match database
                    .song_from_name_in_version(entry.song_name(), version)
                    .collect_vec()[..]
                {
                    [song] => Ok(song),
                    ref songs => bail!("Song is not unique: {entry:?}\nFound: {songs:?}"),
                },
            };
            let score = song?
                .scores(entry.metadata().generation())
                .with_context(|| {
                    format!("Scores for the specified generation not found: {entry:?}")
                })?
                .score(entry.metadata().difficulty())
                .with_context(|| {
                    format!("Score for the specified difficluty not found: {entry:?}")
                })?;
            anyhow::Ok(score)
        };

        let mut score_to_order = HashMap::new();
        for (difficulty, groups) in &list.by_difficulty {
            for (genre_index, group) in groups.iter().enumerate() {
                for (score_index_in_genre, entry) in group.entries.iter().enumerate() {
                    score_to_order.insert(
                        entry_to_score(entry)?,
                        ScoreOrder {
                            genre_index,
                            difficulty: Reverse(difficulty),
                            score_index_in_genre,
                        },
                    );
                }
            }
        }

        let scores_by_level = (list.by_level.iter())
            .map(|&(level, ref groups)| {
                if groups.len() != 1 {
                    bail!("There should be exactly one group (Lv.{level})");
                }
                // println!("- Lv.{level}");
                let orders = groups[0]
                    .entries
                    .iter()
                    .map(|entry| {
                        let score = entry_to_score(entry)?;
                        let &order = score_to_order
                            .get(&score)
                            .with_context(|| format!("Score missing: {entry:?}"))?;
                        // println!("    - {score} => {order:?}");
                        anyhow::Ok(ScoreAndOrder { score, order })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok((level, orders))
            })
            .collect::<Result<_>>()?;

        Ok(Self { scores_by_level })
    }
}

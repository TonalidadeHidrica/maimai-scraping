use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use hashbrown::HashMap;
use itertools::{izip, Itertools};

use crate::maimai::{
    parser::song_score::ScoreEntry,
    rating::ScoreLevel,
    schema::latest::ScoreDifficulty,
    song_list::{
        database::{OrdinaryScoreRef, OrdinaryScoresRef, SongDatabase},
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
    scores_index: usize,
    // difficulty: ScoreDifficulty,
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

        let scores_to_index = {
            use ScoreDifficulty::*;

            let mut song_to_order = HashMap::<OrdinaryScoresRef, _>::new();
            let baem = || {
                let d = |difficulty| {
                    list.by_difficulty[difficulty]
                        .iter()
                        .flat_map(|x| &x.entries)
                };
                [d(Basic), d(Advanced), d(Expert), d(Master)]
            };

            // Check that the lengths are equal
            if !baem().into_iter().map(|x| x.count()).all_equal() {
                bail!("Scores for Basic, Advanced, Expert, and Master do not have the same length");
            }

            // Construct song to order list
            let [b, a, e, m] = baem();
            for (i, (b, a, e, m)) in izip!(b, a, e, m).enumerate() {
                let baem @ [b, a, e, m] = [
                    entry_to_score(b)?,
                    entry_to_score(a)?,
                    entry_to_score(e)?,
                    entry_to_score(m)?,
                ];
                // Make sure that the songs are in the same order
                if !baem.iter().map(|x| x.scores()).all_equal() {
                    bail!("Refferring to different songs:\n  {b}\n  {a}\n  {e}\n  {m}");
                }
                if song_to_order.insert(b.scores(), i).is_some() {
                    bail!("Duplicate scores: {:?}", b.scores());
                }
            }

            // Make sure that ReMaster songs are in the same order
            {
                let mut bef = None;
                for entry in list.by_difficulty[ReMaster].iter().flat_map(|x| &x.entries) {
                    let score = entry_to_score(entry)?;
                    let order = song_to_order
                        .get(&score.scores())
                        .with_context(|| format!("Score not found: {score}"))?;
                    if let Some((bef_order, bef_score)) = bef {
                        if bef_order >= order {
                            bail!("Re:Master not in order: {bef_score} = {bef_order}, {score} = {order}");
                        }
                    }
                    bef = Some((order, score));
                }
            }

            song_to_order
        };

        // let mut score_to_order = HashMap::new();
        // for (difficulty, groups) in &list.by_difficulty {
        //     for group in groups {
        //         use Category::*;
        //         let category: Category = group.label.parse()?;
        //         let genre_index = match category {
        //             GamesVariety => 3,
        //             PopsAnime => 0,
        //             MaimaiOriginal => 4,
        //             NiconicoVocaloid => 1,
        //             OngekiChunithm => 5,
        //             TouhouProject => 2,
        //         };
        //         for (score_index_in_genre, entry) in group.entries.iter().enumerate() {
        //             score_to_order.insert(
        //                 entry_to_score(entry)?,
        //                 ScoreOrder {
        //                     genre_index,
        //                     difficulty: Reverse(difficulty),
        //                     score_index_in_genre,
        //                 },
        //             );
        //         }
        //     }
        // }

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
                        let &scores_index = scores_to_index
                            .get(&score.scores())
                            .with_context(|| format!("Score missing: {entry:?}"))?;
                        let order = ScoreOrder {
                            scores_index,
                            // difficulty: entry.metadata().difficulty(),
                        };
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

use std::collections::BTreeMap;

use anyhow::{bail, Context};
use getset::{CopyGetters, Getters};
use itertools::Itertools;

use crate::maimai::schema::latest::UtageKindRaw;

use super::{
    load_score_level::MaimaiVersion,
    parser::rating_target,
    schema::latest::{self as schema, PlayTime},
    song_list::database::{
        OrdinaryScoreForVersionRef, ScoreForVersionRef, SongDatabase, UtageScoreRef,
    },
    MaimaiUserData,
};

#[derive(Getters)]
#[getset(get = "pub")]
pub struct UserData<'d, 's> {
    records: BTreeMap<PlayTime, PlayRecord<'d, 's>>,
    rating_target: BTreeMap<PlayTime, RatingTargetList<'d, 's>>,
}

#[derive(Getters, CopyGetters)]
pub struct PlayRecord<'d, 's> {
    #[getset(get = "pub")]
    record: &'d schema::PlayRecord,
    #[getset(get_copy = "pub")]
    score: ScoreForVersionRef<'s>,
}

#[derive(Getters)]
pub struct RatingTargetList<'d, 's> {
    #[getset(get_copy = "pub")]
    list: &'d rating_target::RatingTargetList,
    #[getset(get = "pub")]
    target_new: Vec<RatingTargetEntry<'d, 's>>,
    #[getset(get = "pub")]
    target_old: Vec<RatingTargetEntry<'d, 's>>,
    #[getset(get = "pub")]
    candidates_new: Vec<RatingTargetEntry<'d, 's>>,
    #[getset(get = "pub")]
    candidates_old: Vec<RatingTargetEntry<'d, 's>>,
}

#[derive(Getters, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct RatingTargetEntry<'d, 's> {
    data: &'d rating_target::RatingTargetEntry,
    score: OrdinaryScoreForVersionRef<'s>,
}

impl<'d, 's> UserData<'d, 's> {
    pub fn annotate(
        database: &SongDatabase<'s>,
        user_data: &'d MaimaiUserData,
    ) -> anyhow::Result<Self> {
        let records = user_data
            .records
            .iter()
            .map(|(&date, record)| anyhow::Ok((date, PlayRecord::annotate(database, record)?)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            records,
            rating_target: todo!(),
        })
    }
}

impl<'d, 's> PlayRecord<'d, 's> {
    pub fn annotate(
        database: &SongDatabase<'s>,
        record: &'d schema::PlayRecord,
    ) -> anyhow::Result<Self> {
        let song = database.song_from_icon(record.song_metadata().cover_art())?;
        let score = if let Some(utage) = record.utage_metadata() {
            let kind: UtageKindRaw = utage.kind().to_owned().into();
            let candidates = song
                .utage_scores()
                .filter(|score| score.score().kanji() == &kind)
                .collect_vec();
            match candidates[..] {
                [_] => ScoreForVersionRef::Utage(candidates[0]),
                _ => bail!("Utage score could not be determined uniquely: {candidates:?}"),
            }
        } else {
            let version =
                MaimaiVersion::of_time(record.played_at().time().into()).with_context(|| {
                    format!(
                        "Record played at {:?} found, but there is no corresponding version",
                        record.played_at().time()
                    )
                })?;
            let generation = record.score_metadata().generation();
            let scores = song
                .scores(generation)
                .with_context(|| format!("{song:?} does not have a score for {generation:?}"))?;
            let difficulty = record.score_metadata().difficulty();
            let score = scores.score(difficulty).with_context(|| {
                format!("{song:?} does not have a score for {generation:?} {difficulty:?}")
            })?;
            let score = score.for_version(version).with_context(|| {
                format!(
                    "Record played at {:?} has a score that should never exist at this point",
                    record.played_at()
                )
            })?;
            ScoreForVersionRef::Ordinary(score)
        };
        Ok(Self { record, score })
    }
}

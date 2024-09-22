use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context};
use chrono::NaiveDateTime;
use getset::{CopyGetters, Getters};
use itertools::Itertools;

use crate::maimai::schema::latest::UtageKindRaw;

use super::{
    load_score_level::MaimaiVersion,
    parser::rating_target,
    schema::latest::{self as schema, PlayTime},
    song_list::database::{OrdinaryScoreForVersionRef, ScoreForVersionRef, SongDatabase},
    IdxToIconMap, MaimaiUserData,
};

#[derive(Getters)]
#[getset(get = "pub")]
pub struct UserData<'d, 's> {
    records: BTreeMap<PlayTime, PlayRecord<'d, 's>>,
    rating_target: BTreeMap<PlayTime, RatingTargetList<'d, 's>>,
}
impl<'d, 's> UserData<'d, 's> {
    pub fn ordinary_data_associated(&self) -> anyhow::Result<UserDataOrdinaryAssociated<'d, 's>> {
        let ordinary_records = self
            .records()
            .values()
            .filter_map(|r| Some(r.as_ordinary()?.into_associated()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("{e:#?}"))?;
        let rating_target = self
            .rating_target()
            .iter()
            .map(|(&time, r)| Ok((time, r.as_associated()?)))
            .collect::<Result<Vec<_>, &anyhow::Error>>()
            .map_err(|e| anyhow!("{e:#?}"))?;
        Ok(UserDataOrdinaryAssociated {
            ordinary_records,
            rating_target,
        })
    }
}

#[derive(Getters)]
#[getset(get = "pub")]
pub struct UserDataOrdinaryAssociated<'d, 's> {
    ordinary_records: Vec<OrdinaryPlayRecordAssociated<'d, 's>>,
    rating_target: Vec<(PlayTime, RatingTargetListAssociated<'d, 's>)>,
}

#[derive(CopyGetters)]
pub struct PlayRecord<'d, 's> {
    #[getset(get_copy = "pub")]
    record: &'d schema::PlayRecord,
    score: anyhow::Result<ScoreForVersionRef<'s>>,
}
impl<'d, 's> PlayRecord<'d, 's> {
    pub fn score(&self) -> Result<ScoreForVersionRef<'s>, &anyhow::Error> {
        self.score.as_ref().copied()
    }

    pub fn as_ordinary<'p>(&'p self) -> Option<OrdinaryPlayRecord<'d, 's, 'p>> {
        match self.record.utage_metadata() {
            Some(_) => None,
            None => Some(OrdinaryPlayRecord {
                record: self.record,
                score: match self.score {
                    Ok(ScoreForVersionRef::Ordinary(score)) => Ok(score),
                    Ok(ScoreForVersionRef::Utage(_)) => {
                        panic!("Ordinary record associated with utage score")
                    }
                    Err(ref e) => Err(e),
                },
            }),
        }
    }
}

#[derive(Getters, CopyGetters)]
pub struct OrdinaryPlayRecord<'d, 's, 'p> {
    #[getset(get_copy = "pub")]
    record: &'d schema::PlayRecord,
    #[getset(get = "pub")]
    score: Result<OrdinaryScoreForVersionRef<'s>, &'p anyhow::Error>,
}
impl<'d, 's, 'p> OrdinaryPlayRecord<'d, 's, 'p> {
    pub fn into_associated(
        self,
    ) -> Result<OrdinaryPlayRecordAssociated<'d, 's>, &'p anyhow::Error> {
        self.score.map(move |score| OrdinaryPlayRecordAssociated {
            record: self.record,
            score,
        })
    }
}

#[derive(Clone, Copy, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryPlayRecordAssociated<'d, 's> {
    record: &'d schema::PlayRecord,
    score: OrdinaryScoreForVersionRef<'s>,
}

#[derive(Getters, CopyGetters)]
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
impl<'d, 's> RatingTargetList<'d, 's> {
    pub fn as_associated<'p>(
        &'p self,
    ) -> Result<RatingTargetListAssociated<'d, 's>, &'p anyhow::Error> {
        let convert = |list: &'p [RatingTargetEntry<'d, 's>]| {
            list.iter()
                .map(|r| r.as_associated())
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(RatingTargetListAssociated {
            list: self.list,
            target_new: convert(&self.target_new)?,
            target_old: convert(&self.target_old)?,
            candidates_new: convert(&self.candidates_new)?,
            candidates_old: convert(&self.candidates_old)?,
        })
    }
}

#[derive(Getters, CopyGetters)]
pub struct RatingTargetListAssociated<'d, 's> {
    #[getset(get_copy = "pub")]
    list: &'d rating_target::RatingTargetList,
    #[getset(get = "pub")]
    target_new: Vec<RatingTargetEntryAssociated<'d, 's>>,
    #[getset(get = "pub")]
    target_old: Vec<RatingTargetEntryAssociated<'d, 's>>,
    #[getset(get = "pub")]
    candidates_new: Vec<RatingTargetEntryAssociated<'d, 's>>,
    #[getset(get = "pub")]
    candidates_old: Vec<RatingTargetEntryAssociated<'d, 's>>,
}

#[derive(CopyGetters)]
pub struct RatingTargetEntry<'d, 's> {
    #[getset(get_copy = "pub")]
    data: &'d rating_target::RatingTargetEntry,
    score: anyhow::Result<OrdinaryScoreForVersionRef<'s>>,
}
impl<'d, 's> RatingTargetEntry<'d, 's> {
    pub fn score(&self) -> Result<OrdinaryScoreForVersionRef<'s>, &anyhow::Error> {
        self.score.as_ref().copied()
    }

    pub fn as_associated<'slf>(
        &'slf self,
    ) -> Result<RatingTargetEntryAssociated<'d, 's>, &'slf anyhow::Error> {
        match &self.score {
            &Ok(score) => Ok(RatingTargetEntryAssociated {
                data: self.data,
                score,
            }),
            Err(e) => Err(e),
        }
    }
}

#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct RatingTargetEntryAssociated<'d, 's> {
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
            .map(|(&time, record)| anyhow::Ok((time, PlayRecord::annotate(database, record)?)))
            .collect::<Result<_, _>>()?;
        let rating_target = user_data
            .rating_targets
            .iter()
            .map(|(&time, file)| {
                anyhow::Ok((
                    time,
                    RatingTargetList::annotate(
                        database,
                        file,
                        time.get(),
                        &user_data.idx_to_icon_map,
                    )?,
                ))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            records,
            rating_target,
        })
    }
}

impl<'d, 's> PlayRecord<'d, 's> {
    pub fn annotate(
        database: &SongDatabase<'s>,
        record: &'d schema::PlayRecord,
    ) -> anyhow::Result<Self> {
        let song = database.song_from_icon(record.song_metadata().cover_art())?;
        let score = (|| {
            if let Some(utage) = record.utage_metadata() {
                let kind: UtageKindRaw = utage.kind().to_owned().into();
                let candidates = song
                    .utage_scores()
                    .filter(|score| score.score().kanji() == &kind)
                    .collect_vec();
                match candidates[..] {
                    [_] => Ok(ScoreForVersionRef::Utage(candidates[0])),
                    _ => bail!("Utage score could not be determined uniquely: {candidates:?}"),
                }
            } else {
                let version = MaimaiVersion::of_time(record.played_at().time().into())
                    .with_context(|| format!("Record has no corresponding version: {record:?}",))?;
                let generation = record.score_metadata().generation();
                let scores = song.scores(generation).with_context(|| {
                    format!("{song:?} does not have a score for {generation:?}")
                })?;
                let difficulty = record.score_metadata().difficulty();
                let score = scores.score(difficulty).with_context(|| {
                    format!("{song:?} does not have a score for {generation:?} {difficulty:?}")
                })?;
                let score = score.for_version(version).with_context(|| {
                    format!("Record has a score that should never exist at this point: {record:?}",)
                })?;
                Ok(ScoreForVersionRef::Ordinary(score))
            }
        })();
        Ok(Self { record, score })
    }
}

impl<'d, 's> RatingTargetList<'d, 's> {
    pub fn annotate(
        database: &SongDatabase<'s>,
        list: &'d rating_target::RatingTargetList,
        time: NaiveDateTime,
        idx_map: &IdxToIconMap,
    ) -> anyhow::Result<Self> {
        let version = MaimaiVersion::of_time(time).with_context(|| {
            format!("Target list as of {time:?} found, but there is no corresponding version")
        })?;
        let parse = |entries: &'d Vec<rating_target::RatingTargetEntry>| {
            entries
                .iter()
                .map(|entry| RatingTargetEntry::annotate(database, version, entry, idx_map))
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("Failed to parse rating target list as of {time:?}"))
        };
        Ok(Self {
            list,
            target_new: parse(list.target_new())?,
            target_old: parse(list.target_old())?,
            candidates_new: parse(list.candidates_new())?,
            candidates_old: parse(list.candidates_old())?,
        })
    }
}

impl<'d, 's> RatingTargetEntry<'d, 's> {
    pub fn annotate(
        database: &SongDatabase<'s>,
        version: MaimaiVersion,
        data: &'d rating_target::RatingTargetEntry,
        idx_map: &IdxToIconMap,
    ) -> anyhow::Result<Self> {
        let score = (|| {
            let song = match database.song_from_name(data.song_name()).collect_vec()[..] {
                [song] => song,
                ref songs => match idx_map
                    .get(data.idx())
                    .with_context(|| format!("Idx not registered: {:?}", data.idx()))
                    .and_then(|icon| database.song_from_icon(icon))
                {
                    Ok(song) => song,
                    Err(e) => {
                        return Err(e.context(format!(
                            "Song cannot be uniquely determiend from song name {:?}: {:?}",
                            data.song_name(),
                            songs
                        )))
                    }
                },
            };
            let generation = data.score_metadata().generation();
            let scores = song
                .scores(generation)
                .with_context(|| format!("{song:?} does not have a score for {generation:?}"))?;
            let difficulty = data.score_metadata().difficulty();
            let score = scores.score(difficulty).with_context(|| {
                format!("{song:?} does not have a score for {generation:?} {difficulty:?}")
            })?;
            let score = score.for_version(version).with_context(|| {
                format!("Found rating target entry with a score that should never exist at this point: {data:?}")
            })?;
            Ok(score)
        })();
        Ok(Self { data, score })
    }
}

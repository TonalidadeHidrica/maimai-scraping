use std::{collections::BTreeMap, iter::successors, path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail, Context};
use clap::Parser;
use enum_iterator::Sequence;
use hashbrown::{hash_map::Entry as HEntry, HashMap};
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::info;
use maimai_scraping::maimai::{
    estimate_rating::ScoreKey,
    load_score_level::{self, InternalScoreLevel, MaimaiVersion},
    rating::{ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon},
    song_list::{OrdinaryScores, Song, SongAbbreviation},
};
use maimai_scraping_utils::fs_json_util::read_json;
use maimai_scraping_utils::regex;
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    in_lv_dir: PathBuf,
    in_lv_data_dir: PathBuf,
    save_json: PathBuf,
    additional_nicknames: Option<PathBuf>,
}

#[derive(Default)]
/// Collects the resources for the song list.
struct Resources {
    in_lv: BTreeMap<MaimaiVersion, Vec<load_score_level::Song>>,
    in_lv_data: BTreeMap<MaimaiVersion, InLvData>,
    additional_nicknames: Vec<(String, SongIcon)>,
}
impl Resources {
    fn load(opts: &Opts) -> anyhow::Result<Self> {
        let mut ret = Resources::default();

        // Read in_lv
        for version in successors(Some(MaimaiVersion::Festival), MaimaiVersion::next) {
            let path = format!("{}.json", i8::from(version));
            let levels = load_score_level::load(opts.in_lv_dir.join(path))?;
            ret.in_lv.insert(version, levels);
        }

        // Read in_lv_data
        for version in successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next) {
            info!("Processing {version:?}");
            let path = format!("{}.json", i8::from(version));
            let data: InLvData = read_json(opts.in_lv_data_dir.join(path))?;
            ret.in_lv_data.insert(version, data);
        }

        // Read additional_nicknames
        ret.additional_nicknames = opts
            .additional_nicknames
            .as_ref()
            .map_or_else(|| Ok(vec![]), read_json)?;

        Ok(ret)
    }
}

/// Accumulates the actual song list as well as look up tables.
#[derive(Default)]
struct Results {
    songs: SongList,
    icon_to_song: HashMap<SongIcon, SongIndex>,
    abbrev_to_song: HashMap<SongAbbreviation, SongIndex>,
}

impl Results {
    fn read_in_lv(
        &mut self,
        version: MaimaiVersion,
        in_lv: &[load_score_level::Song],
    ) -> anyhow::Result<()> {
        // generation: ScoreGeneration,
        // version: MaimaiVersion,
        // levels: ScoreLevels,
        // song_name: SongName,
        // song_name_abbrev: String,
        // icon: SongIcon,
        for data in in_lv {
            // Use `icon` and `song_name`
            let (index, song) = match self.icon_to_song.entry(data.icon().to_owned()) {
                HEntry::Occupied(i) => {
                    let index = *i.get();
                    let song = self.songs.get_mut(index);
                    if &song.name != data.song_name() || song.icon.as_ref() != Some(data.icon()) {
                        bail!("Inconsistent song name or icon: song = {song:?}, data = {data:?}");
                    }
                    (index, song)
                }
                HEntry::Vacant(e) => {
                    let index = self.songs.index_new();
                    self.songs.0.push(Song {
                        name: data.song_name().to_owned(),
                        category: None,
                        artist: None,
                        pronunciation: None,
                        abbreviation: Default::default(),
                        scores: Default::default(),
                        icon: Some(data.icon().to_owned()),
                    });
                    e.insert(index);
                    (index, self.songs.get_mut(index))
                }
            };

            // Update abbreviation map, check if contradiction occurs
            let abbrev: SongAbbreviation = data.song_name_abbrev().to_owned().into();
            match self.abbrev_to_song.entry(abbrev.clone()) {
                HEntry::Occupied(i) => {
                    if *i.get() != index {
                        bail!(
                            "At least two songs are associated to nickname {abbrev:?}: {:?} and {:?}",
                            self.songs.get(index),
                            self.songs.get(*i.get()),
                        )
                    }
                }
                HEntry::Vacant(e) => {
                    e.insert(index);
                }
            }

            // Record `song_name_abbrev`
            song.abbreviation[version] = Some(abbrev.clone());

            // Record `levels` (indexed by `generation` and `version`)
            let scores = song.scores[data.generation()].get_or_insert_with(|| OrdinaryScores {
                easy: None,
                basic: Default::default(),
                advanced: Default::default(),
                expert: Default::default(),
                master: Default::default(),
                re_master: None,
                version: data.version(),
            });
            scores.basic.levels[version] = Some(data.levels().get(ScoreDifficulty::Basic).unwrap());
            scores.advanced.levels[version] =
                Some(data.levels().get(ScoreDifficulty::Advanced).unwrap());
            scores.expert.levels[version] =
                Some(data.levels().get(ScoreDifficulty::Expert).unwrap());
            scores.master.levels[version] =
                Some(data.levels().get(ScoreDifficulty::Master).unwrap());
            if let Some(level) = data.levels().get(ScoreDifficulty::ReMaster) {
                scores.re_master.get_or_insert_with(Default::default).levels[version] = Some(level);
            }
        }
        Ok(())
    }

    fn read_in_lv_data(&mut self, version: MaimaiVersion, data: &InLvData) -> anyhow::Result<()> {
        // Process unknown songs.
        // Remove entry once process so that no data unprocessed is left.
        let mut unknown: HashMap<_, _> = data.unknown.iter().collect();
        if !unknown
            .remove(&UnknownKey::gen("14".parse()?))
            .is_some_and(|x| x.is_empty())
        {
            bail!("Lv.14 is not empty");
        }
        for level in 10..14 {
            for plus in [false, true] {
                let level = ScoreLevel::new(level, plus)?;
                let data = unknown
                    .remove(&UnknownKey::gen(level))
                    .with_context(|| format!("No unknown entry found for {level}"))?;
                for entry in data {
                    let entry = entry.parse()?;
                    let missing_song = || format!("Missing song: {:?}", entry.entry);

                    let song = self.songs.get_mut(
                        *self
                            .abbrev_to_song
                            .get(&entry.entry.song_nickname)
                            .with_context(missing_song)?,
                    );
                    let scores = song.scores[entry.entry.generation()]
                        .as_mut()
                        .with_context(missing_song)?;
                    let mut set = |difficulty, level| {
                        merge_levels(
                            &mut scores
                                .get_score_mut(difficulty)
                                .with_context(missing_song)?
                                .levels[version],
                            InternalScoreLevel::Unknown(level),
                            version,
                        )?;
                        anyhow::Ok(())
                    };
                    set(entry.entry.difficulty, level)?;
                    for (difficulty, level) in entry.additional {
                        set(difficulty, level)?;
                    }

                    // if let Some(icon) = songs.get(&entry.entry.song_nickname) {
                    //     let key = entry.entry.score_key(icon);
                    //     res.entry(key)
                    //         .or_default()
                    //         .insert(version, InternalScoreLevel::Unknown(level));
                    //     for (difficulty, level) in entry.additional {
                    //         let key = ScoreKey { difficulty, ..key };
                    //         res.entry(key)
                    //             .or_default()
                    //             .insert(version, InternalScoreLevel::Unknown(level));
                    //     }
                    // } else {
                    //     warn!("Missing song: {:?}", entry.entry.song_nickname);
                    // }
                }
            }
        }
        if !unknown.is_empty() {
            bail!("Additional data found: {:?}", unknown);
        }

        // Process known songs.
        // Remove entry once process so that no data unprocessed is left.
        let mut known: HashMap<_, _> = data.known.iter().collect();
        for level in 5..=15 {
            let data = known
                .remove(&KnownKey::gen(level))
                .with_context(|| format!("No known entry found for {level}"))?;
            let expected_len = if level == 15 { 1 } else { 10 };
            if data.len() != expected_len {
                bail!(
                    "Unexpected length for level {level}: expected {expected_len}, found {}",
                    data.len()
                );
            }
            for (entries, fractional) in data.iter().rev().zip(0..) {
                let level = ScoreConstant::try_from(level * 10 + fractional)
                    .map_err(|e| anyhow!("Unexpected internal lv: {e}"))?;
                for entry in entries {
                    let entry = entry.parse()?;
                    let missing_song = || format!("Missing song: {:?}", entry);

                    let song = self.songs.get_mut(
                        *self
                            .abbrev_to_song
                            .get(&entry.song_nickname)
                            .with_context(missing_song)?,
                    );
                    let scores = song.scores[entry.generation()]
                        .as_mut()
                        .with_context(missing_song)?;
                    merge_levels(
                        &mut scores
                            .get_score_mut(entry.difficulty)
                            .with_context(missing_song)?
                            .levels[version],
                        InternalScoreLevel::Known(level),
                        version,
                    )?;

                    // if let Some(icon) = songs.get(&entry.song_nickname) {
                    //     res.entry(entry.score_key(icon))
                    //         .or_default()
                    //         .insert(version, InternalScoreLevel::Known(level));
                    // } else {
                    //     warn!("Missing song: {:?}", entry.song_nickname);
                    // }
                }
            }
        }
        if !known.is_empty() {
            bail!("Additional data found: {:?}", data.unknown);
        }

        Ok(())
    }
}

fn merge_levels(
    x: &mut Option<InternalScoreLevel>,
    y: InternalScoreLevel,
    version: MaimaiVersion,
) -> anyhow::Result<()> {
    enum Verdict {
        Assign,
        Keep,
        Inconsistent(InternalScoreLevel),
    }
    use InternalScoreLevel::*;
    use Verdict::*;
    let verdict = match x {
        None => Assign,
        &mut Some(x0) => match (x0, y) {
            (Unknown(x), Unknown(y)) => {
                if x == y {
                    Keep
                } else {
                    Inconsistent(x0)
                }
            }
            (Unknown(x), Known(y)) => {
                if y.to_lv(version) == x {
                    Assign
                } else {
                    Inconsistent(x0)
                }
            }
            (Known(x), Unknown(y)) => {
                if x.to_lv(version) == y {
                    Keep
                } else {
                    Inconsistent(x0)
                }
            }
            (Known(x), Known(y)) => {
                if x == y {
                    Keep
                } else {
                    Inconsistent(x0)
                }
            }
        },
    };
    if let Assign = verdict {
        *x = Some(y);
    }
    if let Inconsistent(x) = verdict {
        bail!("Inconsitent levels: known to be {x:?}, found {y:?}")
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct SongList(Vec<Song>);
#[derive(Clone, Copy, PartialEq, Eq)]
/// Virtual pointer to an element of `Results::songs`.
struct SongIndex(usize);
impl SongList {
    fn get(&self, index: SongIndex) -> &Song {
        &self.0[index.0]
    }
    fn get_mut(&mut self, index: SongIndex) -> &mut Song {
        &mut self.0[index.0]
    }
    fn index_new(&self) -> SongIndex {
        SongIndex(self.0.len())
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let resources = Resources::load(&opts)?;
    let mut results = Results::default();
    for (&version, in_lv) in &resources.in_lv {
        results.read_in_lv(version, in_lv)?;
    }
    for (&version, in_lv_data) in &resources.in_lv_data {
        results.read_in_lv_data(version, in_lv_data)?;
    }

    // for version in successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next) {
    //     info!("Processing {version:?}");
    //     let path = format!("{}.json", i8::from(version));
    //     let mut data: InLvData = read_json(opts.in_lv_data_dir.join(path))?;

    //     if !data
    //         .unknown
    //         .remove(&UnknownKey::gen("14".parse()?))
    //         .is_some_and(|x| x.is_empty())
    //     {
    //         bail!("Lv.14 is not empty");
    //     }
    //     for level in 10..14 {
    //         for plus in [false, true] {
    //             let level = ScoreLevel::new(level, plus)?;
    //             let data = data
    //                 .unknown
    //                 .remove(&UnknownKey::gen(level))
    //                 .with_context(|| format!("No unknown entry found for {level}"))?;
    //             for entry in data {
    //                 let entry = entry.parse()?;
    //                 if let Some(icon) = songs.get(&entry.entry.song_nickname) {
    //                     let key = entry.entry.score_key(icon);
    //                     res.entry(key)
    //                         .or_default()
    //                         .insert(version, InternalScoreLevel::Unknown(level));
    //                     for (difficulty, level) in entry.additional {
    //                         let key = ScoreKey { difficulty, ..key };
    //                         res.entry(key)
    //                             .or_default()
    //                             .insert(version, InternalScoreLevel::Unknown(level));
    //                     }
    //                 } else {
    //                     warn!("Missing song: {:?}", entry.entry.song_nickname);
    //                 }
    //             }
    //         }
    //     }
    //     if !data.unknown.is_empty() {
    //         bail!("Additional data found: {:?}", data.unknown);
    //     }

    //     for level in 5..=15 {
    //         let data = data
    //             .known
    //             .remove(&KnownKey::gen(level))
    //             .with_context(|| format!("No known entry found for {level}"))?;
    //         let expected_len = if level == 15 { 1 } else { 10 };
    //         if data.len() != expected_len {
    //             bail!(
    //                 "Unexpected length for level {level}: expected {expected_len}, found {}",
    //                 data.len()
    //             );
    //         }
    //         for (entries, fractional) in data.iter().rev().zip(0..) {
    //             let level = ScoreConstant::try_from(level * 10 + fractional)
    //                 .map_err(|e| anyhow!("Unexpected internal lv: {e}"))?;
    //             for entry in entries {
    //                 let entry = entry.parse()?;
    //                 if let Some(icon) = songs.get(&entry.song_nickname) {
    //                     res.entry(entry.score_key(icon))
    //                         .or_default()
    //                         .insert(version, InternalScoreLevel::Known(level));
    //                 } else {
    //                     warn!("Missing song: {:?}", entry.song_nickname);
    //                 }
    //             }
    //         }
    //     }
    //     if !data.known.is_empty() {
    //         bail!("Additional data found: {:?}", data.unknown);
    //     }
    // }

    // write_json(opts.save_json, &res.iter().collect_vec())?;

    Ok(())
}

#[derive(Debug, Deserialize)]
struct InLvData {
    unknown: HashMap<UnknownKey, Vec<UnknownValue>>,
    known: HashMap<KnownKey, Vec<Vec<KnownValue>>>,
}

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
struct KnownKey(String);
impl KnownKey {
    fn gen(level: u8) -> Self {
        Self(format!("lv{level}_rslt"))
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
struct KnownValue(String);
impl KnownValue {
    fn parse(&self) -> anyhow::Result<Entry> {
        let entry = parse_entry(&self.0)?;
        if !entry.additional.is_empty() {
            bail!("Unexpected additional data found in {self:?}");
        }
        Ok(entry.entry)
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
struct UnknownKey(String);
impl UnknownKey {
    fn gen(level: ScoreLevel) -> Self {
        let pm = if level.plus { 'p' } else { 'm' };
        Self(format!("lv{}{pm}", level.level))
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Deserialize)]
struct UnknownValue(String);
impl UnknownValue {
    fn parse(&self) -> anyhow::Result<EntryWithAdditional> {
        parse_entry(&self.0)
    }
}

fn parse_difficulty(s: &str) -> anyhow::Result<ScoreDifficulty> {
    use ScoreDifficulty::*;
    Ok(match s {
        "b" => Basic,
        "a" => Advanced,
        "e" => Expert,
        "m" => Master,
        "r" => ReMaster,
        _ => bail!("Unexpected difficulty: {s:?}"),
    })
}
fn difficulty_char(difficulty: ScoreDifficulty) -> char {
    use ScoreDifficulty::*;
    match difficulty {
        Basic => 'b',
        Advanced => 'a',
        Expert => 'e',
        Master => 'm',
        ReMaster => 'r',
        Utage => 'u',
    }
}

#[derive(Debug)]
struct EntryWithAdditional {
    entry: Entry,
    additional: Vec<(ScoreDifficulty, ScoreLevel)>,
}
#[derive(Debug)]
struct Entry {
    difficulty: ScoreDifficulty,
    #[allow(unused)]
    new_song: bool,
    song_nickname: SongAbbreviation,
    dx: bool,
}
fn parse_entry(s: &str) -> anyhow::Result<EntryWithAdditional> {
    let pattern = regex!(
        r#"(?x)
            <span\ class='wk_(?<difficulty>[baemr])'>
                (?<new_song> <u>)?
                    (?<song_name> .*?)
                    (?<dx> \[dx\])?
                (</u>)?
            </span>
            (
                \( (?<additional> [^)]* ) \)
            )?
            "#
    );
    let captures = pattern.captures(s).context("Unexpected string: {self:?}")?;
    let difficulty = parse_difficulty(&captures["difficulty"])?;
    let new_song = captures.name("new_song").is_some();
    let song_nickname = captures["song_name"].to_owned().into();
    let dx = captures.name("dx").is_some();
    let additional = match captures.name("additional") {
        None => vec![],
        Some(got) => {
            let pattern = regex!(
                r#"(?x)
                    <span\ class='wk_(?<difficulty>[baemr])'>
                        (?<level> .*)
                    </span>
                    "#
            );
            let mut res = vec![];
            for element in got.as_str().split(',') {
                let captures = pattern
                    .captures(element)
                    .with_context(|| format!("Unexpected additional string: {element:?}"))?;
                let difficulty = parse_difficulty(&captures["difficulty"])?;
                let level = ScoreLevel::from_str(&captures["level"])?;
                res.push((difficulty, level));
            }
            res
        }
    };

    let reconstruct = {
        let additional_is_empty = additional.is_empty();
        let make_additional = || {
            additional
                .iter()
                .map(|&(d, lv)| {
                    lazy_format!("<span class='wk_{d}'>{lv}</span>", d = difficulty_char(d))
                })
                .join_with(',')
        };
        let additional = lazy_format!(
            if additional_is_empty => ""
            else => ("({})", make_additional())
        );
        format!(
            "<span class='wk_{d}'>{us}{song_nickname}{dx}{ut}</span>{additional}",
            d = difficulty_char(difficulty),
            us = if new_song { "<u>" } else { "" },
            dx = if dx { "[dx]" } else { "" },
            ut = if new_song { "</u>" } else { "" },
        )
    };
    if s != reconstruct {
        bail!("Input: {s:?}, reconstructed: {reconstruct:?}")
    }
    Ok(EntryWithAdditional {
        entry: Entry {
            difficulty,
            new_song,
            song_nickname,
            dx,
        },
        additional,
    })
}
impl Entry {
    // fn score_key<'a>(&self, icon: &'a SongIcon) -> ScoreKey<'a> {
    //     ScoreKey {
    //         icon,
    //         generation: self.generation(),
    //         difficulty: self.difficulty,
    //     }
    // }

    fn generation(&self) -> ScoreGeneration {
        if self.dx {
            ScoreGeneration::Deluxe
        } else {
            ScoreGeneration::Standard
        }
    }
}

use std::{
    collections::BTreeMap,
    iter::successors,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use clap::Parser;
use enum_iterator::Sequence;
use enum_map::EnumMap;
use fs_err::read_to_string;
use hashbrown::{hash_map::Entry as HEntry, HashMap};
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::info;
use maimai_scraping::maimai::{
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
    database_dir: PathBuf,
    save_json: PathBuf,
    additional_nicknames: Option<PathBuf>,
}

#[derive(Default)]
/// Collects the resources for the song list.
struct Resources {
    in_lv: BTreeMap<MaimaiVersion, Vec<load_score_level::Song>>,
    in_lv_data: BTreeMap<MaimaiVersion, InLvData>,
    removed_songs_wiki: RemovedSongsWiki,
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

        ret.removed_songs_wiki =
            RemovedSongsWiki::read(opts.database_dir.join("removed_songs_wiki.txt"))?;

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
    /// This function is to be called at most once per version.
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
                    (index, song)
                }
                HEntry::Vacant(e) => {
                    let index = self.songs.index_new();
                    self.songs.0.push(Song {
                        name: EnumMap::default(),
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
            song.name[version] = Some(data.song_name().to_owned());

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

#[derive(Default, Debug)]
struct RemovedSongsWiki {
    songs: Vec<RemovedSongWiki>,
}
#[derive(Debug)]
struct RemovedSongWiki {
    date: String,
    genre: String,
    data: [String; 10],
    another: Option<[String; 10]>,
}

impl RemovedSongsWiki {
    fn read(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut songs: Vec<RemovedSongWiki> = vec![];
        let mut current_genre = None;
        let mut current_date = None;

        let text = read_to_string(path)?;
        for line in text.lines().filter_map(|s| s.strip_prefix('|')) {
            let skip = [
                "T:100%|c",
                "center:40|center:230|center:200|CENTER:16|CENTER:17|CENTER:18|CENTER:25|CENTER:25|CENTER:25|center:30|c",
                "center:40|center:230|center:200|CENTER:16|CENTER:17|CENTER:18|CENTER:25|CENTER:25|CENTER:25|center:30|center:40|c",
                "!''ジャンル''|!''曲名''|!''アーティスト''|>|>|>|>|>|!center:''難易度''|!''BPM''|",
                "!''ジャンル''|!''曲名''|!''アーティスト''|>|>|>|>|>|!center:''難易度''|!''BPM''|!''収録日''|",
                "^|^|^|bgcolor(#00ced1):''&color(gray){Ea}''|bgcolor(#98fb98):''Ba''|bgcolor(#ffa500):''Ad''|bgcolor(#fa8080):''Ex''|bgcolor(#ee82ee):''Ma''|bgcolor(#ffceff):''Re:''|^|",
                "^|^|^|bgcolor(#00ced1):''Ea''|bgcolor(#98fb98):''Ba''|bgcolor(#ffa500):''Ad''|bgcolor(#fa8080):''Ex''|bgcolor(#ee82ee):''Ma''|bgcolor(#ffceff):''Re:''|^|^|",
                "center:|center:|center:|center:bgcolor(#87ceee)|bgcolor(#c0ff20):center|bgcolor(#ffe080):center|bgcolor(#ffa0c0):center|bgcolor(#e2a9f3):center|bgcolor(#ffdeff):center|c",
                "center:|center:|center:|center:bgcolor(#87ceee)|bgcolor(#c0ff20):center|bgcolor(#ffe080):center|bgcolor(#ffa0c0):center|bgcolor(#E2A9F3):center|bgcolor(#ffdeff):center|center:|center:|c"
            ];
            if skip.iter().any(|&s| s == line) {
                continue;
            }

            let p = regex!(
                r"^(>\|){9,10}LEFT:''(【(?<version>.*) アップデート】 )?(?<date>\d+/\d+/\d+) - \d+曲\d+譜面(\(内.*\))?''\|$"
            );
            if let Some(captures) = p.captures(line) {
                let _version = captures.name("version").map(|p| p.as_str());
                let date = captures.name("date").unwrap().as_str();
                current_date = Some(date);
            } else {
                let row = line.split('|').collect_vec();
                if ![2, 11, 12].iter().any(|&x| x == row.len()) {
                    bail!("Unexpected number of rows: {row:?}");
                }
                if row[0] != "^" {
                    let p = regex!(r"^bgcolor\(#[0-9a-f]{6}\):''(.*)''$");
                    let genre = p
                        .captures(row[0])
                        .with_context(|| format!("Unexpected genre: {:?}", row[0]))?
                        .get(1)
                        .unwrap()
                        .as_str();
                    current_genre = Some(genre);
                }
                if row.len() == 2 || current_genre == Some("宴") {
                    continue;
                }
                let data: [&str; 10] = row[1..11].try_into().unwrap();
                let data = data.map(|s| s.to_owned());
                if data[8].ends_with("復活") {
                    continue;
                } else if data[0] == "^" {
                    songs
                        .last_mut()
                        .with_context(|| format!("Unexpected continued `^`: {line:?}"))?
                        .another = Some(data);
                } else {
                    songs.push(RemovedSongWiki {
                        date: current_date.context("Date missing")?.to_owned(),
                        genre: current_genre.context("Genre missing")?.to_owned(),
                        data: data.map(|s| s.to_owned()),
                        another: None,
                    });
                }
            }
        }
        for song in &songs {
            println!("{song:?}");
        }

        Ok(Self { songs })
    }
}

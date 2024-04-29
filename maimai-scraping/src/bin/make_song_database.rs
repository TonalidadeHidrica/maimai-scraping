use std::{collections::BTreeMap, iter::successors, path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail, Context};
use clap::Parser;
use enum_iterator::Sequence;
use hashbrown::HashMap;
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::{info, warn};
use maimai_scraping::{
    fs_json_util::{read_json, write_json},
    maimai::{
        estimate_rating::ScoreKey,
        load_score_level::{self, make_hash_multimap, InternalScoreLevel, MaimaiVersion},
        rating::{ScoreConstant, ScoreLevel},
        schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon},
    },
    regex,
};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    in_lv_dir: PathBuf,
    in_lv_data_dir: PathBuf,
    save_json: PathBuf,
    additional_nickname: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let mut version_to_levels = BTreeMap::new();
    for version in successors(Some(MaimaiVersion::Festival), MaimaiVersion::next) {
        let path = format!("{}.json", i8::from(version));
        let levels = load_score_level::load(opts.in_lv_dir.join(path))?;
        version_to_levels.insert(version, levels);
    }
    let additional_nickname: Vec<(String, SongIcon)> = opts
        .additional_nickname
        .map_or_else(|| Ok(vec![]), read_json)?;
    let songs = make_hash_multimap(
        version_to_levels
            .values()
            .flatten()
            .map(|song| (song.song_name_abbrev(), song.icon()))
            .chain(additional_nickname.iter().map(|(x, y)| (x, y))),
    );
    let songs = songs
        .into_iter()
        .map(|(k, v)| match v.into_iter().all_equal_value() {
            Ok(v) => Ok((k, v)),
            Err(Some((x, y))) => {
                bail!("At least two songs are associated to nickname {k:?}: {x} and {y}")
            }
            Err(None) => unreachable!(),
        })
        .collect::<Result<HashMap<_, _>, _>>()?;

    let mut res = HashMap::<ScoreKey, BTreeMap<MaimaiVersion, InternalScoreLevel>>::new();

    for version in successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next) {
        info!("Processing {version:?}");
        let path = format!("{}.json", i8::from(version));
        let mut data: InLvData = read_json(opts.in_lv_data_dir.join(path))?;

        if !data
            .unknown
            .remove(&UnknownKey::gen("14".parse()?))
            .is_some_and(|x| x.is_empty())
        {
            bail!("Lv.14 is not empty");
        }
        for level in 10..14 {
            for plus in [false, true] {
                let level = ScoreLevel::new(level, plus)?;
                let data = data
                    .unknown
                    .remove(&UnknownKey::gen(level))
                    .with_context(|| format!("No unknown entry found for {level}"))?;
                for entry in data {
                    let entry = entry.parse()?;
                    if let Some(icon) = songs.get(&entry.entry.song_nickname) {
                        let key = entry.entry.score_key(icon);
                        res.entry(key)
                            .or_default()
                            .insert(version, InternalScoreLevel::Unknown(level));
                        for (difficulty, level) in entry.additional {
                            let key = ScoreKey { difficulty, ..key };
                            res.entry(key)
                                .or_default()
                                .insert(version, InternalScoreLevel::Unknown(level));
                        }
                    } else {
                        warn!("Missing song: {:?}", entry.entry.song_nickname);
                    }
                }
            }
        }
        if !data.unknown.is_empty() {
            bail!("Additional data found: {:?}", data.unknown);
        }

        for level in 5..=15 {
            let data = data
                .known
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
                    if let Some(icon) = songs.get(&entry.song_nickname) {
                        res.entry(entry.score_key(icon))
                            .or_default()
                            .insert(version, InternalScoreLevel::Known(level));
                    } else {
                        warn!("Missing song: {:?}", entry.song_nickname);
                    }
                }
            }
        }
        if !data.known.is_empty() {
            bail!("Additional data found: {:?}", data.unknown);
        }
    }

    write_json(opts.save_json, &res.iter().collect_vec())?;

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

struct EntryWithAdditional {
    entry: Entry,
    additional: Vec<(ScoreDifficulty, ScoreLevel)>,
}
struct Entry {
    difficulty: ScoreDifficulty,
    #[allow(unused)]
    new_song: bool,
    song_nickname: String,
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
    let song_nickname = captures["song_name"].to_owned();
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
    fn score_key<'a>(&self, icon: &'a SongIcon) -> ScoreKey<'a> {
        ScoreKey {
            icon,
            generation: if self.dx {
                ScoreGeneration::Deluxe
            } else {
                ScoreGeneration::Standard
            },
            difficulty: self.difficulty,
        }
    }
}

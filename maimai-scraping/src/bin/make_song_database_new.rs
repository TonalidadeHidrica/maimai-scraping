use std::{
    collections::BTreeMap,
    fmt::Debug,
    iter::successors,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use clap::Parser;
use enum_iterator::Sequence;
use enum_map::EnumMap;
use fs_err::read_to_string;
use hashbrown::{hash_map::Entry as HEntry, HashMap, HashSet};
use itertools::{chain, EitherOrBoth, Itertools};
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use log::info;
use maimai_scraping::maimai::{
    rating::{InternalScoreLevel, ScoreConstant, ScoreLevel},
    schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
    song_list::{
        database::SongDatabase,
        in_lv::{self, in_lv_kind, Song as InLvSong, SongRaw},
        official::{self, ScoreDetails},
        OrdinaryScore, OrdinaryScores, RemoveState, Song, SongAbbreviation, UtageIdentifier,
    },
    version::MaimaiVersion,
};
use maimai_scraping_utils::{
    fs_json_util::{read_json, write_json},
    regex,
};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    in_lv_dir: PathBuf,
    in_lv_bitmask_dir: PathBuf,
    in_lv_data_dir: PathBuf,
    database_dir: PathBuf,
    official_song_list_paths: Vec<PathBuf>,

    #[arg(long)]
    skip_latest_in_lv: bool,
    #[arg(long)]
    skip_latest_in_lv_bitmask: bool,
    #[arg(long)]
    skip_latest_in_lv_data: bool,
}

type InLvSongMask = InLvSong<in_lv_kind::Bitmask>;
type UtageIdentifierMergeMap =
    HashMap<(SongIcon, UtageIdentifier<'static>), UtageIdentifier<'static>>;
type InLvCorrectionMap<K = in_lv_kind::Levels> = HashMap<
    SongIcon,
    HashMap<(MaimaiVersion, ScoreGeneration, ScoreDifficulty), [<K as in_lv_kind::Kind>::Value; 2]>,
>;
type InLvDataCorrectionMap = HashMap<
    SongIcon,
    HashMap<(MaimaiVersion, ScoreGeneration, ScoreDifficulty), [InternalScoreLevel; 2]>,
>;
#[derive(Default)]
/// Collects the resources for the song list.
struct Resources {
    in_lv: BTreeMap<MaimaiVersion, Vec<InLvSong>>,
    in_lv_bitmask: BTreeMap<MaimaiVersion, Vec<InLvSongMask>>,
    in_lv_data: BTreeMap<MaimaiVersion, InLvData>,
    removed_songs_wiki: RemovedSongsWiki,
    removed_songs_supplemental: Vec<RemovedSongSupplemental>,
    official_song_lists: Vec<OfficialSongList>,
    additional_abbrevs: Vec<(SongAbbreviation, SongName)>,
    utage_identifier_merge: UtageIdentifierMergeMap,
    version_supplemental: Vec<VersionSupplemental>,
    in_lv_override: InLvCorrectionMap,
    in_lv_bitmask_override: InLvCorrectionMap<in_lv_kind::Bitmask>,
    in_lv_data_override: InLvDataCorrectionMap,
}
impl Resources {
    fn load(opts: &Opts) -> anyhow::Result<Self> {
        let mut ret = Resources::default();

        // Read in_lv
        for version in successors(Some(MaimaiVersion::Festival), MaimaiVersion::next) {
            if version == MaimaiVersion::latest() && opts.skip_latest_in_lv {
                continue;
            }
            let path = format!("{}.json", i8::from(version));
            let levels = in_lv::load(opts.in_lv_dir.join(path))?;
            assert!(ret.in_lv.insert(version, levels).is_none());
        }

        // Read in_lv_bitmask
        for version in successors(Some(MaimaiVersion::Prism), MaimaiVersion::next) {
            if version == MaimaiVersion::latest() && opts.skip_latest_in_lv_bitmask {
                continue;
            }
            let path = format!("{}.json", i8::from(version));
            let levels = in_lv::load_mask(opts.in_lv_bitmask_dir.join(path))?;
            assert!(ret.in_lv_bitmask.insert(version, levels).is_none());
        }

        // Read in_lv_data
        for version in successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next) {
            if version == MaimaiVersion::latest() && opts.skip_latest_in_lv_data {
                continue;
            }
            info!("Processing {version:?}");
            let path = format!("{}.json", i8::from(version));
            let data: InLvData = read_json(opts.in_lv_data_dir.join(path))?;
            assert!(ret.in_lv_data.insert(version, data).is_none());
        }

        // Read removed song list from wiki source
        ret.removed_songs_wiki =
            RemovedSongsWiki::read(opts.database_dir.join("removed_songs_wiki.txt"))?;

        // Read supplemental removed song list
        ret.removed_songs_supplemental = read_json(opts.database_dir.join("removed_songs.json"))?;

        // Read official song list json
        for path in &opts.official_song_list_paths {
            let captures = regex!(r"(?x)  ^ [^0-9]*  ( [0-9]{8} ) ( [0-9]{6} )? [^0-9]* $  ")
                .captures(
                    path.file_name()
                        .with_context(|| format!("Invalid path: {path:?}"))?
                        .to_str()
                        .with_context(|| format!("Not a UTF-8 name: {path:?}"))?,
                )
                .with_context(|| format!("Cannot extract timestamp from: {path:?}"))?;
            let date = NaiveDate::parse_from_str(captures.get(1).unwrap().as_str(), "%Y%m%d")
                .with_context(|| format!("Invalid date: {path:?}"))?;
            let time = captures
                .get(2)
                .map(|c| {
                    NaiveTime::parse_from_str(c.as_str(), "%H%M%S")
                        .with_context(|| format!("Invalid time: {path:?}"))
                })
                .transpose()?
                .unwrap_or_else(|| NaiveTime::from_hms_opt(12, 0, 0).unwrap());
            let timestamp = date.and_time(time);
            let songs: Vec<official::SongRaw> = read_json(path)?;
            let list = OfficialSongList {
                timestamp,
                songs: songs.into_iter().map(TryInto::try_into).try_collect()?,
            };
            ret.official_song_lists.push(list)
        }
        // Sort the list by timestamp, and then...
        ret.official_song_lists.sort_by_key(|x| x.timestamp);
        // "debounce" the song list.  Sometimes, the song list are not updated even after the
        // new version starts.  This is a trivial workaround using the heuristic that if the
        // song list is not changed, it is not updated.
        ret.official_song_lists.dedup_by(|x, y| x.songs == y.songs);

        // Read additional_abbrevs
        let abbrevs_path = opts.database_dir.join("additional_abbrevs.json");
        if abbrevs_path.is_file() {
            ret.additional_abbrevs = read_json(&abbrevs_path)?;
        }

        ret.utage_identifier_merge =
            read_json::<_, Vec<(SongIcon, UtageIdentifier, UtageIdentifier)>>(
                opts.database_dir.join("utage_identifier_merge.json"),
            )?
            .into_iter()
            .map(|(a, b, c)| ((a, b), c))
            .collect();

        ret.version_supplemental = read_json(opts.database_dir.join("version_supplemental.json"))?;

        // Read `in_lv` override map
        ret.in_lv_override = read_json::<
            _,
            Vec<(
                SongIcon,
                Vec<((MaimaiVersion, ScoreGeneration, ScoreDifficulty), [f64; 2])>,
            )>,
        >(opts.database_dir.join("in_lv_override.json"))?
        .into_iter()
        .map(|(icon, values)| {
            let map = values
                .into_iter()
                .map(|(key, [x, y])| anyhow::Ok((key, [x.try_into()?, y.try_into()?])))
                .collect::<Result<_, _>>()?;
            anyhow::Ok((icon, map))
        })
        .collect::<Result<_, _>>()?;

        // Read `in_lv_bitmask` override map
        ret.in_lv_bitmask_override =
            read_json::<
                _,
                Vec<(
                    SongIcon,
                    Vec<((MaimaiVersion, ScoreGeneration, ScoreDifficulty), [f64; 2])>,
                )>,
            >(opts.database_dir.join("in_lv_bitmask_override.json"))?
            .into_iter()
            .map(|(icon, values)| {
                let map = values
                    .into_iter()
                    .map(|(key, [x, y])| anyhow::Ok((key, [x.try_into()?, y.try_into()?])))
                    .collect::<Result<_, _>>()?;
                anyhow::Ok((icon, map))
            })
            .collect::<Result<_, _>>()?;

        // Read `in_lv_data` override map
        ret.in_lv_data_override = read_json::<
            _,
            Vec<(
                SongIcon,
                Vec<((MaimaiVersion, ScoreGeneration, ScoreDifficulty), [f64; 2])>,
            )>,
        >(opts.database_dir.join("in_lv_data_override.json"))?
        .into_iter()
        .map(|(icon, values)| {
            let map = values
                .into_iter()
                .map(|(key, [x, y])| {
                    let parse =
                        |x: f64| anyhow::Ok(InternalScoreLevel::from_old(key.0, x.try_into()?));
                    anyhow::Ok((key, [parse(x)?, parse(y)?]))
                })
                .collect::<Result<_, _>>()?;
            anyhow::Ok((icon, map))
        })
        .collect::<Result<_, _>>()?;

        Ok(ret)
    }
}

/// Accumulates the actual song list as well as look up tables.
#[derive(Default)]
struct Results {
    songs: SongList,
    icon_to_song: HashMap<SongIcon, SongIndex>,
    name_to_song: HashMap<SongName, HashSet<SongIndex>>,
    abbrev_to_song: HashMap<SongAbbreviation, SongIndex>,
}

impl Results {
    fn read_official_song_list(
        &mut self,
        list: &OfficialSongList,
        utage_identifier_merge: &UtageIdentifierMergeMap,
    ) -> anyhow::Result<()> {
        let mut found_utage = HashSet::new();

        for data in &list.songs {
            let (index, song) = match self.icon_to_song.entry(data.image().clone()) {
                HEntry::Occupied(e) => {
                    let index = *e.get();
                    let song = self.songs.get_mut(index);
                    (index, song)
                }
                HEntry::Vacant(e) => {
                    let (index, song) = self.songs.create_new();
                    e.insert(index);
                    (index, song)
                }
            };

            (|| {
                let version = MaimaiVersion::of_time(list.timestamp + chrono::Duration::hours(9))
                    .with_context(|| {
                    format!("No matching version for timestamp {:?}", list.timestamp)
                })?;

                // Song name
                let song_name: SongName = match data.details() {
                    ScoreDetails::Ordinary(_) => data.title().clone(),
                    ScoreDetails::Utage(u) => {
                        let name: &str = data.title().as_ref();
                        name.strip_prefix(&format!("[{}]", u.kanji()))
                            .context("Utage score not starting with speicified kanji")?
                            .to_owned()
                            .into()
                        // if let Some(name) = &song.name[version] {
                        //     if format!("[{}]{name}", u.kanji()) != data.title().as_ref() {
                        //         bail!("Unexpected title");
                        //     }
                        // } else {
                        //     self.
                        // }
                    }
                };
                merge_options(&mut song.name[version], Some(&song_name))?;
                self.name_to_song
                    .entry(song_name)
                    .or_default()
                    .insert(index);

                // Song kana
                merge_options(&mut song.pronunciation, Some(data.title_kana()))?;
                // Artist
                merge_options(&mut song.artist[version], Some(data.artist()))?;
                // Icon
                merge_options(&mut song.icon, Some(data.image()))?;
                // Unused: release, sort, new
                song.locked_history.insert(list.timestamp, data.locked());

                if version < data.version().version() {
                    bail!("Conflicting version: song {data:?} found in version {version:?}");
                }

                match data.details() {
                    ScoreDetails::Ordinary(ordinary_data) => {
                        merge_options(
                            &mut song.category[version],
                            Some(&ordinary_data.category()),
                        )?;
                        for (generation, scores_data) in [
                            (ScoreGeneration::Standard, ordinary_data.standard()),
                            (ScoreGeneration::Deluxe, ordinary_data.deluxe()),
                        ] {
                            let Some(scores_data) = scores_data else {
                                continue;
                            };
                            let scores =
                                song.scores[generation].get_or_insert_with(OrdinaryScores::default);
                            if !(ordinary_data.standard().is_some()
                                && ordinary_data.deluxe().is_some())
                            {
                                // If both standard and deluxe scores exist,
                                // then the `release` field may not describe which of them the
                                // release date refers to.
                                // Otherwise, we can determine the release date right now.
                                merge_options(
                                    &mut scores.version,
                                    Some(&data.version().version()),
                                )?;
                            }

                            let lv = |v| InternalScoreLevel::unknown(version, v);
                            merge_levels(
                                &mut scores.basic.levels[version],
                                lv(scores_data.basic()),
                                version,
                            )?;
                            merge_levels(
                                &mut scores.advanced.levels[version],
                                lv(scores_data.advanced()),
                                version,
                            )?;
                            merge_levels(
                                &mut scores.expert.levels[version],
                                lv(scores_data.expert()),
                                version,
                            )?;
                            merge_levels(
                                &mut scores.master.levels[version],
                                lv(scores_data.master()),
                                version,
                            )?;
                            if let Some(level) = scores_data.re_master() {
                                let re_master =
                                    scores.re_master.get_or_insert_with(Default::default);
                                merge_levels(&mut re_master.levels[version], lv(level), version)?;
                            }
                        }
                    }
                    ScoreDetails::Utage(utage_data) => {
                        if utage_identifier_merge
                            .contains_key(&(data.image().clone(), utage_data.identifier()))
                        {
                            // In the future, we need to merge the data, but for now we are just
                            // skipping
                            return Ok(());
                        }
                        if !found_utage.insert((index, utage_data.identifier().to_owned())) {
                            bail!("Duplicate utage scores found: {:?}", data);
                        }
                        let identifier = utage_data.identifier();
                        match song
                            .utage_scores
                            .iter()
                            .find(|u| u.identifier() == identifier)
                        {
                            Some(u) => {
                                if u != utage_data {
                                    bail!(
                                        "Utage score conflict: stored {u:?}, found {utage_data:?}",
                                    );
                                }
                            }
                            None => {
                                song.utage_scores.push(utage_data.clone());
                            }
                        }
                    }
                }
                Ok(())
            })()
            .with_context(|| format!("While incorporating {data:?} into {song:?}"))?;
        }
        Ok(())
    }
}

trait InLvKind: in_lv_kind::Kind {
    fn merge_levels(
        levels: &mut Option<InternalScoreLevel>,
        value: Self::Value,
        version: MaimaiVersion,
    ) -> anyhow::Result<()>;
}
impl InLvKind for in_lv_kind::Levels {
    fn merge_levels(
        levels: &mut Option<InternalScoreLevel>,
        value: Self::Value,
        version: MaimaiVersion,
    ) -> anyhow::Result<()> {
        merge_levels(
            levels,
            InternalScoreLevel::from_old(version, value),
            version,
        )
    }
}
impl InLvKind for in_lv_kind::Bitmask {
    fn merge_levels(
        levels: &mut Option<InternalScoreLevel>,
        value: Self::Value,
        version: MaimaiVersion,
    ) -> anyhow::Result<()> {
        let Some(level) = levels else {
            bail!("Unable to determine base level");
        };
        let new = InternalScoreLevel::new(version, level.into_level(version), value)?;
        merge_levels(levels, new, version)
    }
}

impl Results {
    /// This function is to be called at most once per version.
    fn read_in_lv<K: InLvKind>(
        &mut self,
        version: MaimaiVersion,
        in_lv: &[InLvSong<K>],
        overrides: Option<&InLvCorrectionMap<K>>,
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
                    let (index, song) = self.songs.create_new();
                    e.insert(index);
                    (index, song)
                }
            };
            merge_options(&mut song.icon, Some(data.icon()))?;
            song.name[version] = Some(data.song_name().to_owned());

            Self::read_in_lv_song(
                &mut self.name_to_song,
                &mut self.abbrev_to_song,
                index,
                song,
                version,
                data,
                overrides,
            )?;
        }
        Ok(())
    }

    fn read_in_lv_song<K: InLvKind>(
        name_to_song: &mut HashMap<SongName, HashSet<SongIndex>>,
        abbrev_to_song: &mut HashMap<SongAbbreviation, SongIndex>,
        index: SongIndex,
        song: &mut Song,
        version: MaimaiVersion,
        data: &InLvSong<K>,
        overrides: Option<&InLvCorrectionMap<K>>,
    ) -> Result<(), anyhow::Error> {
        (|| {
            // Update song name map
            name_to_song
                .entry(data.song_name().to_owned())
                .or_default()
                .insert(index);

            // Update abbreviation map, check if contradiction occurs
            let abbrev: SongAbbreviation = data.song_name_abbrev().to_owned().into();
            Self::register_abbrev(abbrev_to_song, &abbrev, index)?;

            // Record `song_name_abbrev`
            song.abbreviation[version] = Some(abbrev.clone());
            let scores = song.scores[data.generation()].get_or_insert_with(OrdinaryScores::default);

            // When `in_lv`'s `v` equals `0`, it means its ジングルベル Std
            // (which is classified to Ver.Maimai);
            // if it's `v` equals `1`, it means it's a song for either Maimai or MaimaiPlus.
            // But mistakenly, these are parsed as Maimai and MaimaiPlus, respectively.
            // In fact, we cannot distinguish from `v` data if it is 1, so we should leave the
            // version blank in this case.
            if !matches!(data.version(), MaimaiVersion::MaimaiPlus) {
                merge_options(&mut scores.version, Some(&data.version()))?;
            }

            // Record `levels` (indexed by `generation` and `version`)
            let get = |difficulty: ScoreDifficulty| {
                let Some(level) = data.levels().get(difficulty) else {
                    return Ok(None);
                };
                let Some(overrides) = overrides else {
                    return Ok(Some(level));
                };
                let key = (version, data.generation(), difficulty);
                let Some(&[before, after]) = overrides.get(data.icon()).and_then(|r| r.get(&key))
                else {
                    return Ok(Some(level));
                };
                if before != level {
                    bail!(
                        "Request to override {key:?} from {before:?} to {after:?}, but found {level:?}"
                    );
                }
                Ok(Some(after))
            };
            K::merge_levels(
                &mut scores.basic.levels[version],
                get(ScoreDifficulty::Basic)?.unwrap(),
                version,
            )?;
            K::merge_levels(
                &mut scores.advanced.levels[version],
                get(ScoreDifficulty::Advanced)?.unwrap(),
                version,
            )?;
            K::merge_levels(
                &mut scores.expert.levels[version],
                get(ScoreDifficulty::Expert)?.unwrap(),
                version,
            )?;
            K::merge_levels(
                &mut scores.master.levels[version],
                get(ScoreDifficulty::Master)?.unwrap(),
                version,
            )?;
            if let Some(level) = get(ScoreDifficulty::ReMaster)? {
                K::merge_levels(
                    &mut scores.re_master.get_or_insert_with(Default::default).levels[version],
                    level,
                    version,
                )?;
            }
            // scores.basic.levels[version] = Some(data.levels().get(ScoreDifficulty::Basic).unwrap());
            // scores.advanced.levels[version] =
            //     Some(data.levels().get(ScoreDifficulty::Advanced).unwrap());
            // scores.expert.levels[version] =
            //     Some(data.levels().get(ScoreDifficulty::Expert).unwrap());
            // scores.master.levels[version] =
            //     Some(data.levels().get(ScoreDifficulty::Master).unwrap());
            // if let Some(level) = data.levels().get(ScoreDifficulty::ReMaster) {
            //     scores.re_master.get_or_insert_with(Default::default).levels[version] = Some(level);
            // }

            anyhow::Ok(())
        })()
        .with_context(|| {
            format!("While incorporating {data:?} of in_lv version {version:?} into {song:?}")
        })
    }
}

impl Results {
    fn register_abbrev(
        abbrev_to_song: &mut HashMap<SongAbbreviation, SongIndex>,
        abbrev: &SongAbbreviation,
        index: SongIndex,
    ) -> anyhow::Result<()> {
        match abbrev_to_song.entry(abbrev.clone()) {
            HEntry::Occupied(i) => {
                if *i.get() != index {
                    bail!("At least two songs are associated to nickname {abbrev:?}")
                }
            }
            HEntry::Vacant(e) => {
                e.insert(index);
            }
        }
        Ok(())
    }

    fn read_removed_songs_wiki(&mut self, songs: &RemovedSongsWiki) -> anyhow::Result<()> {
        for data in &songs.songs {
            // Create or get song from song name
            let song_name = SongName::from(data.song_name.to_owned());
            let (index, song) = match self.name_to_song.entry(song_name.clone()) {
                // Song is already registered by inner_lv
                HEntry::Occupied(e) => match Vec::from_iter(e.get())[..] {
                    [&index] => {
                        let song = self.songs.get_mut(index);
                        (index, song)
                    }
                    ref multiple => bail!(
                        "Song name {:?} is not unique: {:?}",
                        &data.song_name,
                        multiple.iter().map(|&&s| self.songs.get(s)).collect_vec(),
                    ),
                },
                // Song is unique in removed_songs_wiki
                HEntry::Vacant(e) => {
                    let (index, song) = self.songs.create_new();
                    e.insert(HashSet::from_iter([index]));
                    (index, song)
                }
            };

            // Register song name as abbrevation (is this correct?)
            match self.abbrev_to_song.entry(data.song_name.to_owned().into()) {
                HEntry::Occupied(i) => {
                    if *i.get() != index {
                        bail!(
                            "At least two songs are associated to nickname {:?}: {:?} and {:?}",
                            &data.song_name,
                            self.songs.get(index),
                            self.songs.get(*i.get()),
                        )
                    }
                }
                HEntry::Vacant(e) => {
                    e.insert(index);
                }
            };

            let remove_date = NaiveDate::parse_from_str(&data.date, "%Y/%m/%d")
                .with_context(|| format!("Unexpected date: {data:?}"))?;
            let last_version = enum_iterator::all()
                .find_or_last(|x: &MaimaiVersion| remove_date <= x.start_date())
                .expect("MaimaiVersion has at least one element")
                .previous()
                .with_context(|| format!("No corresponding version for remove date: {data:?}"))?;
            merge_options(&mut song.name[last_version], Some(&song_name))?;
            merge_remove_state(&mut song.remove_state, remove_date)?;

            for levels in chain([&data.levels], &data.another) {
                let generation = match levels.0[0] {
                    Some(_) => ScoreGeneration::Standard,
                    None => ScoreGeneration::Deluxe,
                };
                if !levels.0[1..5].iter().all(|x| x.is_some()) {
                    bail!("Missing levels between BASIC and MASTER: {data:?}");
                }
                let make = |i: usize| {
                    levels.0[i].map(|level| {
                        let mut map = EnumMap::default();
                        let version = if i == 0 {
                            MaimaiVersion::Finale.min(last_version)
                        } else {
                            last_version
                        };
                        map[version] = Some(InternalScoreLevel::unknown(version, level));
                        OrdinaryScore { levels: map }
                    })
                };
                song.scores[generation].get_or_insert_with(|| OrdinaryScores {
                    easy: make(0),
                    basic: make(1).unwrap(),
                    advanced: make(2).unwrap(),
                    expert: make(3).unwrap(),
                    master: make(4).unwrap(),
                    re_master: make(5),
                    version: None,
                });
            }
        }
        Ok(())
    }

    fn read_removed_songs_supplemental(
        &mut self,
        removed_songs_supplemental: &[RemovedSongSupplemental],
    ) -> anyhow::Result<()> {
        for data in removed_songs_supplemental {
            let (index, song) = match self.name_to_song.get(&data.name) {
                None => bail!("No song matches for {data:?}"),
                Some(x) => match Vec::from_iter(x)[..] {
                    [&index] => (index, self.songs.get_mut(index)),
                    ref multiple => bail!(
                        "Song name {:?} is not unique: {:?}",
                        &data.name,
                        multiple.iter().map(|&&s| self.songs.get(s)).collect_vec(),
                    ),
                },
            };

            // Register icon
            merge_options(&mut song.icon, data.icon.as_ref())?;

            // Regsiter the song name itself as abbreviation
            if let Some(abbrev) = &data.abbrev {
                Self::register_abbrev(&mut self.abbrev_to_song, abbrev, index)?;
            }

            // Register levels
            for &(version, ref levels) in &data.levels {
                let data = InLvSong::try_from(levels.clone())?;
                // Before calling `read_in_lv_song`, we need to merge those fields not covered by that function.
                // According to the implementation of `read_in_lv`, `icon` and `song_name` qualify.
                merge_options(&mut song.name[version], Some(data.song_name()))?;
                merge_options(&mut song.icon, Some(data.icon()))?;

                // Now we can leave the rest to this function.
                Self::read_in_lv_song(
                    &mut self.name_to_song,
                    &mut self.abbrev_to_song,
                    index,
                    song,
                    version,
                    &data,
                    None,
                )?;
            }

            // Register removed date
            merge_remove_state(&mut song.remove_state, data.date)?;
        }
        Ok(())
    }

    fn read_in_lv_data(
        &mut self,
        version: MaimaiVersion,
        data: &InLvData,
        overrides: &InLvDataCorrectionMap,
    ) -> anyhow::Result<()> {
        let try_override =
            |level: InternalScoreLevel,
             icon: &Option<SongIcon>,
             key: (MaimaiVersion, ScoreGeneration, ScoreDifficulty)| {
                if let Some(&[before, after]) = icon
                    .as_ref()
                    .and_then(|icon| overrides.get(icon))
                    .and_then(|x| x.get(&key))
                {
                    if level == before {
                        Ok(after)
                    } else {
                        bail!("Request to override {key:?} from {before:?} to {after:?}, but found {level:?}")
                    }
                } else {
                    Ok(level)
                }
            };

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
                    let missing_song =
                        || format!("Missing song: {:?} (on version {:?})", entry.entry, version);

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
                            try_override(
                                InternalScoreLevel::unknown(version, level),
                                &song.icon,
                                (version, entry.entry.generation(), entry.entry.difficulty),
                            )?,
                            version,
                        )
                        .with_context(|| format!("While processing {entry:?} in {version:?}"))?;
                        anyhow::Ok(())
                    };
                    set(entry.entry.difficulty, level)?;
                    for &(difficulty, level) in &entry.additional {
                        set(difficulty, level)?;
                    }
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
                    let missing_song =
                        || format!("Missing song: {:?} (on version {:?})", entry, version);

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
                        try_override(
                            InternalScoreLevel::known(level),
                            &song.icon,
                            (version, entry.generation(), entry.difficulty),
                        )?,
                        version,
                    )
                    .with_context(|| format!("While processing {entry:?} in {version:?}"))?;
                }
            }
        }
        if !known.is_empty() {
            bail!("Additional data found: {:?}", data.unknown);
        }

        Ok(())
    }

    fn read_additional_abbrevs(
        &mut self,
        additional_abbrevs: &[(SongAbbreviation, SongName)],
    ) -> anyhow::Result<()> {
        for (abbrev, name) in additional_abbrevs {
            let indices = self
                .name_to_song
                .get(name)
                .with_context(|| format!("No song named {name:?}"))?;
            if indices.len() != 1 {
                bail!("Multiple songs named {name:?}");
            }
            let &index = indices.iter().next().unwrap();
            Self::register_abbrev(&mut self.abbrev_to_song, abbrev, index)?;
        }
        Ok(())
    }

    fn read_version_supplental(
        &mut self,
        version_supplemental: &[VersionSupplemental],
    ) -> anyhow::Result<()> {
        for data in version_supplemental {
            let song = self.songs.get_mut(
                self.icon_to_song
                    .get(&data.icon)
                    .copied()
                    .with_context(|| format!("No song matches {:?}", data.icon))?,
            );
            let score = match &mut song.scores[data.generation] {
                Some(score) => score,
                None => bail!(
                    "Song does not have generation {:?}: {:#?}",
                    data.generation,
                    song
                ),
            };
            if score.version.is_some() {
                bail!(
                    "Version is already stored for generation {:?}: {:#?}",
                    data.generation,
                    song
                );
            }
            score.version = Some(data.version);
        }
        Ok(())
    }

    fn verify_latest_official_songs(&self, list: &OfficialSongList) -> anyhow::Result<()> {
        let version = MaimaiVersion::latest();
        if list.timestamp < version.start_time() {
            bail!("The latest official song list is not of the latest version");
        }

        let mut collected_songs = vec![];
        let mut collected_utages = vec![];
        for song in &self.songs.0 {
            if !song.removed() {
                if song.scores.values().any(|x| x.is_some()) {
                    collected_songs.push(song);
                }
                for score in &song.utage_scores {
                    collected_utages.push((song, score));
                }
            }
        }

        let mut official_songs = vec![];
        let mut official_utages = vec![];
        for song in &list.songs {
            match song.details() {
                ScoreDetails::Ordinary(score) => official_songs.push((song, score)),
                ScoreDetails::Utage(score) => official_utages.push((song, score)),
            }
        }

        collected_songs.sort_by_key(|x| &x.icon);
        official_songs.sort_by_key(|x| x.0.image());
        for item in collected_songs.iter().zip_longest(&official_songs) {
            match item {
                EitherOrBoth::Both(collected, (song, score)) => {
                    let level_ok =
                        |x: ScoreLevel| move |y: InternalScoreLevel| x == y.into_level(version);
                    let ok = |generation: ScoreGeneration| {
                        move |levels: official::Levels| {
                            Some(match &collected.scores[generation] {
                                None => "Missing score",
                                Some(collected) => {
                                    if !collected.basic.levels[version]
                                        .is_some_and(level_ok(levels.basic()))
                                    {
                                        "basic"
                                    } else if !collected.advanced.levels[version]
                                        .is_some_and(level_ok(levels.advanced()))
                                    {
                                        "advanced"
                                    } else if !collected.expert.levels[version]
                                        .is_some_and(level_ok(levels.expert()))
                                    {
                                        "expert"
                                    } else if !collected.master.levels[version]
                                        .is_some_and(level_ok(levels.master()))
                                    {
                                        "master"
                                    } else {
                                        let res = match (&collected.re_master, levels.re_master()) {
                                            (Some(x), Some(y)) => {
                                                x.levels[version].is_some_and(level_ok(y))
                                            }
                                            (None, None) => true,
                                            _ => false,
                                        };
                                        if res {
                                            return None;
                                        } else {
                                            "remaster"
                                        }
                                    }
                                }
                            })
                            // .as_ref()
                            // .is_some_and(|collected| {})
                        }
                    };
                    let item_wrong = [
                        (
                            collected.name[version].as_ref() == Some(song.title()),
                            "song name",
                        ),
                        (
                            collected.pronunciation.as_ref() == Some(song.title_kana()),
                            "song kana",
                        ),
                        (
                            collected.artist[version].as_ref() == Some(song.artist()),
                            "artist",
                        ),
                        (collected.icon.as_ref() == Some(song.image()), "icon"),
                        // (
                        //     collected
                        //         .scores
                        //         .values()
                        //         .flatten()
                        //         .filter_map(|v| v.version)
                        //         .any(|version| version == song.version().version()),
                        //     "version",
                        // ),
                        (
                            collected.locked_history.values().last().copied()
                                == Some(song.locked()),
                            "locked",
                        ),
                        (
                            collected.category[version] == Some(score.category()),
                            "category",
                        ),
                    ]
                    .into_iter()
                    .filter_map(|(x, y)| (!x).then_some(y))
                    .collect_vec();
                    let score_wrong = [
                        (
                            score.standard().and_then(ok(ScoreGeneration::Standard)),
                            "standard score",
                        ),
                        (
                            score.deluxe().and_then(ok(ScoreGeneration::Deluxe)),
                            "deluxe score",
                        ),
                    ]
                    .into_iter()
                    .filter_map(|(l, g)| l.map(|l| (g, l)))
                    .collect_vec();
                    if !item_wrong.is_empty() || !score_wrong.is_empty() {
                        bail!("These scores differ by {item_wrong:?} or {score_wrong:?} at version {version:?}\n\n{collected:#?}\n\n{song:#?}")
                    }
                }
                EitherOrBoth::Left(x) => bail!("Only collected songs have {x:?}"),
                EitherOrBoth::Right(x) => bail!("Only official songs have {x:?}"),
            }
        }

        collected_utages.sort_by_key(|x| (&x.0.icon, x.1.identifier()));
        official_utages.sort_by_key(|x| (x.0.image(), x.1.identifier()));

        // for (i, song) in collected_utages.iter().enumerate() {
        //     println!(
        //         "{i} {:?} {:?}",
        //         song.0.name.values().flatten().last(),
        //         song.0.icon
        //     );
        // }
        // for (i, song) in official_utages.iter().enumerate() {
        //     println!("{i} {} {:?}", song.0.title(), song.0.image());
        // }

        for item in collected_utages.iter().zip_longest(&official_utages) {
            match item {
                EitherOrBoth::Both((collected, x), (song, y)) => {
                    let wrong = [
                        (
                            collected.name[version].as_ref().is_some_and(|name| {
                                format!("[{}]{name}", x.kanji())
                                    == AsRef::<str>::as_ref(song.title())
                            }),
                            "song name",
                        ),
                        (
                            collected.pronunciation.as_ref() == Some(song.title_kana()),
                            "song kana",
                        ),
                        (
                            collected.artist[version].as_ref() == Some(song.artist()),
                            "artist",
                        ),
                        (x == y, "utage score"),
                    ]
                    .into_iter()
                    .filter_map(|(x, y)| (!x).then_some(y))
                    .collect_vec();
                    if !wrong.is_empty() {
                        bail!("These utage scores differ by {wrong:?} at version {version:?}\n  - {collected:#?}\n  - {song:#?}")
                    }
                }
                EitherOrBoth::Left(x) => bail!("Only collected songs have {x:?}"),
                EitherOrBoth::Right(x) => bail!("Only official songs have {x:?}"),
            }
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
        Assign(InternalScoreLevel),
        Inconsistent(InternalScoreLevel),
    }
    // use InternalScoreLevel::*;
    use Verdict::*;
    let verdict = match x {
        None => Assign(y),
        &mut Some(x) => {
            if x.into_level(version) != y.into_level(version) {
                Inconsistent(x)
            } else {
                Assign(x.intersection(y))
            }
            // match (x0, y) {
            //     (Unknown(x), Unknown(y)) => {
            //         if x == y {
            //             Keep
            //         } else {
            //             Inconsistent(x0)
            //         }
            //     }
            //     (Unknown(x), Known(y)) => {
            //         if y.to_lv(version) == x {
            //             Assign
            //         } else {
            //             Inconsistent(x0)
            //         }
            //     }
            //     (Known(x), Unknown(y)) => {
            //         if x.to_lv(version) == y {
            //             Keep
            //         } else {
            //             Inconsistent(x0)
            //         }
            //     }
            //     (Known(x), Known(y)) => {
            //         if x == y {
            //             Keep
            //         } else {
            //             Inconsistent(x0)
            //         }
            //     }
            // },
        }
    };
    match verdict {
        Assign(y) => {
            *x = Some(y);
            Ok(())
        }
        Inconsistent(x) => bail!("Inconsistent levels: known to be {x:?}, found {y:?}"),
    }
    // if let Assign = verdict {
    //     *x = Some(y);
    // }
    // if let Inconsistent(x) = verdict {
    //     bail!("Inconsistent levels: known to be {x:?}, found {y:?}")
    // } else {
    //     Ok(())
    // }
}

fn merge_remove_state(
    remove_state: &mut RemoveState,
    remove_date: NaiveDate,
) -> anyhow::Result<()> {
    match *remove_state {
        RemoveState::Present => *remove_state = RemoveState::Removed(remove_date),
        RemoveState::Removed(known_remove_date) => {
            if remove_date != known_remove_date {
                bail!("Conflicting remove date: stored {remove_date}, found {known_remove_date}");
            }
        }
        RemoveState::Revived(_, _) => {
            bail!("Revived songs should be patched later manually")
        }
    }
    Ok(())
}

fn merge_options<T>(x: &mut Option<T>, y: Option<&T>) -> anyhow::Result<()>
where
    T: Eq + Clone + Debug,
{
    if let Some(y) = y {
        match x {
            Some(x) if x != y => bail!("Value mismatch: {x:?} stored, tried to assign {y:?}"),
            _ => *x = Some(y.clone()),
        }
    }
    Ok(())
}

#[derive(Default)]
struct SongList(Vec<Song>);
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
/// Virtual pointer to an element of `Results::songs`.
struct SongIndex(usize);
impl SongList {
    fn get(&self, index: SongIndex) -> &Song {
        &self.0[index.0]
    }
    fn get_mut(&mut self, index: SongIndex) -> &mut Song {
        &mut self.0[index.0]
    }
    fn create_new(&mut self) -> (SongIndex, &mut Song) {
        let index = SongIndex(self.0.len());
        self.0.push(Song::default());
        (index, self.get_mut(index))
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opts = Opts::parse();

    let resources = Resources::load(&opts)?;
    let mut results = Results::default();
    for list in &resources.official_song_lists {
        results
            .read_official_song_list(list, &resources.utage_identifier_merge)
            .with_context(|| {
                format!(
                    "While processing official song list at {:?}",
                    list.timestamp
                )
            })?;
    }
    for (&version, in_lv) in &resources.in_lv {
        results.read_in_lv(version, in_lv, Some(&resources.in_lv_override))?;
    }
    for (&version, in_lv_bitmask) in &resources.in_lv_bitmask {
        results.read_in_lv(
            version,
            in_lv_bitmask,
            Some(&resources.in_lv_bitmask_override),
        )?;
    }
    results.read_removed_songs_wiki(&resources.removed_songs_wiki)?;
    results.read_removed_songs_supplemental(&resources.removed_songs_supplemental)?;
    results.read_additional_abbrevs(&resources.additional_abbrevs)?;
    for (&version, in_lv_data) in &resources.in_lv_data {
        results.read_in_lv_data(version, in_lv_data, &resources.in_lv_data_override)?;
    }
    results.read_version_supplental(&resources.version_supplemental)?;

    results.verify_latest_official_songs(
        resources
            .official_song_lists
            .last()
            .context("There should be at least one official song")?,
    )?;

    // Verify if database can be constructed correctly.
    // Also run `verify_songs` in the same module.
    let _database = SongDatabase::new(&results.songs.0)?;

    write_json(
        opts.database_dir.join("maimai_song_database.json"),
        &results.songs.0,
    )?;

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
            <span\ class='wk_(?<difficulty>[baemr]) (?<new_song2> _n)?'>
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
    let captures = pattern
        .captures(s)
        .with_context(|| format!("Unexpected string: {s:?}"))?;
    let difficulty = parse_difficulty(&captures["difficulty"])?;
    let new_song = captures.name("new_song").is_some();
    let new_song2 = captures.name("new_song2").is_some();
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
            "<span class='wk_{d}{n}'>{us}{song_nickname}{dx}{ut}</span>{additional}",
            d = difficulty_char(difficulty),
            n = if new_song2 { "_n" } else { "" },
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
    #[allow(unused)]
    genre: String,
    song_name: String,
    levels: LevelSet,
    another: Option<LevelSet>,
}
#[derive(Debug)]
struct LevelSet([Option<ScoreLevel>; 6]);

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
                // Ignoring utage score for now
                if row.len() == 2
                    || current_genre == Some("宴")
                    || row[3] == "bgcolor(#ffa07a):''星''"
                {
                    continue;
                }
                // let data: [&str; 10] = row[1..11].try_into().unwrap();
                // let data = data.map(|s| s.to_owned());
                if row[9].ends_with("復活") {
                    continue;
                }

                let parse_level = |s: &str| {
                    (["", " ", "-"].iter().all(|&t| t != s))
                        .then(|| {
                            let captures = regex!(
                                r"(?x)
                                &color\(gray\)\{
                                    (?<level_gray> \d+ \+? )
                                \}
                                |
                                    (?<level_norm> \d+ \+? )
                            "
                            )
                            .captures(s)
                            .with_context(|| format!("Unexpected level: {line:?}"))?;
                            let c = (captures.name("level_gray"))
                                .or(captures.name("level_norm"))
                                .unwrap();
                            anyhow::Ok(c.as_str().parse()?)
                        })
                        .transpose()
                };
                let levels = LevelSet(
                    row[3..9]
                        .iter()
                        .map(|s| parse_level(s))
                        .collect::<Result<Vec<_>, _>>()?
                        .try_into()
                        .unwrap(),
                );

                if row[1] == "^" {
                    songs
                        .last_mut()
                        .with_context(|| format!("Unexpected continued `^`: {line:?}"))?
                        .another = Some(levels);
                } else {
                    let song_name = regex!(r"\[\[([^>]*)(>.*)?\]\]")
                        .captures(row[1])
                        .with_context(|| format!("Unexpected song name: {line:?}"))?
                        .get(1)
                        .unwrap()
                        .as_str()
                        .to_owned();
                    songs.push(RemovedSongWiki {
                        date: current_date.context("Date missing")?.to_owned(),
                        genre: current_genre.context("Genre missing")?.to_owned(),
                        song_name,
                        levels,
                        another: None,
                    });
                }
            }
        }

        Ok(Self { songs })
    }
}

#[derive(Debug, Deserialize)]
pub struct RemovedSongSupplemental {
    icon: Option<SongIcon>,
    name: SongName,
    #[allow(unused)]
    date: NaiveDate,
    abbrev: Option<SongAbbreviation>,
    #[serde(default)]
    levels: Vec<(MaimaiVersion, SongRaw)>,
}

#[derive(PartialEq, Eq, Debug)]
pub struct OfficialSongList {
    timestamp: NaiveDateTime,
    songs: Vec<official::Song>,
}

#[derive(Deserialize)]
pub struct VersionSupplemental {
    icon: SongIcon,
    generation: ScoreGeneration,
    version: MaimaiVersion,
}

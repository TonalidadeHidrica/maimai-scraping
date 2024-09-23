use std::{
    cmp::Reverse, collections::BTreeSet, fmt::Display, iter::successors, num::ParseIntError,
    ops::Range, path::PathBuf, str::FromStr,
};

use anyhow::{anyhow, bail, Context};
use clap::Parser;
use enum_iterator::Sequence;
use hashbrown::HashSet;
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        favorite_songs::{fetch_favorite_songs_form, song_name_to_idx_map, SetFavoriteSong},
        internal_lv_estimator::{
            self,
            multi_user::{MultiUserEstimator, RatingTargetLabel, RecordLabel},
            CandidatesRef, Estimator,
        },
        rating::{self, ScoreConstant, ScoreLevel},
        schema::latest::{ScoreDifficulty, ScoreGeneration},
        song_list::{database::SongDatabase, Song, SongKana},
        version::MaimaiVersion,
        Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::{read_json, read_toml};

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    database_path: PathBuf,
    locked_toml: PathBuf,
    config_toml: PathBuf,

    // Constraints
    #[clap(long)]
    /// Comma-separated list of previous internal levels as integers (e.g. `127,128,129`)
    previous: Option<PreviousLevels>,
    #[clap(long)]
    /// Current levels in the ordinary format (e.g. `13+`)
    /// A hyphen indicates a range, and comma means union
    current: Option<CurrentLevels>,

    #[clap(long)]
    /// Choose only DX Master/ReMaster scores.  `--dx-master` and `--no-dx-master` cannot coexist.
    dx_master: bool,
    #[clap(long)]
    /// Never choose DX Master/ReMaster scores.  `--dx-master` and `--no-dx-master` cannot coexist.
    no_dx_master: bool,

    #[clap(long)]
    /// Choose only locked scores.  `--locked` and `--no-locked` cannot coexist.
    locked: bool,
    #[clap(long)]
    /// Never choose locked scores.  `--locked` and `--no-locked` cannot coexist.
    no_locked: bool,

    #[clap(long)]
    /// Include nonplayable scores.
    /// `--include-nonplayable` and `--only-nonplayable` cannot coexist.
    include_nonplayable: bool,
    #[clap(long)]
    /// Only include nonplayable scores.
    /// `--include-nonplayable` and `--only-nonplayable` cannot coexist.
    only_nonplayable: bool,

    #[clap(long)]
    dry_run: bool,
    #[clap(flatten)]
    user_identifier: UserIdentifier,

    #[clap(long)]
    /// Preserve old favorite songs list instead of overwriting.
    append: bool,
    // #[clap(long)]
    // /// Maximum songs to be added (existing songs count for `--append`)
    // limit: Option<usize>,
    // #[clap(long, default_value = "0")]
    // /// The number of songs to skip among listed ones
    // skip: usize,
    #[clap(long)]
    choose: Option<ChooseScores>,

    #[clap(long)]
    hide_history: bool,
    #[clap(long)]
    hide_current: bool,
    #[clap(long, default_value = "Master")]
    highlight_difficulty: ScoreDifficulty,

    #[clap(long)]
    difficulty: Option<ScoreDifficulty>,

    #[clap(long)]
    sort_only_by_name: bool,

    #[clap(long)]
    newline_after: Option<usize>,
}
#[derive(Clone)]
struct PreviousLevels(BTreeSet<ScoreConstant>);
impl FromStr for PreviousLevels {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_range(s)
            .map(|x| {
                let [x, y] = x.into_pair().map(|x| x.parse::<u8>());
                anyhow::Ok(x?..=y?)
            })
            .flatten_ok()
            .map(|x| {
                anyhow::Ok(
                    x?.try_into()
                        .map_err(|e| anyhow!("Invalid score constant: {e}")),
                )?
            })
            .collect::<anyhow::Result<_>>()
            .map(Self)
    }
}

#[derive(Clone)]
struct CurrentLevels(BTreeSet<ScoreLevel>);
impl FromStr for CurrentLevels {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        parse_range(s)
            .map(|x| {
                let [x, y] = x.into_pair().map(|x| x.parse());
                Ok(ScoreLevel::range_inclusive(x?, y?))
            })
            .flatten_ok()
            .collect::<anyhow::Result<_>>()
            .map(Self)
    }
}

#[derive(Clone, Debug)]
struct ChooseScores(Vec<Range<usize>>);
impl FromStr for ChooseScores {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_range(s)
            .map(|x| {
                Ok(match x {
                    RangeElement::Single(x) => {
                        let x: usize = x.parse()?;
                        x..x + 1
                    }
                    RangeElement::Range([x, y]) => x.parse()?..y.parse()?,
                })
            })
            .map(|x| x.map_err(|x: ParseIntError| x.into()))
            .try_collect()
            .map(Self)
    }
}

#[derive(Clone, Copy, Debug)]
enum RangeElement<'s> {
    Single(&'s str),
    Range([&'s str; 2]),
}
impl<'s> RangeElement<'s> {
    fn into_pair(self) -> [&'s str; 2] {
        match self {
            Self::Single(x) => [x, x],
            Self::Range(x) => x,
        }
    }
}
fn parse_range(s: &str) -> impl Iterator<Item = RangeElement> {
    s.split(',').map(|s| match s.split_once('-') {
        Some((x, y)) => RangeElement::Range([x, y]),
        _ => RangeElement::Single(s),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let opts = Opts::parse();
    if opts.dx_master && opts.no_dx_master {
        bail!("--dx-master and --no-dx-master cannot coexist.")
    }
    if opts.locked && opts.no_locked {
        bail!("--locked and --no-locked cannot coexist.")
    }
    if opts.include_nonplayable && opts.only_nonplayable {
        bail!("--include-nonplayable and --only-nonplayable cannot coexist.")
    }
    if opts.dry_run && opts.append {
        bail!("--dry-run and --append cannot coexist.")
    }
    // let limit = opts.limit.unwrap_or(30);
    // if !(1..=30).contains(&limit) {
    //     bail!("--limit must be between 1 and 30")
    // }

    let songs: Vec<Song> = read_json(&opts.database_path)?;
    let database = SongDatabase::new(&songs)?;
    let mut estimator = Estimator::new(&database, MaimaiVersion::latest())?;

    let locked_scores: locked_toml::Root = read_toml(&opts.locked_toml)?;
    let locked_scores = locked_scores.read(&database)?;

    let config: internal_lv_estimator::multi_user::Config = read_toml(&opts.config_toml)?;
    let datas = config.read_all()?;
    internal_lv_estimator::multi_user::update_all(&database, &datas, &mut estimator)?;

    let scores = get_matching_scores(&estimator, &locked_scores, &opts)?;

    for (i, score) in scores.iter().enumerate() {
        println!(
            "{i:>5} {}",
            display_score(
                &opts,
                opts.hide_history,
                opts.hide_current,
                &locked_scores,
                score
            )
        );
    }
    println!("({:>4} songs in total)", scores.len());

    if !opts.dry_run {
        let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
            credentials_path: &opts.credentials_path,
            cookie_store_path: &opts.cookie_store_path,
            user_identifier: &opts.user_identifier,
            // There is no need to be Standard member to edit favorite songs
            force_paid: false,
        })
        .await?;
        let page = fetch_favorite_songs_form(&mut client).await?;
        let map = song_name_to_idx_map(&page);

        let mut idxs = HashSet::new();
        if opts.append {
            for song in page.songs.iter().filter(|x| x.checked) {
                println!("Preserving existing song: {}", song.name);
                idxs.insert(&song.idx);
            }
        }

        let indices: BTreeSet<_> = match &opts.choose {
            Some(choose) => {
                if choose.0.iter().any(|x| x.end > scores.len()) {
                    bail!("Index out of range: {choose:?}")
                }
                choose.0.iter().cloned().flatten().collect()
            }
            None => (0..scores.len()).collect(),
        };

        let mut added_scores = vec![];
        let mut skipped_scores = vec![];

        for score in indices.into_iter().map(|i| &scores[i]) {
            let song = score.candidates.score().scores().song();
            let category =
                song.song().category[MaimaiVersion::latest()].context("Category unknown")?;
            let song_name = song.latest_song_name();
            let idx = match &map.get(&(category, song_name)).map_or(&[][..], |x| &x[..]) {
                [] => bail!("Song not found: {}", score.candidates.score(),),
                [idx] => idx,
                candidates => {
                    // Now that songs are distinguished by category as well as title,
                    // This should not happen
                    bail!(
                        "Multiple candidates are found: {} {candidates:?}",
                        score.candidates.score()
                    )
                }
            };

            let len = idxs.len();
            let added = if let hashbrown::hash_set::Entry::Vacant(entry) = idxs.entry(*idx) {
                if len < 30 {
                    entry.insert();
                    true
                } else {
                    false
                }
            } else {
                true
            };
            if added {
                added_scores.push(score);
            } else {
                skipped_scores.push(score);
            }
        }

        SetFavoriteSong::builder()
            .token(&page.token)
            .music(idxs.into_iter().collect())
            .build()
            .send(&mut client)
            .await?;
        println!("Favorite songs have been edited.");

        let mut start = 0;
        for (label, mut scores) in [("Added", added_scores), ("SKIPPED", skipped_scores)] {
            if scores.is_empty() {
                continue;
            }
            println!("{label} scores:");
            scores.sort_by_key(|x| x.name_based_key());
            for (score, i) in scores.iter().zip(start..) {
                if (opts.newline_after).is_some_and(|x| i >= x && (i - x) % 3 == 0) {
                    println!();
                }
                println!(
                    "{i:>4} {}",
                    display_score(&opts, true, false, &locked_scores, score)
                );
            }
            start += scores.len();
        }
    } else {
        println!("WARNING: DRY-RUN!");
    }

    Ok(())
}

struct ScoreRet<'s, 'e: 's, 'n> {
    candidates: CandidatesRef<'s, 'e, RecordLabel<'n>, RatingTargetLabel<'n>>,
    estimation: rating::InternalScoreLevel,
    confident: bool,
    kana: Option<&'s SongKana>,
}

impl<'s> ScoreRet<'s, '_, '_> {
    fn name_based_key(&self) -> impl Ord + 's {
        (
            self.kana,
            self.candidates.score().difficulty(),
            self.candidates.score().scores().generation(),
        )
    }
}

fn get_matching_scores<'s, 'e, 'n>(
    estimator: &'e MultiUserEstimator<'s, 'n>,
    locked_scores: &locked_toml::LockedScores<'s>,
    opts: &Opts,
) -> anyhow::Result<Vec<ScoreRet<'s, 'e, 'n>>> {
    let version = MaimaiVersion::latest();
    let mut ret = vec![];

    for candidates in estimator.get_scores() {
        let reliable_version = match candidates.score().scores().generation() {
            ScoreGeneration::Standard => MaimaiVersion::UniversePlus,
            ScoreGeneration::Deluxe => MaimaiVersion::SplashPlus,
        };
        if version < reliable_version {
            bail!("Given version {version:?} is prior to reliable version {reliable_version:?}")
        }
        let levels = &candidates.score().score().levels;
        if levels[version].is_none() {
            bail!(
                "Level not found for current version: {}",
                candidates.score()
            );
        }
        let estimation = levels
            .iter()
            .filter_map(|(version, &level)| (version >= reliable_version).then_some(level?))
            .reduce(|x, mut y| {
                // `verify_songs` guarantees that x and y are non-empty
                let d = |[x, y]: [ScoreConstant; 2]| u8::from(x).abs_diff(u8::from(y));
                let min_dist = (x.candidates())
                    .flat_map(|x| y.candidates().map(move |y| d([x, y])))
                    .min()
                    .unwrap();
                y.retain(|y| x.candidates().any(|x| d([x, y]) == min_dist));
                y
            })
            .unwrap();
        assert!(!estimation.is_empty());
        let estimation_override = levels[MaimaiVersion::SplashPlus].and_then(|splash_plus_lv| {
            let lv12p = ScoreLevel::new(12, true).unwrap();
            let is_lv12p = estimation.into_level(version) == lv12p;
            if estimation.is_unique() || !is_lv12p {
                return None;
            }
            // Heuristic guess
            let level = match u8::from(splash_plus_lv.get_if_unique()?) {
                124 => 129,
                123 => 128,
                122 => 127,
                121 => 127,
                120 => 127,
                _ => return None,
            };
            Some(rating::InternalScoreLevel::known(level.try_into().unwrap()))
        });
        let (estimation, confident) = match estimation_override {
            Some(estimation_override) => (estimation_override, false),
            None => (estimation, true),
        };
        assert!(!estimation.is_empty());

        let previous = opts.previous.as_ref().map_or(true, |levels| {
            candidates
                .candidates()
                .candidates()
                .any(|x| levels.0.contains(&x))
        });
        let current = opts.current.as_ref().map_or(true, |levels| {
            (levels.0).contains(&candidates.candidates().into_level(version))
        });
        let undetermined = !candidates.candidates().is_unique();
        let difficulty = candidates.score().difficulty();
        let dx_master = {
            let dx_master = candidates.score().scores().generation() == ScoreGeneration::Deluxe
                && (difficulty == ScoreDifficulty::Master
                    || difficulty == ScoreDifficulty::ReMaster);
            if_then(opts.dx_master, dx_master) && if_then(opts.no_dx_master, !dx_master)
        };
        let locked = {
            let locked = locked_scores.is_locked(candidates.score().scores());
            if_then(opts.locked, locked) && if_then(opts.no_locked, !locked)
        };
        let playable = {
            let playable = locked_scores.is_playable(candidates.score().scores());
            if opts.include_nonplayable {
                true
            } else if opts.only_nonplayable {
                !playable
            } else {
                playable
            }
        };
        let difficulty = opts.difficulty.map_or(true, |d| difficulty == d);
        if previous && current && undetermined && dx_master && locked && playable && difficulty {
            ret.push(ScoreRet {
                candidates,
                estimation,
                confident,
                kana: candidates
                    .score()
                    .scores()
                    .song()
                    .song()
                    .pronunciation
                    .as_ref(),
            });
        }
    }

    if opts.sort_only_by_name {
        ret.sort_by_key(|x| x.name_based_key());
    } else {
        ret.sort_by_key(|x| {
            (
                Reverse(x.estimation.into_level(version)),
                &x.candidates.score().scores().song().song().icon,
                x.candidates.score().scores().generation(),
                x.candidates.score().difficulty(),
            )
            // (
            //     // x.estimation.count_candidates(),
            //     Reverse(x.estimation.candidates().last()),
            //     Reverse(x.confident),
            //     x.kana,
            //     &x.candidates.score().scores().song().song().icon,
            //     x.candidates.score().scores().generation(),
            //     x.candidates.score().difficulty(),
            // )
        });
    }

    Ok(ret)
}

fn display_score<'s, 'o: 's, 'sr: 's>(
    opts: &'o Opts,
    hide_history: bool,
    hide_current: bool,
    locked_scores: &locked_toml::LockedScores<'s>,
    score: &'sr ScoreRet<'s, '_, '_>,
) -> impl Display + 's {
    let history = {
        let history = successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next)
            .map(move |v| {
                lazy_format!(match (score.candidates.score().score().levels[v]) {
                    None => ("{:4}", ""),
                    Some(x) => (
                        "{}",
                        lazy_format!(match (x.get_if_unique()) {
                            Some(y) => "{y:4}",
                            _ => ("{:4}", x.into_level(v)),
                        })
                    ),
                })
            })
            .join_with(" ");
        lazy_format!(if hide_history => "" else => "[{history}] => ")
    };
    let estimation = {
        let estimation = score.estimation;
        let confident = if score.confident { "? " } else { "??" };
        let estimation = format!("{estimation:>8}{confident}");
        lazy_format!(if hide_current => "" else => "{estimation:8}")
    };
    let locked = if !locked_scores.is_playable(score.candidates.score().scores()) {
        "!!"
    } else if locked_scores.is_locked(score.candidates.score().scores()) {
        "!"
    } else {
        ""
    };
    let score = {
        let score = score.candidates.score();
        let highlight_if = |b: bool| if b { "`" } else { "" };
        let song = score.scores().song();
        let x = highlight_if(song.song().scores.values().flatten().count() == 2);
        let y = highlight_if(score.difficulty() != opts.highlight_difficulty);
        lazy_format!(
            "{} ({x}{}{x} {y}{}{y})",
            song.latest_song_name(),
            score.scores().generation().abbrev(),
            score.difficulty().abbrev(),
        )
    };
    lazy_format!("{history}{estimation} {locked:2} {score}")
}

fn if_then(a: bool, b: bool) -> bool {
    !a || b
}

mod locked_toml {
    use anyhow::bail;
    use hashbrown::HashMap;
    use maimai_scraping::maimai::{
        schema::latest::{ScoreGeneration, SongIcon, SongName},
        song_list::database::{OrdinaryScoresRef, SongDatabase},
        version::MaimaiVersion,
    };
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct Root {
        pub songs: Vec<Song>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Song {
        pub name: SongName,
        pub generation: ScoreGeneration,
        pub icon: SongIcon,
        pub version: MaimaiVersion,
        pub playable: bool,
    }

    pub struct LockedScores<'s>(HashMap<OrdinaryScoresRef<'s>, bool>);

    impl Root {
        pub fn read<'s>(&self, database: &'s SongDatabase) -> anyhow::Result<LockedScores<'s>> {
            let mut ret = HashMap::new();
            for data in &self.songs {
                let song = database.song_from_icon(&data.icon)?;
                if &data.name != song.latest_song_name() {
                    bail!("Song name mismatch: {data:?} {song:?}")
                }
                let Some(scores) = song.scores(data.generation) else {
                    bail!("Scores not found: {data:?}");
                };
                if scores.scores().version != Some(data.version) {
                    bail!("Version mismtach: {data:?} {scores:?}")
                }
                ret.insert(scores, data.playable);
            }
            Ok(LockedScores(ret))
        }
    }

    impl LockedScores<'_> {
        // Whether the score is locked (not playable by new card).
        pub fn is_locked(&self, score: OrdinaryScoresRef) -> bool {
            self.0.contains_key(&score)
                || score
                    .song()
                    .song()
                    .locked_history
                    .values()
                    .copied()
                    .last()
                    .unwrap_or(false)
        }

        // Whether the score is playable (by main card).
        pub fn is_playable(&self, score: OrdinaryScoresRef) -> bool {
            self.0.get(&score).copied().unwrap_or(true)
        }
    }
}

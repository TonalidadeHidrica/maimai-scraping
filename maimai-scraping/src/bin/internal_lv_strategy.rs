use std::{
    cmp::Reverse, collections::BTreeSet, fmt::Display, iter::successors, path::PathBuf,
    str::FromStr,
};

use anyhow::{bail, Context};
use clap::Parser;
use enum_iterator::Sequence;
use fs_err::read_to_string;
use hashbrown::HashSet;
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    maimai::{
        estimate_rating::EstimatorConfig,
        favorite_songs::{fetch_favorite_songs_form, song_name_to_idx_map, SetFavoriteSong},
        internal_lv_estimator::{
            self,
            multi_user::{MultiUserEstimator, RatingTargetLabel, RecordLabel},
            CandidatesRef, Estimator,
        },
        load_score_level::MaimaiVersion,
        official_song_list::SongKana,
        rating::{self, ScoreConstant, ScoreLevel},
        schema::latest::{ScoreDifficulty, ScoreGeneration},
        song_list::{
            database::{OrdinaryScoreRef, SongDatabase},
            Song,
        },
        Maimai,
    },
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    database_path: PathBuf,
    locked_toml: PathBuf,
    config_toml: PathBuf,

    // Constraints
    // #[clap(long)]
    // /// Comma-separated list of previous internal levels as integers (e.g. `127,128,129`)
    // previous: Option<Levels>,
    #[clap(long)]
    /// Current levels in the ordinary format (e.g. `13+`)
    /// A hyphen indicates a range, and comma means union
    current: Option<CurrentLevels>,
    #[clap(long)]
    /// Choose only DX (ReMaster) scores.
    dx_master: bool,
    #[clap(long)]
    /// Never hoose DX (ReMaster) scores.  `--dx-master` and `--no-dx-master` cannot coexist.
    no_dx_master: bool,

    #[clap(long)]
    dry_run: bool,
    #[clap(flatten)]
    estimator_config: EstimatorConfig,
    #[clap(flatten)]
    user_identifier: UserIdentifier,

    #[clap(long)]
    /// Preserve old favorite songs list instead of overwriting.
    append: bool,
    #[clap(long)]
    /// Maximum songs to be added (existing songs count for `--append`)
    limit: Option<usize>,
    #[clap(long, default_value = "0")]
    /// The number of songs to skip among listed ones
    skip: usize,

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
}
// #[derive(Clone)]
// struct Levels(Vec<ScoreConstant>);
// impl FromStr for Levels {
//     type Err = anyhow::Error;
//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         Ok(Self(
//             s.split(',')
//                 .map(|s| ScoreConstant::try_from(s.parse::<u8>()?).map_err(|e| anyhow!("Bad: {e}")))
//                 .collect::<anyhow::Result<Vec<ScoreConstant>>>()?,
//         ))
//     }
// }

#[derive(Clone)]
struct CurrentLevels(BTreeSet<ScoreLevel>);
impl FromStr for CurrentLevels {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> anyhow::Result<Self> {
        Ok(Self(
            s.split(',')
                .map(|s| {
                    Ok(match s.split_once('-') {
                        Some((x, y)) => ScoreLevel::range_inclusive(x.parse()?, y.parse()?),
                        _ => {
                            let x = s.parse()?;
                            ScoreLevel::range_inclusive(x, x)
                        }
                    })
                })
                .flatten_ok()
                .collect::<anyhow::Result<_>>()?,
        ))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pretty_env_logger::init();
    let opts = Opts::parse();
    if opts.dx_master && opts.no_dx_master {
        bail!("--dx-master and --no-dx-master cannot coexist.")
    }
    if opts.dry_run && opts.append {
        bail!("--dry-run and --append cannot coexist.")
    }
    let limit = opts.limit.unwrap_or(30);
    if !(1..=30).contains(&limit) {
        bail!("--limit must be between 1 and 30")
    }

    let songs: Vec<Song> = read_json(&opts.database_path)?;
    let database = SongDatabase::new(&songs)?;
    let mut estimator = Estimator::new(&database, MaimaiVersion::latest())?;

    let locked_songs: locked_toml::Root =
        toml::from_str(&fs_err::read_to_string(&opts.locked_toml)?)?;
    let locked_songs = locked_songs.read(&database)?;

    let config: internal_lv_estimator::multi_user::Config =
        toml::from_str(&read_to_string(&opts.config_toml)?)?;
    let datas = config.read_all()?;
    internal_lv_estimator::multi_user::update_all(&database, &datas, &mut estimator)?;

    let scores = get_matching_scores(&estimator, &opts)?;

    for (i, score) in scores.iter().enumerate() {
        let history = successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next)
            .map(|v| match score.candidates.score().score().levels[v] {
                None => "".to_owned(),
                Some(x) => match x.get_if_unique() {
                    Some(y) => y.to_string(),
                    _ => x.into_level(v).to_string(),
                },
            })
            .map(|v| lazy_format!("{v:4}"))
            .join_with(" ");
        let history = lazy_format!(if opts.hide_history => "" else => "[{history}] => ");
        let estimation = score.estimation;
        let confident = if score.confident { "? " } else { "??" };
        let estimation = format!("[{estimation}]{confident}");
        let estimation = lazy_format!(if opts.hide_current => "" else => "{estimation:8}");
        let locked = {
            let scores = score.candidates.score().scores();
            let song = scores.song().song();
            let locked_in_database = song
                .locked_history
                .values()
                .copied()
                .last()
                .unwrap_or(false);
            let locked_additional = locked_songs.contains(&scores);
            if locked_in_database || locked_additional {
                '!'
            } else {
                ' '
            }
        };
        println!(
            "{i:>4} {history}{estimation} {locked} {}",
            display_score(score.candidates.score(), opts.highlight_difficulty)
        );
    }

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
        let mut not_all = false;
        let mut skipped = 0;

        for score in scores {
            let song = score.candidates.score().scores().song();
            let song_name = song.latest_song_name();
            match &map
                .get(&(
                    song.song().category[MaimaiVersion::latest()].context("Category unknown")?,
                    song_name,
                ))
                .map_or(&[][..], |x| &x[..])
            {
                [] => println!("Song not found: {}", score.candidates.score(),),
                [idx] => {
                    let len = idxs.len();
                    if let hashbrown::hash_set::Entry::Vacant(entry) = idxs.entry(*idx) {
                        if len < limit {
                            if skipped < opts.skip {
                                skipped += 1;
                            } else {
                                entry.insert();
                            }
                        } else {
                            not_all = true;
                        }
                    }
                }
                candidates => {
                    // Now that songs are distinguished by category as well as title,
                    // This should not happen
                    bail!(
                        "Multiple candidates are found: {} {candidates:?}",
                        score.candidates.score()
                    )
                }
            }
        }
        if skipped > 0 {
            println!("Skipped {skipped} songs.");
        }
        if not_all {
            println!("Only the first {limit} of the candidates will be added.");
        }
        SetFavoriteSong::builder()
            .token(&page.token)
            .music(idxs.into_iter().collect())
            .build()
            .send(&mut client)
            .await?;
        println!("Favorite songs have been edited.");
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

fn get_matching_scores<'s, 'e, 'n>(
    estimator: &'e MultiUserEstimator<'s, 'n>,
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

        // let previous = opts.previous.as_ref().map_or(true, |x| {
        //     x.0.iter().any(|&x| candidates.iter().any(|&y| x == y))
        // });
        let previous = true;
        let current = opts.current.as_ref().map_or(true, |levels| {
            (levels.0).contains(&candidates.candidates().into_level(version))
        });
        let undetermined = !candidates.candidates().is_unique();
        let difficulty = candidates.score().difficulty();
        let dx_master = candidates.score().scores().generation() == ScoreGeneration::Deluxe
            && (difficulty == ScoreDifficulty::Master || difficulty == ScoreDifficulty::ReMaster);
        let dx_master =
            if_then(opts.dx_master, dx_master) && if_then(opts.no_dx_master, !dx_master);
        let difficulty = opts.difficulty.map_or(true, |d| difficulty == d);
        if previous && current && undetermined && dx_master && difficulty {
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
        ret.sort_by_key(|x| x.kana);
    } else {
        ret.sort_by_key(|x| {
            (
                x.estimation.count_candidates(),
                Reverse(x.estimation.candidates().last()),
                Reverse(x.confident),
                x.kana,
                &x.candidates.score().scores().song().song().icon,
                x.candidates.score().scores().generation(),
                x.candidates.score().difficulty(),
            )
        });
    }

    Ok(ret)
}

fn display_score(
    score: OrdinaryScoreRef,
    highlight_difficulty: ScoreDifficulty,
) -> impl Display + '_ {
    let highlight_if = |b: bool| if b { "`" } else { "" };
    let song = score.scores().song();
    let x = highlight_if(song.song().scores.values().flatten().count() == 2);
    let y = highlight_if(score.difficulty() != highlight_difficulty);
    lazy_format!(
        "{} ({x}{}{x} {y}{}{y})",
        song.latest_song_name(),
        score.scores().generation().abbrev(),
        score.difficulty().abbrev(),
    )
}

fn if_then(a: bool, b: bool) -> bool {
    !a || b
}

mod locked_toml {
    use anyhow::bail;
    use hashbrown::HashSet;
    use maimai_scraping::maimai::{
        load_score_level::MaimaiVersion,
        schema::latest::{ScoreGeneration, SongIcon, SongName},
        song_list::database::{OrdinaryScoresRef, SongDatabase},
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
    }

    impl Root {
        pub fn read<'s>(
            &self,
            database: &'s SongDatabase,
        ) -> anyhow::Result<HashSet<OrdinaryScoresRef<'s>>> {
            let mut ret = HashSet::new();
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
                ret.insert(scores);
            }
            Ok(ret)
        }
    }
}

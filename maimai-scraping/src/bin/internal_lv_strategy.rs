use std::{cmp::Reverse, collections::BTreeMap, fmt::Display, iter::successors, path::PathBuf};

use anyhow::bail;
use clap::Parser;
use enum_iterator::Sequence;
use fs_err::read_to_string;
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use maimai_scraping::{
    api::{SegaClient, SegaClientInitializer},
    cookie_store::UserIdentifier,
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{EstimatorConfig, ScoreConstantsEntry, ScoreConstantsStore, ScoreKey},
        estimator_config_multiuser::{self, update_all},
        favorite_songs::{fetch_favorite_songs_form, song_name_to_idx_map, SetFavoriteSong},
        load_score_level::{self, InternalScoreLevel, MaimaiVersion},
        rating::{ScoreConstant, ScoreLevel},
        schema::latest::{ScoreDifficulty, ScoreGeneration, SongIcon, SongName},
        Maimai,
    },
};
use serde::Deserialize;

#[derive(Parser)]
struct Opts {
    credentials_path: PathBuf,
    cookie_store_path: PathBuf,
    in_lv_history_json: PathBuf,
    new_json: PathBuf,
    config_toml: PathBuf,

    // Constraints
    // #[clap(long)]
    // /// Comma-separated list of previous internal levels as integers (e.g. `127,128,129`)
    // previous: Option<Levels>,
    #[clap(long)]
    /// Up to one current level in an ordinary format (e.g. `13+`)
    current: Option<ScoreLevel>,
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

    let in_lvs: Vec<(OwnedScoreKey, BTreeMap<MaimaiVersion, InternalScoreLevel>)> =
        read_json(&opts.in_lv_history_json)?;
    let in_lvs: HashMap<_, _> = in_lvs.iter().map(|(k, v)| (ScoreKey::from(k), v)).collect();
    let new = load_score_level::load(&opts.new_json)?;
    let mut new = ScoreConstantsStore::new(&new, &[])?;

    let config: estimator_config_multiuser::Root =
        toml::from_str(&read_to_string(&opts.config_toml)?)?;
    let datas = config.read_all()?;
    update_all(&datas, &mut new)?;

    let songs = songs(&in_lvs, &new, &opts)?;

    for (i, song) in songs.iter().enumerate() {
        let history = successors(Some(MaimaiVersion::SplashPlus), MaimaiVersion::next)
            .map(|v| match song.history.and_then(|h| h.get(&v)) {
                None => "".to_owned(),
                Some(InternalScoreLevel::Known(v)) => v.to_string(),
                Some(InternalScoreLevel::Unknown(v)) => v.to_string(),
            })
            .map(|v| lazy_format!("{v:4}"))
            .join_with(" ");
        let estimation = song.estimation.iter().map(|x| x.to_string()).join_with(" ");
        let estimation = format!("[{estimation}]?");
        println!(
            "{i:>4} [{history}] => {estimation:8} {}",
            display_song(song.song_name(), song.key)
        );
    }

    if !opts.dry_run {
        let (mut client, _) = SegaClient::<Maimai>::new(SegaClientInitializer {
            credentials_path: &opts.credentials_path,
            cookie_store_path: &opts.cookie_store_path,
            user_identifier: &opts.user_identifier,
        })
        .await?;
        let page = fetch_favorite_songs_form(&mut client).await?;
        let map = song_name_to_idx_map(&page);
        let mut idxs = HashSet::new();
        if opts.append {
            for song in page
                .genres
                .iter()
                .flat_map(|x| &x.songs)
                .filter(|x| x.checked)
            {
                println!("Preserving existing song: {}", song.name);
                idxs.insert(&song.idx);
            }
        }
        let mut not_all = false;
        for song in songs {
            let song_name = song.song_name();
            match &map.get(song_name).map_or(&[][..], |x| &x[..]) {
                [] => println!("Song not found: {}", display_song(song_name, song.key)),
                [idx] => {
                    let len = idxs.len();
                    if let hashbrown::hash_set::Entry::Vacant(entry) = idxs.entry(*idx) {
                        if len < limit {
                            entry.insert();
                        } else {
                            not_all = true;
                        }
                    }
                }
                idxs => bail!("Multiple candidates are found: {song:?} {idxs:?}"),
            }
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
    } else {
        println!("WARNING: DRY-RUN!");
    }

    Ok(())
}

#[derive(Debug)]
struct SongsRet<'os, 'ns, 'nst> {
    // old_song: &'os Song,
    // old_consts: &'os [ScoreConstant],
    estimation: Vec<ScoreConstant>,
    history: Option<&'os BTreeMap<MaimaiVersion, InternalScoreLevel>>,
    key: ScoreKey<'ns>,
    new_entry: &'nst ScoreConstantsEntry<'ns>,
}
impl<'ns> SongsRet<'_, 'ns, '_> {
    fn song_name(&self) -> &'ns SongName {
        self.new_entry.song().song_name()
    }
}

fn songs<'os, 'ns, 'nst>(
    old: &HashMap<ScoreKey, &'os BTreeMap<MaimaiVersion, InternalScoreLevel>>,
    new: &'nst ScoreConstantsStore<'ns>,
    opts: &Opts,
) -> anyhow::Result<Vec<SongsRet<'os, 'ns, 'nst>>> {
    let mut ret = vec![];
    for (&key, entry) in new.scores() {
        let history = old.get(&key).copied();

        // This is just heuristic
        let reliable_version = match entry.song().generation() {
            ScoreGeneration::Standard => MaimaiVersion::UniversePlus,
            ScoreGeneration::Deluxe => MaimaiVersion::SplashPlus,
        };
        let estimation = history
            .into_iter()
            .flatten()
            .filter(|z| *z.0 >= reliable_version)
            .map(|(&version, &lv)| match lv {
                InternalScoreLevel::Unknown(lv) => lv
                    .score_constant_candidates_aware(version >= MaimaiVersion::BuddiesPlus)
                    .collect(),
                InternalScoreLevel::Known(lv) => vec![lv],
            })
            .reduce(|x, y| {
                let d = |[x, y]: [ScoreConstant; 2]| u8::from(x).abs_diff(u8::from(y));
                let y = y
                    .into_iter()
                    .map(|y| (x.iter().map(|&x| d([x, y])).min().unwrap(), y))
                    .collect_vec();
                // x and y is guaranteed to have at least one element
                let min = y.iter().map(|x| x.0).min().unwrap();
                y.into_iter()
                    .filter_map(|(s, y)| (s == min).then_some(y))
                    .collect()
                // Estimation list is always non-empty
            })
            .unwrap_or_else(|| entry.candidates().clone());

        // let previous = opts.previous.as_ref().map_or(true, |x| {
        //     x.0.iter().any(|&x| candidates.iter().any(|&y| x == y))
        // });
        let previous = true;
        let current = opts.current.map_or(true, |level| {
            level
                .score_constant_candidates()
                .any(|x| entry.candidates().iter().any(|&y| x == y))
        });
        let undetermined = entry.candidates().len() != 1;
        let dx_master = key.generation == ScoreGeneration::Deluxe
            && (key.difficulty == ScoreDifficulty::Master
                || key.difficulty == ScoreDifficulty::ReMaster);
        let dx_master =
            if_then(opts.dx_master, dx_master) && if_then(opts.no_dx_master, !dx_master);
        if previous && current && undetermined && dx_master {
            ret.push(SongsRet {
                estimation,
                history,
                key,
                new_entry: entry,
            });
        }
    }
    ret.sort_by_key(|x| {
        (
            x.estimation.len(),
            Reverse(x.estimation.last().copied()),
            x.song_name(),
        )
    });
    Ok(ret)
}

fn display_song<'a>(name: &'a SongName, key: ScoreKey) -> impl Display + 'a {
    lazy_format!("{name} ({:?} {:?})", key.generation, key.difficulty)
}

fn if_then(a: bool, b: bool) -> bool {
    !a || b
}

#[derive(PartialEq, Eq, Hash, Deserialize)]
pub struct OwnedScoreKey {
    pub icon: SongIcon,
    pub generation: ScoreGeneration,
    pub difficulty: ScoreDifficulty,
}
impl<'a> From<&'a OwnedScoreKey> for ScoreKey<'a> {
    fn from(value: &'a OwnedScoreKey) -> Self {
        ScoreKey {
            icon: &value.icon,
            generation: value.generation,
            difficulty: value.difficulty,
        }
    }
}

use std::{borrow::Cow, path::PathBuf};

use clap::{Parser, ValueEnum};
use hashbrown::HashSet;
use joinery::JoinableIterator;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    internal_lv_estimator::{self, multi_user::update_all, Estimator, Reason},
    load_score_level::MaimaiVersion,
    song_list::{self, database::SongDatabase},
};
use maimai_scraping_utils::fs_json_util::read_json;

#[derive(Parser)]
struct Opts {
    database: PathBuf,
    estimator_config: PathBuf,
    #[arg(long)]
    format: Option<ReportFormat>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportFormat {
    Simple,
    Tsv,
    JsonArray,
}

fn main() -> anyhow::Result<()> {
    env_logger::builder().format_timestamp_nanos().init();
    let opts = Opts::parse();

    let songs: Vec<song_list::Song> = read_json(opts.database)?;
    let database = SongDatabase::new(&songs)?;

    let config: internal_lv_estimator::multi_user::Config =
        toml::from_str(&fs_err::read_to_string(opts.estimator_config)?)?;
    let datas = config.read_all()?;

    let version = MaimaiVersion::latest();

    let mut estimator = Estimator::new(&database, version)?;
    let before_len = estimator.event_len();
    update_all(&database, &datas, &mut estimator)?;

    let scores = {
        let mut scores = vec![];
        let mut appeared = HashSet::new();
        for candidates in estimator.events()[before_len..].iter().rev() {
            if appeared.insert(candidates.score()) {
                scores.push(candidates.score());
            }
        }
        scores.reverse();
        scores
    };

    if let Some(ReportFormat::Tsv) = opts.format {
        println!("曲名\t\t\t確定Lv\t変更前mask\t変更後mask\t事由");
    }
    let mut out = vec![];
    for score in scores {
        let candidates = estimator.get(score).unwrap();
        match opts.format {
            Some(ReportFormat::Simple) => {
                if let Some(lv) = candidates.candidates().get_if_unique() {
                    println!("{}: {lv}", candidates.score());
                }
            }
            Some(ReportFormat::Tsv) => {
                let lv = lazy_format!(match (candidates.candidates().get_if_unique()) {
                    None => "",
                    Some(lv) => "{lv}",
                });
                let song_name = score.scores().song().latest_song_name();
                let generation = score.scores().generation().abbrev();
                let difficulty = score.difficulty().abbrev();
                let mask_before = score.score().levels[version]
                    .unwrap()
                    .in_lv_mask(version)
                    .get();
                let mask_after = candidates.candidates().in_lv_mask(version).get();
                let reasons = candidates
                    .reasons()
                    .map(|e| {
                        let reason = lazy_format!(match (e.reason()) {
                            Reason::Database(_) => "既知データより",
                            Reason::Delta(a, d, x) => (
                                "カード {} での {} のプレイで達成率 {a}、単曲レート {d} より",
                                x.user(),
                                x.play_time()
                            ),
                            Reason::List(x) => (
                                "カード {} での {} 時点のベスト枠 ({}周目) より",
                                x.user(),
                                x.timestamp(),
                                x.iteration() + 1,
                            ),
                        });
                        lazy_format!(
                            "{reason} {} に{}",
                            e.candidates(),
                            lazy_format!(if e.candidates().is_unique() => "確定" else => "限定")
                        )
                    })
                    .join_with("\t");
                println!(
                    "{song_name}\t{generation}\t{difficulty}\t{lv}\t{mask_before}\t{mask_after}\t{reasons}"
                );
            }
            Some(ReportFormat::JsonArray) => {
                if let Some(lv) = candidates.candidates().get_if_unique() {
                    let song = candidates.score().scores().song();
                    let song_name = song.latest_song_name();
                    let song_name: &str = if database.song_from_name(song_name).count() != 1 {
                        song.song()
                            .abbreviation
                            .values()
                            .flatten()
                            .last()
                            .map_or_else(|| song_name.as_ref(), |x| x.as_ref())
                    } else {
                        song_name.as_ref()
                    };
                    out.push([
                        Cow::Borrowed(song_name),
                        Cow::Borrowed(score.scores().generation().abbrev()),
                        Cow::Borrowed(score.difficulty().abbrev()),
                        Cow::Owned(lv.to_string()),
                    ]);
                }
            }
            None => {}
        }
    }
    if let Some(ReportFormat::JsonArray) = opts.format {
        println!("[");
        for out in out {
            println!("  {},", serde_json::to_string(&out)?);
        }
        println!("]");
    }

    Ok(())
}

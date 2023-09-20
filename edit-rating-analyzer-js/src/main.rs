use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Parser;
use itertools::{EitherOrBoth, Itertools};
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        estimate_rating::{ScoreConstantsStore, ScoreKey},
        load_score_level::{self, RemovedSong, Song, SongRaw},
        MaimaiUserData,
    },
};
use rslint_parser::{
    ast::{Expr, ExprOrSpread, LiteralProp, ObjectExpr, ObjectProp, PropName, VarDecl},
    AstNode,
};

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
    level_file: PathBuf,
    removed_songs: PathBuf,
    in_lv_js: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();

    let js = std::fs::read_to_string(&opts.in_lv_js)?;
    let songs = load_songs(&js)?;

    let data: MaimaiUserData = read_json(&opts.maimai_user_data_path)?;
    let levels_original = load_score_level::load(opts.level_file)?;
    let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    let mut levels = ScoreConstantsStore::new(&levels_original, &removed_songs)?;
    levels.do_everything(data.records.values(), &data.rating_targets)?;

    // Assert that the parsed data and generated data coincide
    for entry in songs.iter().map(|x| &x.song).zip_longest(&levels_original) {
        match entry {
            EitherOrBoth::Both(x, y) if x == y => {}
            _ => {
                bail!("There is a difference! {entry:?}");
            }
        }
    }

    let mut tasks = vec![];
    for SongInJs { song, lv_tokens } in &songs {
        for (difficulty, level) in song.levels().iter() {
            let (level, index) = (level.value(), level.index());
            let token = &lv_tokens[index];
            let key = ScoreKey {
                icon: song.icon(),
                generation: song.generation(),
                difficulty,
            };
            let (_, candidates) = levels.get(key)?.context("Entry not found for {key:?}")?;
            if !level.is_known() && candidates.len() == 1 {
                tasks.push((token.range(), candidates[0]));
            }
        }
    }
    tasks.sort_by_key(|x| (x.0.start(), x.0.end()));

    let mut cursor = 0;
    let mut result = String::with_capacity(js.len());
    for (range, level) in tasks {
        let (start, end) = (usize::from(range.start()), usize::from(range.end()));
        if start < cursor {
            bail!("Range is overlapping");
        }
        result += &js[cursor..start];
        result += &level.to_string();
        cursor = end;
    }
    result += &js[cursor..];

    std::fs::write(opts.in_lv_js, &result)?;

    Ok(())
}

fn load_songs(js: &str) -> anyhow::Result<Vec<SongInJs>> {
    let parse_script = rslint_parser::parse_text(js, 0);
    let syntax_node = parse_script.syntax();

    let Expr::ArrayExpr(value) = syntax_node
        .children()
        .filter_map(VarDecl::cast)
        .flat_map(|var_decl| var_decl.declared())
        .find_map(|decl| {
            (&decl.pattern()?.text().to_string() == "in_lv")
                .then(|| decl.value())
                .flatten()
        })
        .context("in_lv value not found")?
    else {
        bail!("in_lv is not an array");
    };

    let mut songs = vec![];
    for value in value.elements() {
        let ExprOrSpread::Expr(Expr::ObjectExpr(obj)) = value else {
            continue;
        };
        let song = parse_song(&obj).with_context(|| format!("While parsing {}", obj.text()))?;
        songs.push(song);
    }
    Ok(songs)
}

fn parse_song(obj: &ObjectExpr) -> anyhow::Result<SongInJs> {
    let mut dx = None::<u8>;
    let mut v = None::<i8>;
    let mut lv = None;
    let mut n = None::<String>;
    let mut ico = None::<String>;
    for prop in obj.props() {
        let ObjectProp::LiteralProp(prop) = prop else {
            continue;
        };
        let Some(PropName::Ident(key)) = prop.key() else {
            continue;
        };
        match &key.text()[..] {
            "dx" => {
                dx = Some(
                    prop.value()
                        .context("The value of `dx` does not exist")?
                        .text()
                        .parse()
                        .context("While parsing `dx`")?,
                )
            }
            "v" => {
                v = Some(
                    prop.value()
                        .context("The value of `v` does not exist")?
                        .text()
                        .parse()
                        .context("While parsing `v`")?,
                )
            }
            "lv" => {
                let Expr::ArrayExpr(array) =
                    prop.value().context("The value of `lv` does not exist")?
                else {
                    bail!("The value of `lv` is not an array")
                };
                let mut lvs = vec![];
                let mut tokens = vec![];
                for value in array.elements() {
                    let lv = value
                        .text()
                        .parse()
                        .context("The array for `lv` has a non-number element")?;
                    lvs.push(lv);
                    tokens.push(value);
                }

                lv = Some((lvs, tokens))
            }
            "n" => n = Some(parse_backtick_string(&prop).context("While parsing `n`")?),
            "ico" => ico = Some(parse_backtick_string(&prop).context("While parsing `ico`")?),
            _ => {}
        }
    }
    match (dx, v, lv, n, ico) {
        (Some(dx), Some(v), Some((lv, lv_tokens)), Some(n), Some(ico)) => Ok(SongInJs {
            song: SongRaw { dx, v, lv, n, ico }.try_into()?,
            lv_tokens,
        }),
        _ => bail!("Some of the fields (`dx, `v`, `lv`, `n`, and `ico`) are missing"),
    }
}

#[derive(Debug)]
struct SongInJs {
    song: Song,
    lv_tokens: Vec<ExprOrSpread>,
}

fn parse_backtick_string(prop: &LiteralProp) -> anyhow::Result<String> {
    Ok(prop
        .value()
        .context("The value does not exist")?
        .text()
        .strip_prefix('`')
        .context("The value does not start with backtick")?
        .strip_suffix('`')
        .context("The value does not end with backtick")?
        .to_owned())
}

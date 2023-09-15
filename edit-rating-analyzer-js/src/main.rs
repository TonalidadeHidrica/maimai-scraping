use std::path::PathBuf;

use anyhow::{bail, Context};
use clap::Parser;
use maimai_scraping::maimai::load_score_level::{Song, SongRaw};
use rslint_parser::{
    ast::{Expr, ExprOrSpread, LiteralProp, ObjectExpr, ObjectProp, PropName, VarDecl},
    AstNode,
};

#[derive(Parser)]
struct Opts {
    in_lv_js: PathBuf,
}

fn main() -> anyhow::Result<()> {
    // let data = load_data_from_file::<Maimai, _>(&opts.maimai_user_data_path)?;
    // let levels = load_score_level::load(opts.level_file)?;
    // let removed_songs: Vec<RemovedSong> = read_json(opts.removed_songs)?;
    // let mut levels = ScoreConstantsStore::new(&levels, &removed_songs)?;
    // levels.do_everything(data.records.values(), &data.rating_targets)?;

    let opts = Opts::parse();
    let js = std::fs::read_to_string(opts.in_lv_js)?;
    let parse_script = rslint_parser::parse_text(&js, 0);
    let syntax_node = parse_script.syntax();
    // for child in syntax_node.children_with_tokens() {
    //     let node = match child {
    //         SyntaxElement::Node(node) => node,
    //         SyntaxElement::Token(token) => {
    //             // print!("{}", token.text());
    //             continue;
    //         }
    //     };
    //     let Some(var_decl) = node.try_to::<VarDecl>() else {
    //         // print!("{}", node.text());
    //         continue;
    //     };
    //     println!("{:?}", var_decl.range());
    //     println!("{:?}", var_decl.text());
    //     println!("{:?}", var_decl.var_token());
    //     println!("{:?}", var_decl.var_token().map(|v| v.text_range()));
    // }

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
    for value in value.elements() {
        let ExprOrSpread::Expr(Expr::ObjectExpr(obj)) = value else {
            continue;
        };
        let song = parse_song(&obj).with_context(|| format!("While parsing {}", obj.text()))?;
        println!("{song:?}");
    }

    Ok(())
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

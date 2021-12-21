use std::str::FromStr;

use anyhow::{anyhow, bail, Context};
use chrono::NaiveDateTime;
use scraper::{ElementRef, Html};

use crate::ongeki::schema::latest::*;

pub fn parse(html: &Html) -> anyhow::Result<()> {
    let root_div = html
        .select(selector!("."))
        .next()
        .context("Top level div not found")?;
    let mut root_div_children = root_div.children().filter_map(ElementRef::wrap);

    let not_found = "Element not found";
    root_div_children.next().context(not_found)?;

    let first_div = root_div_children.next().context(not_found)?;
    let _ = parse_first_div(first_div).context("Failed to parse first div")?;
    // (battle_result, technical_result, full_bell_kind, full_combo_kind)

    Ok(())
}

fn parse_first_div(div: ElementRef) -> anyhow::Result<(PlayTime, SongMetadata, ScoreMetadata, PlaylogScoreBlockData)> {
    let mut children = div.children().filter_map(ElementRef::wrap);
    let difficulty = parse_difficulty_img(children.next().context("Difficulty img not found")?)
        .context("Failed to parse difficulty")?;
    let date = parse_play_date(children.next().context("Play date span not found")?)
        .context("Failed to parse play date")?;
    let song_name: SongName = children
        .next()
        .context("Song name div not found")?
        .text()
        .collect::<String>()
        .trim()
        .to_owned()
        .into();
    let cover_art: SongCoverArtUrl = (|| {
        anyhow::Ok(
            src_attr(children.next().context("Cover art img not found")?)?
                .to_owned()
                .parse()?,
        )
    })()
    .context("Failed to get cover art")?;
    let playlog_score_block_data = parse_playlog_score_block(
        children
            .next()
            .context("Playlog score block wrapper div not found")?,
    )?;

    let song_metadata = SongMetadata::builder()
        .name(song_name)
        .cover_art(cover_art)
        .build();
    let score_metadata = ScoreMetadata::builder().difficulty(difficulty).build();

    Ok((date, song_metadata, score_metadata, playlog_score_block_data))
}

fn parse_difficulty_img(img: ElementRef) -> anyhow::Result<ScoreDifficulty> {
    let src = src_attr(img).context("Failed to parse diffficulty img")?;
    use ScoreDifficulty::*;
    Ok(match src {
        "https://ongeki-net.com/ongeki-mobile/img/diff_basic.png" => Basic,
        "https://ongeki-net.com/ongeki-mobile/img/diff_advanced.png" => Advanced,
        "https://ongeki-net.com/ongeki-mobile/img/diff_expert.png" => Expert,
        "https://ongeki-net.com/ongeki-mobile/img/diff_master.png" => Master,
        "https://ongeki-net.com/ongeki-mobile/img/diff_lunatic.png" => Lunatic,
        _ => return Err(anyhow!("Unexpected src: {:?}", src)),
    })
}

fn parse_play_date(span: ElementRef) -> anyhow::Result<PlayTime> {
    Ok(NaiveDateTime::parse_from_str(&span.text().collect::<String>(), "%Y/%m/%d %H:%M")?.into())
}

fn src_attr(element: ElementRef) -> anyhow::Result<&str> {
    element
        .value()
        .attr("src")
        .with_context(|| format!("No src in: {}", element.html()))
}

type PlaylogScoreBlockData = (BattleResult, TechnicalResult, FullBellKind, FullComboKind);
fn parse_playlog_score_block(div: ElementRef) -> anyhow::Result<PlaylogScoreBlockData> {
    let mut scores = div.select(selector!(".f_20"));
    let battle_score = parse_value_with_new_record(
        scores.next().context("No element found for battle score")?,
        parse_comma_separated_integer::<u32, BattleScore>,
    )
    .context("Failed to parse battle score")?;
    let over_damage = parse_value_with_new_record(
        scores.next().context("No element found for over damage")?,
        parse_over_damage,
    )
    .context("Failed to parse over damage")?;
    let technical_score = parse_value_with_new_record(
        scores.next().context("No element found for over damage")?,
        parse_comma_separated_integer::<u32, TechnicalScore>,
    )
    .context("Failed to parse over damage")?;

    let mut images = div.select(selector!("img"));
    let battle_rank = parse_battle_rank(images.next().context("No img found for battle rank")?)
        .context("Failed to parse battle rank")?;
    let technical_rank =
        parse_technical_rank(images.next().context("No img found for technical rank")?)
            .context("Failed to parse technical rank")?;
    let win_or_lose = parse_win_or_lose(images.next().context("No img for win or lose")?)
        .context("Failed to parse win or lose")?;
    let full_bell_kind = parse_full_bell(images.next().context("No img for full bell kind")?)
        .context("Failed to parse full bell kind")?;
    let full_combo_kind = parse_full_combo(images.next().context("No img for full combo kind")?)
        .context("Failed to parse full combo kind")?;

    let battle_result = BattleResult::builder()
        .score(battle_score)
        .over_damage(over_damage)
        .rank(battle_rank)
        .win_or_lose(win_or_lose)
        .build();
    let technical_result = TechnicalResult::builder()
        .score(technical_score)
        .rank(technical_rank)
        .build();

    Ok((
        battle_result,
        technical_result,
        full_bell_kind,
        full_combo_kind,
    ))
}

fn parse_value_with_new_record<T: Copy>(
    element: ElementRef,
    parser: impl FnOnce(&str) -> anyhow::Result<T>,
) -> anyhow::Result<ValueWithNewRecord<T>> {
    let value = parser(&element.text().collect::<String>())?;
    let parent = element
        .parent()
        .with_context(|| format!("No parent for: {}", element.html()))?;
    let is_new_record = ElementRef::wrap(parent)
        .with_context(|| format!("Parent is not an element: {:?}", element.html()))?
        .value()
        .attr("class")
        .with_context(|| format!("Parent has no class: {}", element.html()))?
        .ends_with("_new");
    Ok(ValueWithNewRecord::builder()
        .value(value)
        .new_record(is_new_record)
        .build())
}

fn parse_comma_separated_integer<T, U>(s: &str) -> anyhow::Result<U>
where
    T: FromStr,
    anyhow::Error: From<<T as FromStr>::Err>,
    U: From<T>,
{
    Ok(s.replace(",", "").parse::<T>()?.into())
}

fn parse_over_damage(s: &str) -> anyhow::Result<OverDamage> {
    let captures = regex!(r"([0-9)]+)\.([0-9]{2})")
        .captures(s)
        .with_context(|| format!("Over damage is in an unexpected format: {:?}", s))?;
    (|| {
        let x: u32 = captures[1].parse().ok()?;
        let y: u32 = captures[2].parse().ok()?;
        Some(x.checked_mul(100)?.checked_add(y)?.into())
    })()
    .with_context(|| format!("Too large over damage: {:?}", s))
}

fn parse_battle_rank(img: ElementRef) -> anyhow::Result<BattleRank> {
    use BattleRank::*;
    Ok(match src_attr(img)? {
        // => Bad,
        "https://ongeki-net.com/ongeki-mobile/img/score_br_usually_another.png" => FairLose,
        "https://ongeki-net.com/ongeki-mobile/img/score_br_usually.png" => FairCleared,
        "https://ongeki-net.com/ongeki-mobile/img/score_br_good.png" => Good,
        "https://ongeki-net.com/ongeki-mobile/img/score_br_great.png" => Great,
        "https://ongeki-net.com/ongeki-mobile/img/score_br_excellent.png " => Excellent,
        // => UltimatePlatinum,
        // => UltimateRainbow,
        src => bail!("Unexpected url for battle rank: {:?}", src),
    })
}

fn parse_technical_rank(img: ElementRef) -> anyhow::Result<TechnicalRank> {
    use TechnicalRank::*;
    Ok(match src_attr(img)? {
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_sssplus.png" => SSSPlus,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_sss.png" => SSS,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_ss.png" => SS,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_s.png" => S,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_aaa.png" => AAA,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_aa.png" => AA,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_a.png" => A,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_bbb.png" => BBB,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_bb.png" => BB,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_b.png" => B,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_c.png" => C,
        "https://ongeki-net.com/ongeki-mobile/img/score_tr_d.png" => D,
        src => bail!("Unexpected url for technical rank: {:?}", src),
    })
}

fn parse_win_or_lose(img: ElementRef) -> anyhow::Result<WinOrLose> {
    use WinOrLose::*;
    Ok(match src_attr(img)? {
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_win.png" => Win,
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_draw.png" => Draw,
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_lose.png" => Lose,
        src => bail!("Unexpected url for win or lose: {:?}", src),
    })
}

fn parse_full_bell(img: ElementRef) -> anyhow::Result<FullBellKind> {
    use FullBellKind::*;
    Ok(match src_attr(img)? {
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_fb_base.png" => Nothing,
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_fb.png" => FullBell,
        src => bail!("Unexpected url for full bell: {:?}", src),
    })
}

fn parse_full_combo(img: ElementRef) -> anyhow::Result<FullComboKind> {
    use FullComboKind::*;
    Ok(match src_attr(img)? {
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_fc_base.png" => Nothing,
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_fc.png" => FullCombo,
        "https://ongeki-net.com/ongeki-mobile/img/score_detail_ab.png" => AllBreak,
        src => bail!("Unexpected url for full combo: {:?}", src),
    })
}

use std::str::FromStr;

use anyhow::{bail, Context};
use itertools::Itertools;
use scraper::ElementRef;

use crate::maimai::{
    play_record_parser::{parse_achievement_txt, parse_deluxscore},
    schema::latest::ScoreGeneration,
};

use super::schema::latest::{
    AchievementRank, AchievementValue, FullComboKind, FullSyncKind, ScoreDifficulty, ScoreMetadata,
    ValueWithMax,
};

pub fn parse(html: &scraper::Html, difficulty: ScoreDifficulty) -> anyhow::Result<Vec<ScoreEntry>> {
    html.select(selector!(
        "form[action='https://maimaidx.jp/maimai-mobile/record/musicDetail/']"
    ))
    .map(move |e| parse_entry_form(e, difficulty))
    .collect()
}

fn parse_entry_form(
    entry_form: ElementRef,
    difficulty: ScoreDifficulty,
) -> anyhow::Result<ScoreEntry> {
    let metadata = parse_score_metadata(entry_form, difficulty)?;

    let level: ScoreLevel = entry_form
        .select(selector!("div.music_lv_block"))
        .next()
        .context("Song name not found")?
        .text()
        .collect::<String>()
        .parse()?;
    let song_name: String = entry_form
        .select(selector!("div.music_name_block"))
        .next()
        .context("Song name not found")?
        .text()
        .collect();

    let result = parse_score_result(entry_form)?;

    let idx = ScoreIdx(
        entry_form
            .select(selector!("input"))
            .next()
            .context("Idx input not found")?
            .value()
            .attr("value")
            .context("Idx input does not have `value` attribute")?
            .to_owned(),
    );
    Ok(ScoreEntry {
        metadata,
        song_name,
        level,
        result,
        idx,
    })
}

fn parse_score_metadata(
    entry_form: ElementRef,
    difficulty: ScoreDifficulty,
) -> anyhow::Result<ScoreMetadata> {
    let generation = entry_form
        .parent()
        .and_then(ElementRef::wrap)
        .context("No parent of entry form")?
        .next_siblings()
        .find_map(ElementRef::wrap)
        .context("No sibling of entry form wrapper div")?
        .value()
        .attr("src")
        .context("No src attribute for img")?;
    let generation = if generation.ends_with("music_standard.png") {
        ScoreGeneration::Standard
    } else if generation.ends_with("music_dx.png") {
        ScoreGeneration::Deluxe
    } else {
        bail!("Unexpected src url: {generation:?}")
    };
    Ok(ScoreMetadata::builder()
        .difficulty(difficulty)
        .generation(generation)
        .build())
}

fn parse_score_result(entry_form: ElementRef) -> anyhow::Result<Option<ScoreResult>> {
    let achievement = entry_form
        .select(selector!("div.music_score_block.w_120"))
        .next()
        .map(parse_achievement_txt)
        .transpose()?;
    let deluxscore = entry_form
        .select(selector!("div.music_score_block.w_180"))
        .next()
        .map(parse_deluxscore)
        .transpose()?;
    let images = entry_form
        .select(selector!("img.h_30"))
        .take(3)
        .collect_vec();
    let ranks = match images[..] {
        [full_sync, full_combo, rank] => {
            let full_combo = parse_full_combo_img(full_combo)?;
            let full_sync = parse_full_sync_img(full_sync)?;
            let rank = parse_achievement_rank(rank)?;
            Some((full_combo, full_sync, rank))
        }
        [] => None,
        _ => bail!("Unexpected number of images: {images:?}"),
    };
    Ok(match (achievement, deluxscore, ranks) {
        (Some(a), Some(d), Some((c, s, r))) => Some(ScoreResult {
            achievement: a,
            rank: r,
            deluxscore: d,
            full_combo_kind: c,
            full_sync_kind: s,
        }),
        (None, None, None) => None,
        res => bail!("Inconsistent score result: {res:?}",),
    })
}

fn parse_achievement_rank(achievement_rank: ElementRef) -> anyhow::Result<AchievementRank> {
    use AchievementRank::*;
    let res = match achievement_rank
        .value()
        .attr("src")
        .context("No src was found for achievement rank image")?
    {
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sssp.png?ver=1.35" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sss.png?ver=1.35" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ssp.png?ver=1.35" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ss.png?ver=1.35" => SS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sp.png?ver=1.35" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_s.png?ver=1.35" => S,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aaa.png?ver=1.35" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aa.png?ver=1.35" => AA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_a.png?ver=1.35" => A,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bbb.png?ver=1.35" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bb.png?ver=1.35" => BB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_b.png?ver=1.35" => B,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_c.png?ver=1.35" => C,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_d.png?ver=1.35" => D,
        src => bail!("Unknown src for achievement rank: {src:?}"),
    };
    Ok(res)
}

fn parse_full_combo_img(full_combo_img: ElementRef) -> anyhow::Result<FullComboKind> {
    use FullComboKind::*;
    let res = match full_combo_img
        .value()
        .attr("src")
        .context("No src was found for full sync image")?
    {
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.35" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.35" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.35" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.35" => AllPerfectPlus,
        src => bail!("Unknown src for full combo img: {src:?}"),
    };
    Ok(res)
}

fn parse_full_sync_img(full_sync_img: ElementRef) -> anyhow::Result<FullSyncKind> {
    use FullSyncKind::*;
    let res = match full_sync_img
        .value()
        .attr("src")
        .context("No src was found for full sync image")?
    {
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.35" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.35" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsd.png?ver=1.35" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsdp.png?ver=1.35" => FullSyncDxPlus,
        src => bail!("Unknown src for full sync img: {src:?}"),
    };
    Ok(res)
}

#[derive(Debug)]
pub struct ScoreEntry {
    metadata: ScoreMetadata,
    song_name: String,
    level: ScoreLevel,
    result: Option<ScoreResult>,
    idx: ScoreIdx,
}
#[derive(Debug)]
pub struct ScoreLevel {
    level: u8,
    plus: bool,
}
#[derive(Debug)]
pub struct ScoreResult {
    achievement: AchievementValue,
    rank: AchievementRank,
    deluxscore: ValueWithMax<u32>,
    full_combo_kind: FullComboKind,
    full_sync_kind: FullSyncKind,
}
#[derive(Debug)]
pub struct ScoreIdx(String);

impl FromStr for ScoreLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let stripped = s.strip_suffix('+');
        let level = stripped.unwrap_or(s).parse()?;
        let plus = stripped.is_some();
        match (level, plus) {
            (16.., _) | (15, true) => bail!("Level out of range: {s:?}"),
            _ => Ok(ScoreLevel { level, plus }),
        }
    }
}

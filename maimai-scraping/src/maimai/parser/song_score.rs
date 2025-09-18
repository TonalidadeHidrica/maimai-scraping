use anyhow::{bail, Context};
use derive_more::{Display, From};
use getset::{CopyGetters, Getters};
use itertools::Itertools;
use maimai_scraping_utils::selector;
use scraper::ElementRef;
use serde::{Deserialize, Serialize};

use crate::maimai::{
    parser::play_record::{parse_achievement_txt, parse_deluxscore},
    rating::ScoreLevel,
    schema::latest::{
        AchievementRank, AchievementValue, FullComboKind, FullSyncKind, ScoreGeneration,
        ScoreMetadata, SongName, ValueWithMax,
    },
    song_list::song_score::{self, EntryGroup},
};

use super::play_record::parse_playlog_diff;

pub fn parse(html: &scraper::Html) -> anyhow::Result<Vec<song_score::EntryGroup>> {
    html.select(selector!("div.screw_block"))
        .map(|div| {
            let label = div.text().collect();
            let selector =
                selector!("form[action='https://maimaidx.jp/maimai-mobile/record/musicDetail/']");
            let entries = div
                .next_siblings()
                .filter_map(ElementRef::wrap)
                .map(|x| x.select(selector).next())
                .take_while(|x| x.is_some())
                .flatten()
                .map(parse_entry_form)
                .collect::<anyhow::Result<Vec<_>>>()?;
            anyhow::Ok(EntryGroup { label, entries })
        })
        .collect()
}

fn parse_entry_form(entry_form: ElementRef) -> anyhow::Result<ScoreEntry> {
    let metadata = parse_score_metadata(entry_form)?;
    let level = find_and_parse_score_level(entry_form)?;
    let song_name = find_and_parse_song_name(entry_form)?;
    let result = parse_score_result(entry_form)?;
    let idx = find_and_parse_score_idx(entry_form)?;
    Ok(ScoreEntry {
        metadata,
        song_name,
        level,
        result,
        idx,
    })
}

pub fn find_and_parse_score_level(e: ElementRef) -> anyhow::Result<ScoreLevel> {
    e.select(selector!("div.music_lv_block"))
        .next()
        .context("Song name not found")?
        .text()
        .collect::<String>()
        .parse()
}
pub fn find_and_parse_song_name(e: ElementRef) -> anyhow::Result<SongName> {
    Ok(e.select(selector!("div.music_name_block"))
        .next()
        .context("Song name not found")?
        .text()
        .collect::<String>()
        .into())
}

fn parse_score_metadata(entry_form: ElementRef) -> anyhow::Result<ScoreMetadata> {
    let difficulty = parse_playlog_diff(
        entry_form
            .select(selector!("img.h_20"))
            .next()
            .context("Difficulty img not found")?,
    )?;

    let generation_outside = entry_form
        .parent()
        .and_then(ElementRef::wrap)
        .context("No parent of entry form")?
        .next_siblings()
        .find_map(ElementRef::wrap)
        .context("No sibling of entry form wrapper div")?
        .value()
        .attr("src");
    let generation_inside = entry_form
        .select(selector!("img.music_kind_icon"))
        .next()
        .and_then(|x| x.attr("src"));
    let generation = generation_inside
        .xor(generation_outside)
        .context("Generation img is not found or both exist")?;
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
    let achievement = find_and_parse_achievement_value(entry_form)?;
    let deluxscore = entry_form
        .select(selector!("div.music_score_block.w_190"))
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
        res => bail!("Inconsistent score result: {res:?}\n{}", entry_form.html()),
    })
}

pub fn find_and_parse_achievement_value(e: ElementRef) -> anyhow::Result<Option<AchievementValue>> {
    e.select(selector!(
        "div.music_score_block.w_112,div.music_score_block.w_150"
    ))
    .next()
    .map(parse_achievement_txt)
    .transpose()
}

fn parse_achievement_rank(achievement_rank: ElementRef) -> anyhow::Result<AchievementRank> {
    use AchievementRank::*;
    let res = match achievement_rank
        .value()
        .attr("src")
        .context("No src was found for achievement rank image")?
    {
        // Ver 1.35
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
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sssp.png?ver=1.50" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sss.png?ver=1.50" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ssp.png?ver=1.50" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ss.png?ver=1.50" => SS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sp.png?ver=1.50" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_s.png?ver=1.50" => S,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aaa.png?ver=1.50" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aa.png?ver=1.50" => AA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_a.png?ver=1.50" => A,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bbb.png?ver=1.50" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bb.png?ver=1.50" => BB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_b.png?ver=1.50" => B,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_c.png?ver=1.50" => C,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_d.png?ver=1.50" => D,
        // Ver 1.55
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sssp.png?ver=1.55" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sss.png?ver=1.55" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ssp.png?ver=1.55" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ss.png?ver=1.55" => SS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sp.png?ver=1.55" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_s.png?ver=1.55" => S,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aaa.png?ver=1.55" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aa.png?ver=1.55" => AA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_a.png?ver=1.55" => A,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bbb.png?ver=1.55" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bb.png?ver=1.55" => BB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_b.png?ver=1.55" => B,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_c.png?ver=1.55" => C,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_d.png?ver=1.55" => D,
        // Ver 1.59
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sssp.png?ver=1.59" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sss.png?ver=1.59" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ssp.png?ver=1.59" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ss.png?ver=1.59" => SS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sp.png?ver=1.59" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_s.png?ver=1.59" => S,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aaa.png?ver=1.59" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aa.png?ver=1.59" => AA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_a.png?ver=1.59" => A,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bbb.png?ver=1.59" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bb.png?ver=1.59" => BB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_b.png?ver=1.59" => B,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_c.png?ver=1.59" => C,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_d.png?ver=1.59" => D,
        // Ver 1.60
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sssp.png?ver=1.60" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sss.png?ver=1.60" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ssp.png?ver=1.60" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ss.png?ver=1.60" => SS,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sp.png?ver=1.60" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_s.png?ver=1.60" => S,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aaa.png?ver=1.60" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_aa.png?ver=1.60" => AA,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_a.png?ver=1.60" => A,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bbb.png?ver=1.60" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_bb.png?ver=1.60" => BB,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_b.png?ver=1.60" => B,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_c.png?ver=1.60" => C,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_d.png?ver=1.60" => D,
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
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.35" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.35" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.35" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.35" => AllPerfectPlus,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.50" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.50" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.50" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.50" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.50" => AllPerfectPlus,
        // Ver 1.55
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.55" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.55" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.55" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.55" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.55" => AllPerfectPlus,
        // Ver 1.59
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.59" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.59" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.59" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.59" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.59" => AllPerfectPlus,
        // Ver 1.60
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.60" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fc.png?ver=1.60" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fcp.png?ver=1.60" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_ap.png?ver=1.60" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_app.png?ver=1.60" => AllPerfectPlus,
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
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.35" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.35" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdx.png?ver=1.35" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdxp.png?ver=1.35" => FullSyncDxPlus,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.50" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sync.png?ver=1.50" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.50" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.50" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdx.png?ver=1.50" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdxp.png?ver=1.50" => FullSyncDxPlus,
        // Ver 1.55
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.55" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sync.png?ver=1.55" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.55" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.55" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdx.png?ver=1.55" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdxp.png?ver=1.55" => FullSyncDxPlus,
        // Ver 1.59
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.59" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sync.png?ver=1.59" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.59" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.59" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdx.png?ver=1.59" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdxp.png?ver=1.59" => FullSyncDxPlus,
        // Ver 1.60
        "https://maimaidx.jp/maimai-mobile/img/music_icon_back.png?ver=1.60" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_sync.png?ver=1.60" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fs.png?ver=1.60" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fsp.png?ver=1.60" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdx.png?ver=1.60" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/music_icon_fdxp.png?ver=1.60" => FullSyncDxPlus,
        src => bail!("Unknown src for full sync img: {src:?}"),
    };
    Ok(res)
}

pub fn find_and_parse_score_idx(e: ElementRef) -> anyhow::Result<ScoreIdx> {
    Ok(ScoreIdx(
        e.select(selector!("input"))
            .next()
            .context("Idx input not found")?
            .value()
            .attr("value")
            .context("Idx input does not have `value` attribute")?
            .to_owned(),
    ))
}

#[derive(Debug, Serialize, Deserialize, Getters, CopyGetters)]
pub struct ScoreEntry {
    #[getset(get = "pub")]
    metadata: ScoreMetadata,
    #[getset(get = "pub")]
    song_name: SongName,
    #[getset(get_copy = "pub")]
    level: ScoreLevel,
    #[getset(get = "pub")]
    result: Option<ScoreResult>,
    #[getset(get = "pub")]
    idx: ScoreIdx,
}
#[derive(Debug, Serialize, Deserialize, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct ScoreResult {
    achievement: AchievementValue,
    rank: AchievementRank,
    deluxscore: ValueWithMax<u32>,
    full_combo_kind: FullComboKind,
    full_sync_kind: FullSyncKind,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug, From, Display, Serialize, Deserialize)]
pub struct ScoreIdx(String);

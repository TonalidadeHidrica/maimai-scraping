use std::ops::Deref;
use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};

use anyhow::{anyhow, bail};
use chrono::NaiveDateTime;
use itertools::Itertools;
use maimai_scraping_utils::{regex, selector};
use scraper::{ElementRef, Html};

use crate::maimai::schema::latest::*;

pub type RecordIndexData = (PlayTime, Idx);

pub fn parse_record_index(html: &Html) -> anyhow::Result<Vec<RecordIndexData>> {
    let mut res = vec![];
    for playlog_top_container in iterate_playlot_top_containers(html) {
        let playlog_main_container = playlog_top_container
            .next_siblings()
            .find_map(ElementRef::wrap)
            .ok_or_else(|| anyhow!("Next sibling was not found."))?;

        let idx = parse_idx_from_playlog_main_container(playlog_main_container)?;
        let play_date = match idx.timestamp_jst() {
            Some(x) => x,
            // Support old version where timestamp is not included in idx
            None => parse_playlog_top_conatiner(playlog_top_container)?.5,
        };
        res.push((play_date, idx));
    }
    Ok(res)
}

fn parse_idx_from_playlog_main_container(playlog_top_container: ElementRef) -> anyhow::Result<Idx> {
    playlog_top_container
        .select(selector!("input[name='idx']"))
        .next()
        .ok_or_else(|| anyhow!("idx not found"))?
        .value()
        .attr("value")
        .ok_or_else(|| anyhow!("idx does not have 'value' attr"))?
        .parse()
        // .map_err(|e| anyhow!("Expected integer for idx but found: {}", e))?
        // .try_into()
        .map_err(|e| anyhow!("Could not parse Idx: {e:?}"))
}

pub fn parse(html: &Html, idx: Idx, place_expected: bool) -> anyhow::Result<PlayRecord> {
    let playlog_top_container = iterate_playlot_top_containers(html)
        .next()
        .ok_or_else(|| anyhow!("Playlog top container was not found."))?;
    let (difficulty, utage_metadata, battle_kind, battle_win_or_lose, track_index, play_date) =
        parse_playlog_top_conatiner(playlog_top_container)?;

    let playlog_main_container = playlog_top_container
        .next_siblings()
        .find_map(ElementRef::wrap)
        .ok_or_else(|| anyhow!("Next sibling was not found."))?;
    let (
        song_metadata,
        cleared,
        generation,
        achievement_result,
        deluxscore_result,
        full_combo_kind,
        full_sync_kind,
        matching_rank,
        life_result,
    ) = parse_playlog_main_container(playlog_main_container)?;
    let generation = match (difficulty, generation) {
        (_, Some(generation)) => generation,
        (ScoreDifficulty::Utage, None) => ScoreGeneration::Deluxe, // TODO: is this correct?
        _ => bail!("Score generation icon not found"),
    };

    let battle_opponent = html
        .select(selector!("#vsUser"))
        .next()
        .map(parse_vs_user)
        .transpose()?;

    let place_name = html
        .select(selector!("#placeName > span"))
        .next()
        .map(|place_name_div| PlaceName::from(place_name_div.text().collect::<String>()));
    if place_expected && place_name.is_none() {
        bail!("Place name expected, but not found")
    }

    let gray_block = playlog_top_container
        .parent()
        .ok_or_else(|| anyhow!("No parent found for playlog top container"))?
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .find(|e| selector!(".gray_block").matches(e))
        .ok_or_else(|| anyhow!("Gray block was not found"))?;
    let (tour_members, judge_count, rating_result, max_combo, max_sync) =
        parse_center_gray_block(gray_block)?;

    let other_players = html
        .select(selector!("#matching"))
        .next()
        .map(parse_matching_div)
        .transpose()?;

    let played_at = PlayedAt::builder()
        .time(play_date)
        .place(place_name)
        .track(track_index)
        .idx(idx)
        .build();
    let score_metadata = ScoreMetadata::builder()
        .generation(generation)
        .difficulty(difficulty)
        .build();
    let combo_result = ComboResult::builder()
        .full_combo_kind(full_combo_kind)
        .combo(max_combo)
        .build();

    let battle_result = match (battle_kind, battle_win_or_lose, battle_opponent) {
        (Some(kind), Some(win_or_lose), Some((kind2, opponent))) if kind == kind2 => {
            BattleResult::builder()
                .kind(kind)
                .win_or_lose(win_or_lose)
                .opponent(opponent)
                .build()
                .into()
        }
        (None, None, None) => None,
        otherwise => return Err(anyhow!("Inconsistent battle result: {:?}", otherwise)),
    };

    let matching_result = match (full_sync_kind, max_sync, other_players, matching_rank) {
        (FullSyncKind::Nothing, None, None, None) => None,
        (full_sync_kind, Some(max_sync), Some(other_players), Some(rank)) => {
            MatchingResult::builder()
                .full_sync_kind(full_sync_kind)
                .max_sync(max_sync)
                .other_players(other_players)
                .rank(rank)
                .build()
                .into()
        }
        otherwise => return Err(anyhow!("Inconsistent matching result: {:?}", otherwise)),
    };

    let res = PlayRecord::builder()
        .played_at(played_at)
        .song_metadata(song_metadata)
        .score_metadata(score_metadata)
        .utage_metadata(utage_metadata)
        .cleared(cleared)
        .achievement_result(achievement_result)
        .deluxscore_result(deluxscore_result)
        .combo_result(combo_result)
        .battle_result(battle_result)
        .matching_result(matching_result)
        .tour_members(tour_members)
        .rating_result(rating_result)
        .judge_result(judge_count)
        .life_result(life_result)
        .build();
    Ok(res)
}

fn iterate_playlot_top_containers(html: &Html) -> impl Iterator<Item = ElementRef> {
    html.select(selector!(".playlog_top_container"))
}

#[allow(clippy::type_complexity)]
fn parse_playlog_top_conatiner(
    div: ElementRef,
) -> anyhow::Result<(
    ScoreDifficulty,
    Option<UtageMetadata>,
    Option<BattleKind>,
    Option<BattleWinOrLose>,
    TrackIndex,
    PlayTime,
)> {
    let difficulty = parse_playlog_diff(
        div.select(selector!("img.playlog_diff"))
            .next()
            .ok_or_else(|| anyhow!("Difficulty image was not found."))?,
    )?;

    let utage_metadata = div
        .select(selector!("div.playlog_music_kind_icon_utage"))
        .next()
        .map(parse_utage_metadata)
        .transpose()?;

    let battle_kind = div
        .select(selector!("img.playlog_vs"))
        .next()
        .map(parse_battle_kind_img)
        .transpose()?;

    let battle_win_or_lose = div
        .select(selector!("img.playlog_vs_result"))
        .next()
        .map(parse_playlog_vs_result)
        .transpose()?;

    let mut spans = div.select(selector!("div.sub_title > span"));

    let track_index = parse_track_index(
        spans
            .next()
            .ok_or_else(|| anyhow!("Track index span was not found."))?,
    )?;
    let play_date = parse_play_date(
        spans
            .next()
            .ok_or_else(|| anyhow!("Play date span was not found."))?,
    )?;

    Ok((
        difficulty,
        utage_metadata,
        battle_kind,
        battle_win_or_lose,
        track_index,
        play_date,
    ))
}

pub fn parse_utage_metadata(div: ElementRef) -> anyhow::Result<UtageMetadata> {
    let utage_back_div = div
        .select(selector!(
            r#"img[src="https://maimaidx.jp/maimai-mobile/img/music_utage.png"],
               img[src="https://maimaidx-eng.com/maimai-mobile/img/music_utage.png"]"#
        ))
        .next()
        .ok_or_else(|| anyhow!("Utage kind icon not found"))?;
    let utage_kind = match &utage_back_div
        .next_siblings()
        .find_map(ElementRef::wrap)
        .ok_or_else(|| anyhow!("No succeeding element after utage kind icon found"))?
        .text()
        .collect::<String>()[..]
    {
        "光" => UtageKind::AllBreak,
        "協" => UtageKind::Collaborative,
        "狂" => UtageKind::Insane,
        "蛸" => UtageKind::ManyHands,
        "覚" => UtageKind::Memorize,
        "宴" => UtageKind::Miscellaneous,
        "蔵" => UtageKind::Shelved,
        "星" => UtageKind::Slides,
        kind => UtageKind::Raw(kind.to_owned().into()),
    };
    let buddy = div
        .select(selector!(
            r#"img[src="https://maimaidx.jp/maimai-mobile/img/music_utage_buddy.png"],
               img[src="https://maimaidx-eng.com/maimai-mobile/img/music_utage_buddy.png"]"#
        ))
        .next()
        .is_some();
    Ok(UtageMetadata::builder()
        .kind(utage_kind)
        .buddy(buddy)
        .build())
}

pub fn parse_score_generation_img(img: ElementRef) -> anyhow::Result<ScoreGeneration> {
    use ScoreGeneration::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/music_dx.png") => Ok(Deluxe),
        Some("https://maimaidx.jp/maimai-mobile/img/music_standard.png") => Ok(Standard),
        // International
        Some("https://maimaidx-eng.com/maimai-mobile/img/music_dx.png") => Ok(Deluxe),
        Some("https://maimaidx-eng.com/maimai-mobile/img/music_standard.png") => Ok(Standard),
        url => Err(anyhow!(
            "Unexpected image source for music generation: {:?}",
            url
        )),
    }
}

pub fn parse_playlog_diff(img: ElementRef) -> anyhow::Result<ScoreDifficulty> {
    use ScoreDifficulty::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/diff_basic.png") => Ok(Basic),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_advanced.png") => Ok(Advanced),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_expert.png") => Ok(Expert),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_master.png") => Ok(Master),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_remaster.png") => Ok(ReMaster),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_utage.png") => Ok(Utage),
        // International
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_basic.png") => Ok(Basic),
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_advanced.png") => Ok(Advanced),
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_expert.png") => Ok(Expert),
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_master.png") => Ok(Master),
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_remaster.png") => Ok(ReMaster),
        Some("https://maimaidx-eng.com/maimai-mobile/img/diff_utage.png") => Ok(Utage),
        url => Err(anyhow!("Unexpected difficulty image: {:?}", url)),
    }
}

fn parse_track_index(span: ElementRef) -> anyhow::Result<TrackIndex> {
    Ok(regex!(r"TRACK ([0-9]{2})")
        .captures(&span.text().collect::<String>())
        .ok_or_else(|| anyhow!("The format of track index was invalid."))?
        .get(1)
        .expect("There is a group in the pattern")
        .as_str()
        .parse::<u8>()
        .expect("The captured pattern is an integer")
        .try_into()
        .expect("The value is within the range of 1-4"))
}

fn parse_play_date(span: ElementRef) -> anyhow::Result<PlayTime> {
    Ok(NaiveDateTime::parse_from_str(&span.text().collect::<String>(), "%Y/%m/%d %H:%M")?.into())
}

#[allow(clippy::type_complexity)]
fn parse_playlog_main_container(
    playlog_main_container: ElementRef,
) -> anyhow::Result<(
    SongMetadata,
    bool,
    Option<ScoreGeneration>,
    AchievementResult,
    DeluxscoreResult,
    FullComboKind,
    FullSyncKind,
    Option<MatchingRank>,
    LifeResult,
)> {
    let basic_block = playlog_main_container
        .select(selector!(".basic_block"))
        .next()
        .ok_or_else(|| anyhow!("No basic_block was found"))?;
    let song_title = basic_block.text().collect::<String>().into();

    let cleared = match basic_block
        .select(selector!("img"))
        .next()
        .map(|e| e.value().attr("src"))
    {
        Some(Some("https://maimaidx.jp/maimai-mobile/img/playlog/clear.png")) => true,
        // International
        Some(Some("https://maimaidx-eng.com/maimai-mobile/img/playlog/clear.png")) => true,
        Some(src) => return Err(anyhow!("Unexpected image source for cleared: {:?}", src)),
        _ => false,
    };

    let music_img_src = playlog_main_container
        .select(selector!("img.music_img"))
        .next()
        .ok_or_else(|| anyhow!("music_img was not found"))?
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("Music img doesn't have src"))?;

    let generation = playlog_main_container
        .select(selector!("img.playlog_music_kind_icon"))
        .next()
        .map(parse_score_generation_img)
        .transpose()?;

    let playlog_result_block = playlog_main_container
        .select(selector!(".playlog_result_block"))
        .next()
        .ok_or_else(|| anyhow!("playlog result block was not found"))?;
    let (
        achievement_result,
        deluxscore_result,
        full_combo_kind,
        full_sync_kind,
        matching_rank,
        life_result,
    ) = parse_playlog_result_block(playlog_result_block)?;

    let song_metadata = SongMetadata::builder()
        .name(song_title)
        .cover_art(music_img_src.parse()?)
        .build();

    Ok((
        song_metadata,
        cleared,
        generation,
        achievement_result,
        deluxscore_result,
        full_combo_kind,
        full_sync_kind,
        matching_rank,
        life_result,
    ))
}

fn parse_playlog_result_block(
    playlog_result_block: ElementRef,
) -> anyhow::Result<(
    AchievementResult,
    DeluxscoreResult,
    FullComboKind,
    FullSyncKind,
    Option<MatchingRank>,
    LifeResult,
)> {
    let achievement_is_new_record = playlog_result_block
        .select(selector!("img.playlog_achievement_newrecord"))
        .next()
        .is_some();

    let achievement_value = parse_achievement_txt(
        playlog_result_block
            .select(selector!("div.playlog_achievement_txt"))
            .next()
            .ok_or_else(|| anyhow!("Achievement text was not found"))?,
    )?;
    let achievement_rank = parse_achievement_rank(
        playlog_result_block
            .select(selector!("img.playlog_scorerank"))
            .next()
            .ok_or_else(|| anyhow!("Achievement scorerank was not found"))?,
    )?;
    let achievement_result = AchievementResult::builder()
        .new_record(achievement_is_new_record)
        .value(achievement_value)
        .rank(achievement_rank)
        .build();

    let (deluxscore_result, full_combo_kind, full_sync_kind, matching_rank) =
        parse_playlog_result_innerblock(
            playlog_result_block
                .select(selector!("div.playlog_result_innerblock"))
                .next()
                .ok_or_else(|| anyhow!("playlog result innerblock was not found"))?,
        )?;

    let life_result = match playlog_result_block
        .select(selector!("div.playlog_life_block"))
        .next()
    {
        Some(element) => parse_life_block(element)?,
        None => LifeResult::Nothing,
    };

    Ok((
        achievement_result,
        deluxscore_result,
        full_combo_kind,
        full_sync_kind,
        matching_rank,
        life_result,
    ))
}

pub fn parse_achievement_txt(achievement_txt: ElementRef) -> anyhow::Result<AchievementValue> {
    let text = achievement_txt.text().collect::<String>();
    let captures = regex!(r"^([0-9]{1,3})\.([0-9]{4})%$")
        .captures(&text)
        .ok_or_else(|| anyhow!("Unexpected format of achievement"))?;
    let integral: u32 = captures
        .get(1)
        .expect("There is group 1 in the pattern")
        .as_str()
        .parse()
        .expect("Pattern is always integral");
    let fractional: u32 = captures
        .get(2)
        .expect("There is group 2 in the pattern")
        .as_str()
        .parse()
        .expect("Pattern is always integral");
    let value = AchievementValue::try_from(integral * 10000 + fractional)
        .map_err(|e| anyhow!("Out of bounds: {}", e))?;
    Ok(value)
}

fn parse_achievement_rank(achievement_rank: ElementRef) -> anyhow::Result<AchievementRank> {
    use AchievementRank::*;
    let res = match achievement_rank
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src found in achievement image"))?
    {
        // Ver 1.15
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.15" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.15" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.15" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.15" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.15" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.15" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.15" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.15" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.15" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.15" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.15" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.15" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.15" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.15" => D,
        // Ver 1.17
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.17" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.17" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.17" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.17" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.17" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.17" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.17" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.17" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.17" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.17" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.17" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.17" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.17" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.17" => D,
        // Ver 1.20
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.20" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.20" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.20" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.20" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.20" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.20" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.20" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.20" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.20" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.20" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.20" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.20" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.20" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.20" => D,
        // Ver 1.25
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.25" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.25" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.25" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.25" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.25" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.25" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.25" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.25" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.25" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.25" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.25" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.25" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.25" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.25" => D,
        // Ver 1.30
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.30" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.30" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.30" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.30" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.30" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.30" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.30" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.30" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.30" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.30" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.30" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.30" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.30" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.30" => D,
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.35" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.35" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.35" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.35" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.35" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.35" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.35" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.35" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.35" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.35" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.35" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.35" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.35" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.35" => D,
        // Ver 1.40
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.40" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.40" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.40" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.40" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.40" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.40" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.40" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.40" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.40" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.40" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.40" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.40" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.40" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.40" => D,
        // Ver 1.45
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.45" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.45" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.45" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.45" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.45" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.45" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.45" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.45" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.45" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.45" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.45" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.45" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.45" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.45" => D,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/playlog/sssplus.png?ver=1.50" => SSSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sss.png?ver=1.50" => SSS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ssplus.png?ver=1.50" => SSPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ss.png?ver=1.50" => SS,
        "https://maimaidx.jp/maimai-mobile/img/playlog/splus.png?ver=1.50" => SPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/s.png?ver=1.50" => S,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aaa.png?ver=1.50" => AAA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/aa.png?ver=1.50" => AA,
        "https://maimaidx.jp/maimai-mobile/img/playlog/a.png?ver=1.50" => A,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bbb.png?ver=1.50" => BBB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/bb.png?ver=1.50" => BB,
        "https://maimaidx.jp/maimai-mobile/img/playlog/b.png?ver=1.50" => B,
        "https://maimaidx.jp/maimai-mobile/img/playlog/c.png?ver=1.50" => C,
        "https://maimaidx.jp/maimai-mobile/img/playlog/d.png?ver=1.50" => D,
        // International
        // Ver 1.35
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/sssplus.png?ver=1.35" => SSSPlus,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/sss.png?ver=1.35" => SSS,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/ssplus.png?ver=1.35" => SSPlus,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/ss.png?ver=1.35" => SS,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/splus.png?ver=1.35" => SPlus,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/s.png?ver=1.35" => S,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/aaa.png?ver=1.35" => AAA,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/aa.png?ver=1.35" => AA,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/a.png?ver=1.35" => A,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/bbb.png?ver=1.35" => BBB,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/bb.png?ver=1.35" => BB,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/b.png?ver=1.35" => B,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/c.png?ver=1.35" => C,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/d.png?ver=1.35" => D,
        src => return Err(anyhow!("Unknown url: {}", src)),
    };
    Ok(res)
}

fn parse_playlog_result_innerblock(
    playlog_result_innerblock: ElementRef,
) -> anyhow::Result<(
    DeluxscoreResult,
    FullComboKind,
    FullSyncKind,
    Option<MatchingRank>,
)> {
    let playlog_score_block = playlog_result_innerblock
        .select(selector!(".playlog_score_block"))
        .next()
        .ok_or_else(|| anyhow!("playlog score block not found"))?;

    let is_new_record = playlog_score_block
        .select(selector!(".playlog_deluxscore_newrecord"))
        .next()
        .is_some();

    let deluxscore_div = playlog_score_block
        .select(selector!("div"))
        .next()
        .ok_or_else(|| anyhow!("No deluxscore div was found"))?;
    let deluxscore = parse_deluxscore(deluxscore_div)?;

    let dxstar_img = deluxscore_div
        .next_siblings()
        .flat_map(ElementRef::wrap)
        .next();
    let dxstar = match dxstar_img {
        Some(dxstar_img) => parse_dxstar(dxstar_img)?,
        None => DeluxscoreRank::try_from(0).expect("Rank 0 is valid"),
    };

    let deluxscore_result = DeluxscoreResult::builder()
        .new_record(is_new_record)
        .score(deluxscore)
        .rank(dxstar)
        .build();

    let mut imgs = playlog_score_block
        .next_siblings()
        .flat_map(ElementRef::wrap)
        .filter(|e| selector!("img").matches(e));

    let full_combo_kind = parse_full_combo_img(
        imgs.next()
            .ok_or_else(|| anyhow!("Full combo image was not found"))?,
    )?;

    let full_sync_kind = parse_full_sync_img(
        imgs.next()
            .ok_or_else(|| anyhow!("Full combo image was not found"))?,
    )?;

    let matching_rank = imgs.next().map(parse_matching_rank_img).transpose()?;

    Ok((
        deluxscore_result,
        full_combo_kind,
        full_sync_kind,
        matching_rank,
    ))
}

pub fn parse_deluxscore(deluxe_score_div: ElementRef) -> anyhow::Result<ValueWithMax<u32>> {
    let text = deluxe_score_div.text().collect::<String>();
    let captures = regex!(r"^([0-9,]+) / ([0-9,]+)$")
        .captures(text.trim())
        .ok_or_else(|| anyhow!("Invalid deluxscore format: {:?}", text))?;
    let a = parse_integer_with_camma(captures.get(1).expect("Group 1 exists").as_str())?;
    let b = parse_integer_with_camma(captures.get(2).expect("Group 2 exists").as_str())?;
    ValueWithMax::new(a, b).map_err(|res| anyhow!("Value is larger than full: {:?}", res))
}

fn parse_integer_with_camma<F: FromStr>(s: &str) -> Result<F, F::Err> {
    s.replace(',', "").parse()
}

fn parse_dxstar(dxstar_img: ElementRef) -> anyhow::Result<DeluxscoreRank> {
    let res = match dxstar_img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src in provided element"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/playlog/dxstar_1.png" => 1,
        "https://maimaidx.jp/maimai-mobile/img/playlog/dxstar_2.png" => 2,
        "https://maimaidx.jp/maimai-mobile/img/playlog/dxstar_3.png" => 3,
        "https://maimaidx.jp/maimai-mobile/img/playlog/dxstar_4.png" => 4,
        "https://maimaidx.jp/maimai-mobile/img/playlog/dxstar_5.png" => 5,
        // International
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/dxstar_1.png" => 1,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/dxstar_2.png" => 2,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/dxstar_3.png" => 3,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/dxstar_4.png" => 4,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/dxstar_5.png" => 5,
        src => return Err(anyhow!("Unknown src for dxstar: {}", src)),
    };
    Ok(res.try_into().expect("value is always valid"))
}

fn parse_full_combo_img(full_combo_img: ElementRef) -> anyhow::Result<FullComboKind> {
    use FullComboKind::*;
    let res = match full_combo_img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src was found for full combo image"))?
    {
        // Ver 1.15
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.15" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.15" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.15" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.15" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.15" => AllPerfectPlus,
        // Ver 1.17
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.17" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.17" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.17" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.17" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.17" => AllPerfectPlus,
        // Ver 1.20
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.20" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.20" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.20" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.20" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.20" => AllPerfectPlus,
        // Ver 1.25
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.25" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.25" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.25" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.25" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.25" => AllPerfectPlus,
        // Ver 1.30
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.30" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.30" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.30" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.30" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.30" => AllPerfectPlus,
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.35" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.35" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.35" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.35" => AllPerfectPlus,
        // Ver 1.40
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.40" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.40" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.40" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.40" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.40" => AllPerfectPlus,
        // Ver 1.45
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.45" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.45" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.45" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.45" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.45" => AllPerfectPlus,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.50" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.50" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.50" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.50" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.50" => AllPerfectPlus,
        // International
        // Ver 1.35
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fc_dummy.png?ver=1.35" => Nothing,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fc.png?ver=1.35" => FullCombo,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fcplus.png?ver=1.35" => FullComboPlus,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/ap.png?ver=1.35" => AllPerfect,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/applus.png?ver=1.35" => AllPerfectPlus,
        src => return Err(anyhow!("Unknown src for full combo img: {}", src)),
    };
    Ok(res)
}

fn parse_full_sync_img(full_sync_img: ElementRef) -> anyhow::Result<FullSyncKind> {
    use FullSyncKind::*;
    let res = match full_sync_img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src was found for full sync image"))?
    {
        // Ver 1.15
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.15" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.15" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.15" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.15" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.15" => FullSyncDxPlus,
        // Ver 1.17
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.17" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.17" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.17" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.17" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.17" => FullSyncDxPlus,
        // Ver 1.20
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.20" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.20" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.20" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.20" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.20" => FullSyncDxPlus,
        // Ver 1.25
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.25" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.25" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.25" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.25" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.25" => FullSyncDxPlus,
        // Ver 1.30
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.30" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.30" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.30" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.30" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.30" => FullSyncDxPlus,
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.35" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.35" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.35" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.35" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.35" => FullSyncDxPlus,
        // Ver 1.40
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync_dummy.png?ver=1.40" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync.png?ver=1.40" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.40" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.40" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.40" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.40" => FullSyncDxPlus,
        // Ver 1.45
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync_dummy.png?ver=1.45" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync.png?ver=1.45" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.45" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.45" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.45" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.45" => FullSyncDxPlus,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync_dummy.png?ver=1.50" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/sync.png?ver=1.50" => SyncPlay,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.50" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.50" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.50" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.50" => FullSyncDxPlus,
        // International
        // Ver 1.35
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/sync_dummy.png?ver=1.35" => Nothing,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/sync.png?ver=1.35" => SyncPlay,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fs.png?ver=1.35" => FullSync,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fsplus.png?ver=1.35" => FullSyncPlus,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fsd.png?ver=1.35" => FullSyncDx,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/fsdplus.png?ver=1.35" => FullSyncDxPlus,
        src => return Err(anyhow!("Unknown src for full sync img: {}", src)),
    };
    Ok(res)
}

fn parse_matching_rank_img(matching_rank_img: ElementRef) -> anyhow::Result<MatchingRank> {
    let res = match matching_rank_img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src was found for matching rank img"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/playlog/1st.png" => 1,
        "https://maimaidx.jp/maimai-mobile/img/playlog/2nd.png" => 2,
        "https://maimaidx.jp/maimai-mobile/img/playlog/3rd.png" => 3,
        "https://maimaidx.jp/maimai-mobile/img/playlog/4th.png" => 4,
        // International
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/1st.png" => 1,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/2nd.png" => 2,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/3rd.png" => 3,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/4th.png" => 4,
        src => return Err(anyhow!("Unknown src for matching rank img: {}", src)),
    };
    Ok(res.try_into().expect("Value is always in the bounds"))
}

fn parse_value_with_max<T>(text: &str) -> anyhow::Result<ValueWithMax<T>>
where
    T: PartialOrd + Copy + std::fmt::Debug + FromStr,
    <T as FromStr>::Err: Send + Sync + std::error::Error + 'static,
{
    let captures = regex!(r"^([0-9,]+)/([0-9,]+)$")
        .captures(text)
        .ok_or_else(|| {
            anyhow!(
                "Invalid life block / max combo / max sync format: {:?}",
                text
            )
        })?;
    let a = parse_integer_with_camma(captures.get(1).expect("Group 1 exists").as_str())?;
    let b = parse_integer_with_camma(captures.get(2).expect("Group 2 exists").as_str())?;
    let res =
        ValueWithMax::new(a, b).map_err(|res| anyhow!("Value is larger than full: {:?}", res))?;
    Ok(res)
}

#[allow(clippy::type_complexity)]
fn parse_center_gray_block(
    gray_block: ElementRef,
) -> anyhow::Result<(
    TourMemberList,
    JudgeResult,
    RatingResult,
    ValueWithMax<u32>,
    Option<ValueWithMax<u32>>,
)> {
    let tour_members: TourMemberList = gray_block
        .select(selector!("div.playlog_chara_container"))
        .map(parse_chara_container)
        .filter_map(Result::transpose)
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|e| anyhow!("Unexpected number of members: {:?}", e))?;

    let (fast, late) = parse_fl_block(
        gray_block
            .select(selector!("div.playlog_fl_block"))
            .next()
            .ok_or_else(|| anyhow!("Fast-Late block was not found"))?,
    )?;

    let mut trs = gray_block
        .select(selector!("table.playlog_notes_detail"))
        .next()
        .ok_or_else(|| anyhow!("Notes detail table was not found"))?
        .select(selector!("tr:not(:first-child)"));
    let mut get_count = |label: &str| {
        let tr = trs
            .next()
            .ok_or_else(|| anyhow!("Table row for {} was not found", label))?;
        parse_judge_count_row(tr)
    };
    let tap = get_count("tap")?;
    let hold = get_count("hold")?;
    let slide = get_count("slide")?;
    let touch = get_count("touch")?;
    let break_ = match get_count("break")? {
        JudgeCount::JudgeCountWithCP(count) => count,
        e => {
            return Err(anyhow!(
                "Count for break does not have critical perfect count: {:?}",
                e
            ))
        }
    };

    let judge_count = JudgeResult::builder()
        .fast(fast)
        .late(late)
        .tap(tap)
        .hold(hold)
        .slide(slide)
        .touch(touch)
        .break_(break_)
        .build();

    let rating_result = parse_rating_deatil_block(
        gray_block
            .select(selector!(".playlog_rating_detail_block"))
            .next()
            .ok_or_else(|| anyhow!("Rating detail block was not found"))?,
    )?;

    let mut playlog_score_blocks = gray_block.select(selector!("div.playlog_score_block"));
    let max_combo_div = playlog_score_blocks
        .next()
        .ok_or_else(|| anyhow!("Max combo block was not found"))?;
    let max_combo = parse_max_combo_sync_div(max_combo_div)?
        .ok_or_else(|| anyhow!("Max combo was not found, hyphen found instead"))?;
    let max_sync_div = playlog_score_blocks
        .next()
        .ok_or_else(|| anyhow!("Max sync block was not found"))?;
    let max_sync = parse_max_combo_sync_div(max_sync_div)?;

    Ok((
        tour_members,
        judge_count,
        rating_result,
        max_combo,
        max_sync,
    ))
}

fn parse_chara_container(chara_container: ElementRef) -> anyhow::Result<Option<TourMember>> {
    let img_url = match chara_container
        .select(selector!("img.chara_cycle_img"))
        .next()
    {
        None => return Ok(None),
        Some(img) => img
            .value()
            .attr("src")
            .ok_or_else(|| anyhow!("Chara img does not have src")),
    }?;
    let img_url = img_url
        .parse()
        .map_err(|e| anyhow!("Invalid chara img url: {:?}", e))?;

    let star = parse_chara_star_block(
        chara_container
            .select(selector!("div.playlog_chara_star_block"))
            .next()
            .ok_or_else(|| anyhow!("No chara star block found"))?,
    )?;

    let level = parse_chara_lv_block(
        chara_container
            .select(selector!("div.playlog_chara_lv_block"))
            .next()
            .ok_or_else(|| anyhow!("No chara lv block found"))?,
    )?;

    let res = TourMember::builder()
        .icon(img_url)
        .star(star)
        .level(level)
        .build();

    Ok(Some(res))
}

fn parse_chara_star_block(chara_star_block: ElementRef) -> anyhow::Result<u32> {
    let text = chara_star_block.text().collect::<String>();
    regex!(r"^×([0-9]+)$")
        .captures(&text)
        .ok_or_else(|| anyhow!("Unexpected format of chara stars"))?
        .get(1)
        .expect("Group 1 always exists")
        .as_str()
        .parse()
        .map_err(|e| anyhow!("Value out of bounds: {}", e))
}

fn parse_chara_lv_block(chara_lv_block: ElementRef) -> anyhow::Result<u32> {
    let text = chara_lv_block.text().collect::<String>();
    regex!(r"^Lv([0-9]+)$")
        .captures(&text)
        .ok_or_else(|| anyhow!("Unexpected format of chara level"))?
        .get(1)
        .expect("Group 1 always exists")
        .as_str()
        .parse()
        .map_err(|e| anyhow!("Value out of bounds: {}", e))
}

fn parse_fl_block(fl_block: ElementRef) -> anyhow::Result<(u32, u32)> {
    let mut divs = fl_block.children().filter_map(ElementRef::wrap);
    let mut parse_div = |kind| {
        divs.next()
            .ok_or_else(|| anyhow!("{} div not found", kind))?
            .text()
            .collect::<String>()
            .parse()
            .map_err(|e| anyhow!("Unexpected {} count: {}", kind, e))
    };
    let fast = parse_div("Fast")?;
    let late = parse_div("Late")?;
    Ok((fast, late))
}

#[allow(clippy::many_single_char_names)]
fn parse_judge_count_row(row: ElementRef) -> anyhow::Result<JudgeCount> {
    let parse = |e: ElementRef| match e.text().collect::<String>().as_ref() {
        "" | "　" => Ok(None),
        s => s
            .parse::<u32>()
            .map(Some)
            .map_err(|x| anyhow!("Failed to parse: {:?} ({})", s, x)),
    };
    let values = row
        .select(selector!("td"))
        .map(parse)
        .collect::<Result<Vec<_>, _>>()?;
    let res = match &values[..] {
        [None, None, None, None, None] => JudgeCount::Nothing,
        [a, Some(b), Some(c), Some(d), Some(e)] => {
            let counts = JudgeCountWithoutCP::builder()
                .perfect(*b)
                .great(*c)
                .good(*d)
                .miss(*e)
                .build();
            match a {
                &Some(a) => JudgeCount::JudgeCountWithCP(
                    JudgeCountWithCP::builder()
                        .critical_perfect(a)
                        .others(counts)
                        .build(),
                ),
                None => JudgeCount::JudgeCountWithoutCP(counts),
            }
        }
        e => return Err(anyhow!("Unexpected row: {:?}", e)),
    };
    Ok(res)
}

fn parse_rating_deatil_block(rating_detail_block: ElementRef) -> anyhow::Result<RatingResult> {
    let (rating_block, rating, rating_color) = parse_rating_block_and_color(rating_detail_block)?;

    let mut next_elements = rating_block
        .parent()
        .ok_or_else(|| anyhow!("No parent elmenets for rating detail block"))?
        .next_siblings()
        .filter_map(ElementRef::wrap);

    let delta_sign = parse_delta_sign(
        next_elements
            .next()
            .ok_or_else(|| anyhow!("No element was found next to rating block"))?,
    )?;

    let next_div = next_elements
        .next()
        .ok_or_else(|| anyhow!("No element was found next to rating delta sign"))?;

    let delta = parse_rating_delta(
        next_div
            .select(selector!("span"))
            .next()
            .ok_or_else(|| anyhow!("No rating delta span was found"))?,
    )?;
    // Abolished as of DELUXE Splash PLUS, started on 2021/3/18
    // let grade_icon = parse_rating_grade(
    //     next_div
    //         .select(selector!("img"))
    //         .next()
    //         .ok_or_else(|| anyhow!("No rating grade icon was found"))?,
    // )?;

    let rating_result = RatingResult::builder()
        .rating(rating)
        .border_color(rating_color)
        .delta_sign(delta_sign)
        .delta(delta)
        // Abolished as of DELUXE Splash PLUS, started on 2021/3/18
        // .grade_icon(grade_icon)
        .build();
    Ok(rating_result)
}

fn parse_rating_block_and_color(
    parent_block: ElementRef,
) -> anyhow::Result<(ElementRef, RatingValue, RatingBorderColor)> {
    let rating_block = parent_block
        .select(selector!("div.rating_block"))
        .next()
        .ok_or_else(|| anyhow!("Rating block not found"))?;

    let rating = rating_block
        .text()
        .collect::<String>()
        .parse::<u16>()?
        .into();

    let rating_color = parse_rating_color(
        rating_block
            .prev_siblings()
            .find_map(ElementRef::wrap)
            .ok_or_else(|| anyhow!("No rating image was found before rating value"))?,
    )?;

    Ok((rating_block, rating, rating_color))
}

fn parse_rating_color(img: ElementRef) -> anyhow::Result<RatingBorderColor> {
    use RatingBorderColor::*;
    let res = match img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("Rating border image does not have src"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png" => Rainbow,
        // Ver 1.17
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.17" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.17" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.17" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.17" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.17" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.17" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.17" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.17" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.17" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.17" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.17" => Rainbow,
        // Ver 1.20
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.20" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.20" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.20" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.20" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.20" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.20" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.20" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.20" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.20" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.20" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.20" => Rainbow,
        // Ver 1.25
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.25" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.25" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.25" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.25" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.25" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.25" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.25" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.25" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.25" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.25" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.25" => Rainbow,
        // Ver 1.30
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.30" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.30" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.30" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.30" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.30" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.30" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.30" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.30" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.30" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.30" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.30" => Rainbow,
        // Ver 1.35
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.35" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.35" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.35" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.35" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.35" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.35" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.35" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.35" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.35" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.35" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.35" => Rainbow,
        // Ver 1.40
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.40" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.40" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.40" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.40" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.40" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.40" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.40" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.40" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.40" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.40" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.40" => Rainbow,
        // Ver 1.45
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.45" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.45" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.45" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.45" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.45" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.45" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.45" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.45" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.45" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.45" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.45" => Rainbow,
        // Ver 1.50
        "https://maimaidx.jp/maimai-mobile/img/rating_base_normal.png?ver=1.50" => Normal,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_blue.png?ver=1.50" => Blue,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_green.png?ver=1.50" => Green,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_orange.png?ver=1.50" => Orange,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_red.png?ver=1.50" => Red,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_purple.png?ver=1.50" => Purple,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_bronze.png?ver=1.50" => Bronze,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_silver.png?ver=1.50" => Silver,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_gold.png?ver=1.50" => Gold,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_platinum.png?ver=1.50" => Platinum,
        "https://maimaidx.jp/maimai-mobile/img/rating_base_rainbow.png?ver=1.50" => Rainbow,
        // International
        // Ver 1.35
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_normal.png?ver=1.35" => Normal,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_blue.png?ver=1.35" => Blue,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_green.png?ver=1.35" => Green,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_orange.png?ver=1.35" => Orange,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_red.png?ver=1.35" => Red,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_purple.png?ver=1.35" => Purple,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_bronze.png?ver=1.35" => Bronze,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_silver.png?ver=1.35" => Silver,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_gold.png?ver=1.35" => Gold,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_platinum.png?ver=1.35" => Platinum,
        "https://maimaidx-eng.com/maimai-mobile/img/rating_base_rainbow.png?ver=1.35" => Rainbow,
        src => return Err(anyhow!("Unexpected border color: {}", src)),
    };
    Ok(res)
}

fn parse_delta_sign(img: ElementRef) -> anyhow::Result<RatingDeltaSign> {
    use RatingDeltaSign::*;
    let res = match img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("Rating border image does not have src"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/playlog/rating_up.png" => Up,
        "https://maimaidx.jp/maimai-mobile/img/playlog/rating_keep.png" => Keep,
        "https://maimaidx.jp/maimai-mobile/img/playlog/rating_down.png" => Down,
        // International
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/rating_up.png" => Up,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/rating_keep.png" => Keep,
        "https://maimaidx-eng.com/maimai-mobile/img/playlog/rating_down.png" => Down,
        src => return Err(anyhow!("Unexpected border color: {}", src)),
    };
    Ok(res)
}

fn parse_rating_delta(span: ElementRef) -> anyhow::Result<i16> {
    regex!(r"^\(([+-][0-9]+)\)$")
        .captures(&span.text().collect::<String>())
        .ok_or_else(|| anyhow!("Rating delta text does not match the pattern"))?
        .get(1)
        .expect("Group 1 always exists")
        .as_str()
        .parse()
        .map_err(|e| anyhow!("Given integer was out of bounds: {}", e))
}

// fn parse_rating_grade(img: ElementRef) -> anyhow::Result<GradeIcon> {
//     Ok(img
//         .value()
//         .attr("src")
//         .ok_or_else(|| anyhow!("Grade icon does not have src"))?
//         .parse::<Url>()
//         .map_err(|e| anyhow!("Grade icon src was not a url: {}", e))?
//         .into())
// }

fn parse_max_combo_sync_div(div: ElementRef) -> anyhow::Result<Option<ValueWithMax<u32>>> {
    let inner_div = div
        .select(selector!("div"))
        .next()
        .ok_or_else(|| anyhow!("No inner div was found in max combo"))?;
    match inner_div.text().collect::<String>().as_str() {
        "―" => Ok(None),
        s => parse_value_with_max(s).map(Some),
    }
}

fn parse_matching_div(matching_div: ElementRef) -> anyhow::Result<OtherPlayersList> {
    use ScoreDifficulty::*;
    matching_div
        .select(selector!(":scope > span"))
        .filter_map(|e| {
            let difficulty = match e.value().classes().find_map(|c| {
                let d = match c {
                    "playlog_basic_container" => Basic,
                    "playlog_advanced_container" => Advanced,
                    "playlog_expert_container" => Expert,
                    "playlog_master_container" => Master,
                    "playlog_remaster_container" => ReMaster,
                    "playlog_utage_container" => Utage,
                    "gray_block" => return Some(None),
                    _ => return None,
                };
                Some(Some(d))
            }) {
                Some(Some(d)) => d,
                Some(None) => return None,
                None => return Some(Err(anyhow!("No valid class was found for matching div"))),
            };
            let user_name = match e.select(selector!("div")).next() {
                Some(e) => e.text().collect::<String>().into(),
                None => return Some(Err(anyhow!("User name div was not found for matching div"))),
            };
            Some(Ok(OtherPlayer::builder()
                .difficulty(difficulty)
                .user_name(user_name)
                .build()))
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .and_then(|e| {
            e.try_into()
                .map_err(|e| anyhow!("Other players of unexpected length: {:?}", e))
        })
}

pub fn parse_playlog_vs_result(img: ElementRef) -> anyhow::Result<BattleWinOrLose> {
    use BattleWinOrLose::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/playlog/win.png") => Ok(Win),
        Some("https://maimaidx.jp/maimai-mobile/img/playlog/lose.png") => Ok(Lose),
        // International
        Some("https://maimaidx-eng.com/maimai-mobile/img/playlog/win.png") => Ok(Win),
        Some("https://maimaidx-eng.com/maimai-mobile/img/playlog/lose.png") => Ok(Lose),
        url => Err(anyhow!(
            "Unexpected playlog vs result image image: {:?}",
            url
        )),
    }
}

pub fn parse_vs_user(div: ElementRef) -> anyhow::Result<(BattleKind, BattleOpponent)> {
    let outer_span = div
        .select(selector!(":scope > span"))
        .next()
        .ok_or_else(|| anyhow!("Outer span was not found"))?;

    let (battle_kind, user_name, achievement_value) = parse_vs_user_left_span(
        outer_span
            .select(selector!(":scope > span.p_t_5.d_ib.f_l"))
            .next()
            .ok_or_else(|| anyhow!("Left span was not found"))?,
    )?;

    let (rating, rating_color /*, grade_icon*/) = parse_vs_user_right_div(
        outer_span
            .select(selector!(":scope > div.p_3.f_l"))
            .next()
            .ok_or_else(|| anyhow!("Right div was not found"))?,
    )?;

    Ok((
        battle_kind,
        BattleOpponent::builder()
            .user_name(user_name)
            .achievement_value(achievement_value)
            .rating(rating)
            .border_color(rating_color)
            // .grade_icon(grade_icon)
            .build(),
    ))
}

pub fn parse_vs_user_left_span(
    span: ElementRef,
) -> anyhow::Result<(BattleKind, UserName, AchievementValue)> {
    let battle_kind = parse_battle_kind_img(
        span.select(selector!(":scope > img"))
            .next()
            .ok_or_else(|| anyhow!("Battle kind img not found"))?,
    )?;

    let wrapping_div = span
        .select(selector!(":scope > div"))
        .next()
        .ok_or_else(|| anyhow!("Opponent div was not found in battle left span"))?;

    let user_name = wrapping_div
        .children()
        .find_map(|e| match e.value() {
            scraper::Node::Text(text) => Some(text.deref().to_owned()),
            _ => None,
        })
        .ok_or_else(|| anyhow!("Opponent user name not found"))?
        .into();

    let achievement_value = parse_achievement_txt(
        wrapping_div
            .select(selector!(":scope > span"))
            .next()
            .ok_or_else(|| anyhow!("Opponent achievement value span was not found"))?,
    )?;

    Ok((battle_kind, user_name, achievement_value))
}

pub fn parse_battle_kind_img(img: ElementRef) -> anyhow::Result<BattleKind> {
    use BattleKind::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/playlog/vs.png") => Ok(VsFriend),
        Some("https://maimaidx.jp/maimai-mobile/img/playlog/boss.png") => Ok(Promotion),
        // International
        Some("https://maimaidx-eng.com/maimai-mobile/img/playlog/vs.png") => Ok(VsFriend),
        Some("https://maimaidx-eng.com/maimai-mobile/img/playlog/boss.png") => Ok(Promotion),
        src => Err(anyhow!("Unknown src for battle kind img: {:?}", src)),
    }
}

pub fn parse_vs_user_right_div(
    div: ElementRef,
) -> anyhow::Result<(RatingValue, RatingBorderColor /*, GradeIcon*/)> {
    let (_, rating, rating_color) = parse_rating_block_and_color(div)?;

    // let grade_icon = parse_rating_grade(
    //     div.select(selector!(":scope > img"))
    //         .next()
    //         .ok_or_else(|| anyhow!("No rating grade icon was found"))?,
    // )?;

    Ok((rating, rating_color /*, grade_icon*/))
}

fn parse_life_block(div: ElementRef) -> anyhow::Result<LifeResult> {
    use LifeResult::*;
    let life_value = parse_value_with_max(div.text().collect::<String>().as_str())?;
    match &div
        .prev_siblings()
        .filter_map(ElementRef::wrap)
        .map(|e| e.value().attr("src"))
        .collect_vec()[..]
    {
        [Some("https://maimaidx.jp/maimai-mobile/img/icon_life.png"), Some("https://maimaidx.jp/maimai-mobile/img/icon_perfectchallenge.png")] => {
            Ok(PerfectChallengeResult(life_value))
        }
        [Some("https://maimaidx.jp/maimai-mobile/img/course/icon_course_life.png"), Some("https://maimaidx.jp/maimai-mobile/img/course/icon_course.png")] => {
            Ok(CourseResult(life_value))
        }
        // International
        [Some("https://maimaidx-eng.com/maimai-mobile/img/icon_life.png"), Some("https://maimaidx-eng.com/maimai-mobile/img/icon_perfectchallenge.png")] => {
            Ok(PerfectChallengeResult(life_value))
        }
        [Some("https://maimaidx-eng.com/maimai-mobile/img/course/icon_course_life.png"), Some("https://maimaidx-eng.com/maimai-mobile/img/course/icon_course.png")] => {
            Ok(CourseResult(life_value))
        }
        elements => Err(anyhow!(
            "Unknown previous elements for life block: {:?}",
            elements
        )),
    }
}

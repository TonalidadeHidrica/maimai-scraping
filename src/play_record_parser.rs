use crate::schema::*;
use anyhow::anyhow;
use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::{
    convert::{TryFrom, TryInto},
    str::FromStr,
};

macro_rules! selector {
    ($e: expr) => {{
        static SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse($e).unwrap());
        &*SELECTOR
    }};
}

macro_rules! regex {
    ($e: expr) => {{
        static PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new($e).unwrap());
        &*PATTERN
    }};
}

pub fn parse(html: Html) -> anyhow::Result<PlayRecord> {
    let playlog_top_container = html
        .select(selector!(".playlog_top_container"))
        .next()
        .ok_or_else(|| anyhow!("Playlog top container was not found."))?;
    parse_playlog_top_conatiner(playlog_top_container)?;

    let playlog_main_container = playlog_top_container
        .next_siblings()
        .filter_map(ElementRef::wrap)
        .next()
        .ok_or_else(|| anyhow!("Next sibling was not found."))?;
    parse_playlog_main_container(playlog_main_container)?;

    unimplemented!()
}

fn parse_playlog_top_conatiner(
    div: ElementRef,
) -> anyhow::Result<(ScoreDifficulty, TrackIndex, NaiveDateTime)> {
    let difficulty = parse_playlog_diff(
        div.select(selector!("img.playlog_diff"))
            .next()
            .ok_or_else(|| anyhow!("Difficulty image was not found."))?,
    )?;

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
    Ok((difficulty, track_index, play_date))
}

fn parse_playlog_diff(img: ElementRef) -> anyhow::Result<ScoreDifficulty> {
    use ScoreDifficulty::*;
    match img.value().attr("src") {
        Some("https://maimaidx.jp/maimai-mobile/img/diff_basic.png") => Ok(Basic),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_advanced.png") => Ok(Advanced),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_expert.png") => Ok(Expert),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_master.png") => Ok(Master),
        Some("https://maimaidx.jp/maimai-mobile/img/diff_remaster.png") => Ok(ReMaster),
        url => Err(anyhow!("Unexpected difficulty image: {:?}", url)),
    }
}

fn parse_track_index(span: ElementRef) -> anyhow::Result<TrackIndex> {
    Ok(regex!(r"TRACK 0([1-4])")
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

fn parse_play_date(span: ElementRef) -> anyhow::Result<NaiveDateTime> {
    Ok(NaiveDateTime::parse_from_str(
        &span.text().collect::<String>(),
        "%Y/%m/%d %H:%M",
    )?)
}

fn parse_playlog_main_container(playlog_main_container: ElementRef) -> anyhow::Result<()> {
    let basic_block = playlog_main_container
        .select(selector!(".basic_block"))
        .next()
        .ok_or_else(|| anyhow!("No basic_block was found"))?;
    let song_title = basic_block.text().collect::<String>();

    let cleared = match basic_block
        .select(selector!("img"))
        .next()
        .map(|e| e.value().attr("src"))
    {
        Some(Some("https://maimaidx.jp/maimai-mobile/img/playlog/clear.png")) => true,
        Some(src) => Err(anyhow!("Unexpected image source for cleared: {:?}", src))?,
        _ => false,
    };

    let music_img_src = playlog_main_container
        .select(selector!("img.music_img"))
        .next()
        .ok_or_else(|| anyhow!("music_img was not found"))?
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("Music img doesn't have src"))?;

    let generation = match playlog_main_container
        .select(selector!("img.playlog_music_kind_icon"))
        .next()
        .ok_or_else(|| anyhow!("Music generation icon not found"))?
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("Image src was not found"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/music_dx.png" => ScoreGeneration::Deluxe,
        "https://maimaidx.jp/maimai-mobile/img/music_standard.png" => ScoreGeneration::Standard,
        src => Err(anyhow!(
            "Unexpected image source for music generation: {}",
            src
        ))?,
    };

    dbg!(&song_title);
    dbg!(&cleared);
    dbg!(&music_img_src);
    dbg!(&generation);

    let playlog_result_block = playlog_main_container
        .select(selector!(".playlog_result_block"))
        .next()
        .ok_or_else(|| anyhow!("playlog result block was not found"))?;

    parse_playlog_result_block(playlog_result_block)?;

    Ok(())
}

fn parse_playlog_result_block(playlog_result_block: ElementRef) -> anyhow::Result<()> {
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

    parse_playlog_result_innerblock(
        playlog_result_block
            .select(selector!("div.playlog_result_innerblock"))
            .next()
            .ok_or_else(|| anyhow!("playlog result innerblock was not found"))?,
    )?;

    let perfect_challenge_result = playlog_result_block
        .select(selector!("div.playlog_life_block"))
        .next()
        .map(parse_playlog_life_block)
        .transpose()?;

    dbg!(&achievement_result);
    dbg!(&perfect_challenge_result);

    Ok(())
}

fn parse_achievement_txt(achievement_txt: ElementRef) -> anyhow::Result<AchievementValue> {
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
        src => Err(anyhow!("Unknown url: {}", src))?,
    };
    Ok(res)
}

fn parse_playlog_result_innerblock(playlog_result_innerblock: ElementRef) -> anyhow::Result<()> {
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

    dbg!(&deluxscore_result);
    dbg!(&full_combo_kind);
    dbg!(&full_sync_kind);
    dbg!(&matching_rank);

    Ok(())
}

fn parse_deluxscore(deluxe_score_div: ElementRef) -> anyhow::Result<ValueWithMax<u32>> {
    let text = deluxe_score_div.text().collect::<String>();
    let captures = regex!(r"^([0-9,]+) / ([0-9,]+)$")
        .captures(&text)
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
        src => Err(anyhow!("Unknown src for dxstar: {}", src))?,
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
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc_dummy.png?ver=1.15" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fc.png?ver=1.15" => FullCombo,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fcplus.png?ver=1.15" => FullComboPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/ap.png?ver=1.15" => AllPerfect,
        "https://maimaidx.jp/maimai-mobile/img/playlog/applus.png?ver=1.15" => AllPerfectPlus,
        src => Err(anyhow!("Unknown src for full combo img: {}", src))?,
    };
    Ok(res)
}

fn parse_full_sync_img(full_sync_img: ElementRef) -> anyhow::Result<FullSyncKind> {
    use FullSyncKind::*;
    let res = match full_sync_img
        .value()
        .attr("src")
        .ok_or_else(|| anyhow!("No src was found for full combo image"))?
    {
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs_dummy.png?ver=1.15" => Nothing,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fs.png?ver=1.15" => FullSync,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsplus.png?ver=1.15" => FullSyncPlus,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsd.png?ver=1.15" => FullSyncDx,
        "https://maimaidx.jp/maimai-mobile/img/playlog/fsdplus.png?ver=1.15" => FullSyncDxPlus,
        src => Err(anyhow!("Unknown src for full sync img: {}", src))?,
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
        src => Err(anyhow!("Unknown src for matching rank img: {}", src))?,
    };
    Ok(res.try_into().expect("Value is always in the bounds"))
}

fn parse_playlog_life_block(
    playlog_life_block: ElementRef,
) -> anyhow::Result<PerfectChallengeResult> {
    let text = playlog_life_block.text().collect::<String>();
    let captures = regex!(r"^([0-9,]+)/([0-9,]+)$")
        .captures(&text)
        .ok_or_else(|| anyhow!("Invalid deluxscore format: {:?}", text))?;
    let a = parse_integer_with_camma(captures.get(1).expect("Group 1 exists").as_str())?;
    let b = parse_integer_with_camma(captures.get(2).expect("Group 2 exists").as_str())?;
    let res =
        ValueWithMax::new(a, b).map_err(|res| anyhow!("Value is larger than full: {:?}", res))?;
    Ok(res.into())
}

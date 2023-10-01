use std::fmt::Display;

use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    estimate_rating::{ScoreConstantsStore, ScoreKey},
    rating::{ScoreConstant, ScoreLevel},
    schema::{
        latest::{LifeResult, PlayRecord, RatingBorderColor, ScoreMetadata},
        ver_20210316_2338::RatingValue,
    },
};

pub fn get_song_lvs<'a>(
    record: &'_ PlayRecord,
    levels: &'a ScoreConstantsStore<'_, '_>,
) -> &'a [ScoreConstant] {
    if let Ok(Some((_, candidates))) = levels.get(ScoreKey::from(record)) {
        candidates
    } else {
        &[]
    }
}

pub fn make_message<'a>(
    record: &'a PlayRecord,
    song_lvs: &'a [ScoreConstant],
) -> impl Display + Send + 'a {
    use maimai_scraping::maimai::schema::latest::{AchievementRank::*, FullComboKind::*};
    let score_kind = describe_score_kind(record.score_metadata());
    let lv = lazy_format!(match (song_lvs[..]) {
        [] => "?",
        [lv] => "{lv}",
        [lv, ..] => ("{}", ScoreLevel::from(lv)),
    });
    let rank = match record.achievement_result().rank() {
        D => "D",
        C => "C",
        BBB => "BBB",
        BB => "BB",
        B => "B",
        A => "A",
        AA => "AA",
        AAA => "AAA",
        S => "S",
        SPlus => "S+",
        SS => "SS",
        SSPlus => "SS+",
        SSS => "SSS",
        SSSPlus => "SSS+",
    };
    let ach_new = lazy_format!(
        if record.achievement_result().new_record() => " :new:"
        else => ""
    );
    let fc = match record.combo_result().full_combo_kind() {
        Nothing => "",
        FullCombo => "FC",
        FullComboPlus => "FC+",
        AllPerfect => "AP",
        AllPerfectPlus => "AP+",
    };
    let time = (record.played_at().idx().timestamp_jst()).unwrap_or(record.played_at().time());
    let main_line = lazy_format!(
        "{time}　{title} ({score_kind} Lv.{lv})　{rank}({ach}{ach_new})　{fc}\n",
        title = record.song_metadata().name(),
        ach = record.achievement_result().value(),
    );
    let rating_line = (record.rating_result().delta() > 0).then(|| {
        let new = record.rating_result().rating();
        let delta = record.rating_result().delta();
        let old = RatingValue::from((new.get() as i16 - delta) as u16);
        use RatingBorderColor::*;
        let old_color = match old.get() {
            15000.. => Rainbow,
            14500.. => Platinum,
            14000.. => Gold,
            13000.. => Silver,
            12000.. => Bronze,
            10000.. => Purple,
            7000.. => Red,
            4000.. => Orange,
            2000.. => Green,
            1000.. => Blue,
            ..=999 => Normal,
        };
        let new_color = record.rating_result().border_color();
        let color_change = lazy_format!(
            if old_color != new_color => "　Color changed to {new_color:?}!"
            else => ""
        );
        lazy_format!("Rating: {old} => {new} ({delta:+}){color_change}\n")
    });
    let rating_line = lazy_format!(if let Some(x) = rating_line => "{x}" else => "");
    // let rating_line = rating_line.as_deref().unwrap_or("");
    let life_line = match record.life_result() {
        LifeResult::Nothing => None,
        LifeResult::PerfectChallengeResult(res) => Some(("Perfect challenge", res)),
        LifeResult::CourseResult(res) => Some(("Course", res)),
    }
    .map(|(name, res)| lazy_format!("{name} life: {}/{}\n", res.value(), res.max()));
    let life_line = lazy_format!(if let Some(x) = life_line => "{x}" else => "");
    lazy_format!("{main_line}{rating_line}{life_line}")
}

pub fn describe_score_kind<'a>(metadata: ScoreMetadata) -> impl Display + 'a {
    use maimai_scraping::maimai::schema::latest::{ScoreDifficulty::*, ScoreGeneration::*};
    let gen = match metadata.generation() {
        Standard => "STD",
        Deluxe => "DX",
    };
    let dif = match metadata.difficulty() {
        Basic => "Bas",
        Advanced => "Adv",
        Expert => "Exp",
        Master => "Mas",
        ReMaster => "ReMas",
        Utage => "Utg",
    };
    lazy_format!("{gen} {dif}")
}

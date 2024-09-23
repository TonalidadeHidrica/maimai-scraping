use std::fmt::Display;

use either::Either;
use lazy_format::lazy_format;
use maimai_scraping::maimai::{
    associated_user_data,
    rating::{rank_coef, single_song_rating_precise},
    schema::{
        latest::{
            JudgeCount, JudgeCountWithoutCP, JudgeResult, LifeResult, PlayRecord,
            RatingBorderColor, ScoreDifficulty, ScoreMetadata,
        },
        ver_20210316_2338::RatingValue,
    },
    song_list::database::ScoreForVersionRef,
    version::MaimaiVersion,
};

pub fn make_message<'a>(
    record: &'a PlayRecord,
    associated: Option<&associated_user_data::PlayRecord>,
) -> impl Display + Send + 'a {
    use maimai_scraping::maimai::schema::latest::{AchievementRank::*, FullComboKind::*};
    let time = (record.played_at().idx().timestamp_jst()).unwrap_or(record.played_at().time());
    let score_kind = describe_score_kind(record.score_metadata());
    let level = associated
        .and_then(|x| x.score().ok())
        .and_then(|x| match x {
            ScoreForVersionRef::Ordinary(score) => {
                let level = score.level()?;
                Some(match level.get_if_unique() {
                    Some(x) => Either::Left(x),
                    None => Either::Right(level.into_level(MaimaiVersion::latest())),
                })
            }
            ScoreForVersionRef::Utage(score) => Some(Either::Right(score.score().level())),
        });
    let lv = lazy_format!(match (level) {
        None => "?",
        Some(x) => "{x}",
    });
    let lv_question = lazy_format!(
        if record.utage_metadata().is_some() => "?"
        else => ""
    );
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
    let barely_fc = make_barely_fc(record.judge_result(), record.combo_result().combo().max());
    let barely_fc = if let Some(x) = barely_fc {
        format!(" ({x})")
    } else {
        String::new()
    };
    let rating_value = {
        let value = match level {
            Some(Either::Left(lv)) => {
                let a = record.achievement_result().value();
                let result = single_song_rating_precise(lv, a, rank_coef(a)) / 1_000_000;
                Some(lazy_format!("{}.{:02}", result / 100, result % 100))
            }
            _ => None,
        };
        lazy_format!(match (value) {
            Some(x) => "{x}",
            None => "",
        })
    };
    let main_line = lazy_format!(
        "{time}　{title} ({score_kind} Lv.{lv}{lv_question})　{rank}({ach}{ach_new}) {rating_value}　{fc}{barely_fc}\n",
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
    let gen = metadata.generation().abbrev();
    let dif = metadata.difficulty().abbrev();
    lazy_format!(match (metadata.difficulty()) {
        ScoreDifficulty::Utage => "Utg",
        _ => "{gen} {dif}",
    })
}

pub fn make_barely_fc(judge: JudgeResult, max_combo: u32) -> Option<String> {
    use JudgeCount as JC;
    let border = (max_combo / 50).max(10);

    fn make(
        judge: JudgeResult,
        get: impl Fn(JudgeCountWithoutCP) -> u32,
        judge_str: &'static str,
        include_nonbreak: bool,
        include_break: bool,
    ) -> (u32, String) {
        let mut sum = 0;
        let mut res = String::new();
        for (kind_str, x) in [
            ("T", judge.tap()),
            ("H", judge.hold()),
            ("S", judge.slide()),
            ("t", judge.touch()),
        ]
        .into_iter()
        .filter(|_| include_nonbreak)
        .filter_map(|(c, x)| match x {
            JC::Nothing => None,
            JC::JudgeCountWithCP(x) => Some((c, x.others())),
            JC::JudgeCountWithoutCP(x) => Some((c, x)),
        })
        .chain(include_break.then_some(("B", judge.break_().others())))
        {
            let count = get(x);
            if count > 0 {
                sum += count;
                res += kind_str;
                res += judge_str;
                if count > 1 {
                    res += &count.to_string();
                }
                res += " ";
            }
        }
        (sum, res)
    }

    let (miss, miss_str) = make(judge, |x| x.miss(), "m", true, true);
    let (good, good_str) = make(judge, |x| x.good(), "g", true, true);
    let (great, great_str) = make(judge, |x| x.great(), "G", true, true);
    let (perfect_break, perfect_break_str) = make(judge, |x| x.perfect(), "P", false, true);
    let (perfect_nonbreak, perfect_nonbreak_str) = make(judge, |x| x.perfect(), "P", true, false);

    let good_or_less = miss + good;
    let great_or_less = good_or_less + great;
    // perfect or less for break and great or less for others
    let polfbagolfo = great_or_less + perfect_break;
    let prefect_or_less = polfbagolfo + perfect_nonbreak;

    let critical_perfect_available = [judge.tap(), judge.hold(), judge.slide(), judge.touch()]
        .into_iter()
        .filter_map(|x| match x {
            JC::Nothing => None,
            JC::JudgeCountWithCP(_) => Some(true),
            JC::JudgeCountWithoutCP(_) => Some(false),
        })
        .all(|x| x);
    if critical_perfect_available && (1..=border).contains(&prefect_or_less) {
        Some(format!(
            "{perfect_nonbreak_str}{perfect_break_str}{great_str}{good_str}{miss_str}for ACP"
        ))
    } else if (1..=border).contains(&polfbagolfo) {
        Some(format!(
            "{perfect_break_str}{great_str}{good_str}{miss_str}for AP+"
        ))
    } else if (1..=border).contains(&great_or_less) {
        Some(format!("{great_str}{good_str}{miss_str}for AP"))
    } else if (1..=border).contains(&good_or_less) {
        Some(format!("{good_str}{miss_str}for FC+"))
    } else if (1..=border).contains(&miss) {
        Some(format!("{miss_str}for FC"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use maimai_scraping::maimai::schema::latest::{
        JudgeCount, JudgeCountWithCP, JudgeCountWithoutCP, JudgeResult,
    };

    use super::make_barely_fc;

    macro_rules! wto {
        ($b: expr, $c: expr, $d: expr, $e: expr) => {
            JudgeCountWithoutCP::builder()
                .perfect($b)
                .great($c)
                .good($d)
                .miss($e)
                .build()
        };
    }
    macro_rules! wcp {
        ($a: expr, $b: expr, $c: expr, $d: expr, $e: expr) => {
            JudgeCountWithCP::builder()
                .critical_perfect($a)
                .others(wto!($b, $c, $d, $e))
                .build()
        };
    }
    macro_rules! cnt {
        ($a: expr, $b: expr, $c: expr, $d: expr, $e: expr) => {
            JudgeCount::JudgeCountWithCP(wcp!($a, $b, $c, $d, $e))
        };
        (=, $b: expr, $c: expr, $d: expr, $e: expr) => {
            JudgeCount::JudgeCountWithoutCP(wto!($b, $c, $d, $e))
        };
        () => {
            JudgeCount::Nothing
        };
    }

    fn assert_barely_fc(
        tap: JudgeCount,
        hold: JudgeCount,
        slide: JudgeCount,
        touch: JudgeCount,
        break_: JudgeCountWithCP,
        expected: impl Into<Option<&'static str>>,
    ) {
        let judge = JudgeResult::builder()
            .fast(0)
            .late(0)
            .tap(tap)
            .hold(hold)
            .slide(slide)
            .touch(touch)
            .break_(break_)
            .build();
        let result = make_barely_fc(judge, 1); // border = 10
        assert_eq!(result.as_deref(), expected.into());
    }

    #[test]
    fn test_make_barely_fc() {
        assert_barely_fc(
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            wcp!(100, 0, 0, 0, 0),
            None,
        );
        assert_barely_fc(
            cnt!(100, 0, 1, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            wcp!(100, 0, 0, 0, 0),
            "TG for ACP",
        );
        assert_barely_fc(
            cnt!(100, 0, 2, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            wcp!(100, 0, 0, 0, 0),
            "TG2 for ACP",
        );
        assert_barely_fc(
            cnt!(100, 100, 2, 0, 0),
            cnt!(100, 100, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            wcp!(100, 100, 0, 0, 0),
            "TG2 for AP",
        );
        assert_barely_fc(
            cnt!(100, 100, 2, 0, 0),
            cnt!(100, 100, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            wcp!(100, 5, 0, 0, 0),
            "BP5 TG2 for AP+",
        );
        assert_barely_fc(
            cnt!(100, 100, 0, 3, 1),
            cnt!(100, 100, 0, 0, 0),
            cnt!(100, 0, 0, 0, 0),
            cnt!(100, 0, 0, 1, 1),
            wcp!(100, 7, 0, 0, 0),
            "Tg3 tg Tm tm for AP",
        );
        assert_barely_fc(
            cnt!(330, 302, 24, 3, 1),
            cnt!(2, 4, 0, 0, 0),
            cnt!(68, 0, 1, 0, 0),
            cnt!(),
            wcp!(8, 5, 0, 0, 0),
            "Tg3 Tm for FC+",
        );
        assert_barely_fc(
            cnt!(124, 85, 0, 0, 0),
            cnt!(21, 9, 2, 0, 0),
            cnt!(25, 0, 0, 0, 0),
            cnt!(2, 0, 0, 0, 0),
            wcp!(9, 0, 0, 0, 0),
            "HG2 for AP+",
        );
    }
}

use chrono::NaiveDateTime;
use typed_html::{
    dom::TextNode,
    elements::{div, img, table, td},
    html, text,
    types::{Class, SpacedSet},
};

use super::schema::latest::*;

pub fn reconstruct(record: &PlayRecord) -> Box<div<String>> {
    html!(
        <div class="container3 t_l">
            <hr class="gray_line" />
            {construct_first_div(record)}
            <div class="clearfix"></div>
            {construct_vs_container(record.battle_participants())}
            <div class="m_10 t_r f_12">
                <div class="col2 f_l p_5">
                    {construct_score_detail_table_left(record)}
                </div>
                <div class="col2 f_r p_5">
                    {construct_score_detail_table_right(record.achievement_per_note_kind())}
                </div>
                <div class="clearfix"></div>
            </div>
            {construct_playlog_event_name(record.mission_result())}
            {construct_place_name(record.played_at().place())}
            {construct_record_link_block(record)}
            <div class="clearfix"></div>
            <hr class="gray_line"/>
        </div>
    )
}

fn construct_first_div(record: &PlayRecord) -> Box<div<String>> {
    html!(
        <div class="m_10">
            {construct_difficulty_img(record.score_metadata().difficulty())}
            <span class="f_r f_12 h_10">{text!("{}",
                NaiveDateTime::from(record.played_at().time()).format("%Y/%m/%d %H:%M")
            )}</span>
            <div class="m_5 l_h_10 break">
                <img src="https://ongeki-net.com/ongeki-mobile/img/icon_event.png" class="f_r h_21" />
                {text!("{}", record.song_metadata().name())}
            </div>
            <img src={record.song_metadata().cover_art().to_string()} class="m_5 f_l"/>
            <div class="f_r">
                {construct_playlog_score_block(record)}
            </div>
        </div>
    )
}

fn construct_difficulty_img(difficulty: ScoreDifficulty) -> Box<img<String>> {
    use ScoreDifficulty::*;
    let src = match difficulty {
        Basic => "https://ongeki-net.com/ongeki-mobile/img/diff_basic.png",
        Advanced => "https://ongeki-net.com/ongeki-mobile/img/diff_advanced.png",
        Expert => "https://ongeki-net.com/ongeki-mobile/img/diff_expert.png",
        Master => "https://ongeki-net.com/ongeki-mobile/img/diff_master.png",
        Lunatic => "https://ongeki-net.com/ongeki-mobile/img/diff_lunatic.png",
    };
    html!(<img src={src} />)
}

fn construct_playlog_score_block(record: &PlayRecord) -> Box<div<String>> {
    html!(
        <div class="playlog_score_block m_5 p_5 t_r white">
            <table>
                <tr>
                    {construct_score_td(
                        record.battle_result().score(),
                        |x| text!("{}", x),
                        "battle_score_block",
                        "BATTLE SCORE",
                    )}
                    <td rowspan="2" class="w_65">
                        {construct_battle_rank(record.battle_result().rank())}
                    </td>
                </tr>
                <tr>
                    {construct_score_td(
                        record.battle_result().over_damage(),
                        |x| text!("{}％", x),
                        "battle_score_block",
                        "OVER DAMAGE",
                    )}
                </tr>
                <tr>
                    {construct_score_td(
                        record.technical_result().score(),
                        |x| text!("{}", x),
                        "technical_score_block",
                        "TECHNICAL SCORE",
                    )}
                    <td class="w_65">
                        {construct_technical_rank(record.technical_result().rank())}
                    </td>
                </tr>
            </table>
            <div class="clearfix p_t_5 t_l f_0">
                {construct_win_or_lose(record.battle_result().win_or_lose())}
                {construct_full_bell(record.bell_result().full_bell_kind())}
                {construct_full_combo(record.combo_result().full_combo_kind())}
            </div>
        </div>
    )
}

fn construct_score_td<T: Copy>(
    value: ValueWithNewRecord<T>,
    show: impl FnOnce(T) -> Box<TextNode<String>>,
    class_base: &'static str,
    caption: &'static str,
) -> Box<td<String>> {
    let mut class_name = String::from(class_base);
    if value.new_record() {
        class_name += "_new";
    }
    html!(
        <td class=[&class_name as &str]>
            <div class="f_11">{text!(caption)}</div>
            <div class="f_20">{show(value.value())}</div>
        </td>
    )
}

fn construct_battle_rank(value: BattleRank) -> Box<img<String>> {
    use BattleRank::*;
    let src = match value {
        // => Bad,
        FairLose => "https://ongeki-net.com/ongeki-mobile/img/score_br_usually_another.png",
        FairCleared => "https://ongeki-net.com/ongeki-mobile/img/score_br_usually.png",
        Good => "https://ongeki-net.com/ongeki-mobile/img/score_br_good.png",
        Great => "https://ongeki-net.com/ongeki-mobile/img/score_br_great.png",
        Excellent => "https://ongeki-net.com/ongeki-mobile/img/score_br_excellent.png ",
        // => UltimatePlatinum,
        // => UltimateRainbow,
    };
    html!(<img src={src} class="f_r"/>)
}

fn construct_technical_rank(value: TechnicalRank) -> Box<img<String>> {
    use TechnicalRank::*;
    let src = match value {
        SSSPlus => "https://ongeki-net.com/ongeki-mobile/img/score_tr_sssplus.png",
        SSS => "https://ongeki-net.com/ongeki-mobile/img/score_tr_sss.png",
        SS => "https://ongeki-net.com/ongeki-mobile/img/score_tr_ss.png",
        S => "https://ongeki-net.com/ongeki-mobile/img/score_tr_s.png",
        AAA => "https://ongeki-net.com/ongeki-mobile/img/score_tr_aaa.png",
        AA => "https://ongeki-net.com/ongeki-mobile/img/score_tr_aa.png",
        A => "https://ongeki-net.com/ongeki-mobile/img/score_tr_a.png",
        BBB => "https://ongeki-net.com/ongeki-mobile/img/score_tr_bbb.png",
        BB => "https://ongeki-net.com/ongeki-mobile/img/score_tr_bb.png",
        B => "https://ongeki-net.com/ongeki-mobile/img/score_tr_b.png",
        C => "https://ongeki-net.com/ongeki-mobile/img/score_tr_c.png",
        D => "https://ongeki-net.com/ongeki-mobile/img/score_tr_d.png",
    };
    html!(<img src={src} class="f_r" />)
}

fn construct_win_or_lose(value: WinOrLose) -> Box<img<String>> {
    use WinOrLose::*;
    let src = match value {
        Win => "https://ongeki-net.com/ongeki-mobile/img/score_detail_win.png",
        Draw => "https://ongeki-net.com/ongeki-mobile/img/score_detail_draw.png",
        Lose => "https://ongeki-net.com/ongeki-mobile/img/score_detail_lose.png",
    };
    html!(<img src={src} />)
}

fn construct_full_bell(value: FullBellKind) -> Box<img<String>> {
    use FullBellKind::*;
    let src = match value {
        Nothing => "https://ongeki-net.com/ongeki-mobile/img/score_detail_fb_base.png",
        FullBell => "https://ongeki-net.com/ongeki-mobile/img/score_detail_fb.png",
    };
    html!(<img src={src} />)
}

fn construct_full_combo(value: FullComboKind) -> Box<img<String>> {
    use FullComboKind::*;
    let src = match value {
        Nothing => "https://ongeki-net.com/ongeki-mobile/img/score_detail_fc_base.png",
        FullCombo => "https://ongeki-net.com/ongeki-mobile/img/score_detail_fc.png",
        AllBreak => "https://ongeki-net.com/ongeki-mobile/img/score_detail_ab.png",
    };
    html!(<img src={src} />)
}

fn construct_vs_container(participants: &BattleParticipants) -> Box<div<String>> {
    let opponent = participants.opponent();
    html!(
        <div class="vs_container">
            <img src="https://ongeki-net.com/ongeki-mobile/img/playlog_deck.png" class="add_title_img" />
            <div class="vs_block m_10 p_3 f_13 break">
                {construct_card_icon(opponent.color())}
                {text!("{} Lv.{}", opponent.name(), opponent.level())}
                {construct_enemy_icon(opponent.color())}
            </div>
            <div class="t_c l_h_10">
                {participants.deck().iter().enumerate().map(|(i, e)| construct_card_block(e, i == 0))}
                <div class="clearfix"></div>
            </div>
        </div>
    )
}

fn construct_card_icon(color: BattleOpponentColor) -> Box<img<String>> {
    use BattleOpponentColor::*;
    let src = match color {
        Fire => "https://ongeki-net.com/ongeki-mobile/img/card_icon_fire.png",
        Leaf => "https://ongeki-net.com/ongeki-mobile/img/card_icon_leaf.png",
        Aqua => "https://ongeki-net.com/ongeki-mobile/img/card_icon_aqua.png",
    };
    html!(<img src={src} class="h_16 v_m"/>)
}

fn construct_enemy_icon(color: BattleOpponentColor) -> Box<img<String>> {
    use BattleOpponentColor::*;
    let src = match color {
        Fire => "https://ongeki-net.com/ongeki-mobile/img/enemy_icon_mini_fire.png",
        Leaf => "https://ongeki-net.com/ongeki-mobile/img/enemy_icon_mini_leaf.png",
        Aqua => "https://ongeki-net.com/ongeki-mobile/img/enemy_icon_mini_aqua.png",
    };
    html!(<img src={src} class=" v_m"/>)
}

fn construct_card_block(card: &DeckCard, is_first: bool) -> Box<div<String>> {
    dbg!(card);
    let mut classes: SpacedSet<Class> = "card_block f_l col3".try_into().unwrap();
    if is_first {
        classes.insert("f_0".try_into().unwrap());
    }
    html!(
        <div class={classes}>
            <div class="card_info_block f_11 gray">
                <span class="main_color">{text!("Lv.{}", card.level())}</span>
                "　攻撃力"<span class="sub_color">{text!("{}", card.power())}</span>
            </div>
            <img src={card.card_image().to_string()} class="w_127" />
        </div>
    )
}

fn construct_score_detail_table_left(record: &PlayRecord) -> Box<table<String>> {
    let judges = record.judge_result();
    let bells = record.bell_result();
    html!(
        <table class="score_detail_table f_r">
            <tr>
                <th class="f_0"><img src="https://ongeki-net.com/ongeki-mobile/img/score_max_combo.png" class="h_16"/></th>
                <td class="f_b">{text!("{}", record.combo_result().max_combo())}</td>
            </tr>
            <tr class="score_critical_break">
                <th class="f_0"><img src="https://ongeki-net.com/ongeki-mobile/img/score_critical_break.png" class="h_16"/></th>
                <td class="f_b">{text!("{}", judges.critical_break())}</td>
            </tr>
            <tr class="score_break">
                <th class="f_0"><img src="https://ongeki-net.com/ongeki-mobile/img/score_break.png" class="h_16"/></th>
                <td class="f_b">{text!("{}", judges.break_())}</td>
            </tr>
            <tr class="score_hit">
                <th class="f_0"><img src="https://ongeki-net.com/ongeki-mobile/img/score_hit.png" class="h_16"/></th>
                <td class="f_b">{text!("{}", judges.hit())}</td>
            </tr>
            <tr class="score_miss">
                <th class="f_0"><img src="https://ongeki-net.com/ongeki-mobile/img/score_miss.png" class="h_16"/></th>
                <td class="f_b">{text!("{}", judges.miss())}</td>
            </tr>
            <tr class="score_bell">
                <th>"BELL"</th>
                <td class="f_b">{text!("{}/{}", bells.count(), bells.max())}</td>
            </tr>
            <tr class="score_damage">
                <th>"DAMAGE"</th>
                <td>{text!("{}", record.damage_count())}</td>
            </tr>
        </table>
    )
}

fn construct_score_detail_table_right(record: &AchievementPerNoteKindResult) -> Box<table<String>> {
    let show = |x: Option<AchievementPerNoteKind>| {
        text!(match x {
            None => String::from("--%"),
            Some(x) => format!("{}%", x),
        })
    };
    html!(
        <table class="score_detail_table">
            <tr>
                <th>"TAP"</th>
                <td class="f_b">{show(record.tap())}</td>
            </tr>
            <tr>
                <th>"HOLD"</th>
                <td class="f_b">{show(record.hold())}</td>
            </tr>
            <tr>
                <th>"FLICK"</th>
                <td class="f_b">{show(record.flick())}</td>
            </tr>
            <tr>
                <th>"SIDE TAP"</th>
                <td class="f_b">{show(record.side_tap())}</td>
            </tr>
            <tr>
                <th>"SIDE HOLD"</th>
                <td class="f_b">{show(record.side_hold())}</td>
            </tr>
        </table>
    )
}

fn construct_playlog_event_name(mission: &MissionResult) -> Box<div<String>> {
    html!(
        <div class="playlog_event_name m_10 f_13 l_h_10 t_l break">
            <img src="https://ongeki-net.com/ongeki-mobile/img/icon_event.png" class="f_l h_17 m_10" />
            <div class="p_10">
                {text!("{}", mission.name())}
                <span class="main_color f_b">{text!(" +{} ", mission.score())}</span>
                "獲得！"
            </div>
        </div>
    )
}

fn construct_place_name(record: &PlayPlace) -> Box<div<String>> {
    html!(
        <div id="placeName" class="t_l m_10 f_13 l_h_10 break">
            <img src="https://ongeki-net.com/ongeki-mobile/img/on.png" id="placeNameCtrl" class="f_r" />
            <span class="d_b p_10">{text!("{}", record)}</span>
            <div class="clearfix"></div>
        </div>
    )
}

fn construct_record_link_block(record: &PlayRecord) -> Box<div<String>> {
    let score_id = record.score_metadata().id().to_string();
    let mut classes: SpacedSet<Class> = "basic_btn w_100 m_5 p_5 d_ib f_r t_c f_12 white"
        .try_into()
        .unwrap();
    let additional = match record.score_metadata().difficulty() {
        ScoreDifficulty::Basic => "basic_score_back",
        ScoreDifficulty::Advanced => "advanced_score_back",
        ScoreDifficulty::Expert => "expert_score_back",
        ScoreDifficulty::Master => "master_score_back",
        ScoreDifficulty::Lunatic => "lunatic_score_back",
    };
    classes.insert(additional.try_into().unwrap());
    html!(
        <div class="p_r_5">
            <div class="f_r m_5">
                <form action="https://ongeki-net.com/ongeki-mobile/record/musicDetail/" method="get" accept-charset=["utf-8"]>
                    <img id="myRecord" src="https://ongeki-net.com/ongeki-mobile/img/btn_myrecord.png" class="basic_btn h_35" />
                    <input type="hidden" name="idx" value={score_id.clone()} />
                </form>
            </div>
            <div class={classes} onclick="linkRanking(this)">
                <form action="https://ongeki-net.com/ongeki-mobile/ranking/musicRankingDetail/" method="get" accept-charset=["utf-8"]>
                    "ランキング"
                    <input type="hidden" name="diff" value="2" />
                    <input type="hidden" name="idx" value={score_id} />
                </form>
            </div>
        </div>
    : String)
}

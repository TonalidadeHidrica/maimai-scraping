pub mod associated_user_data;
pub mod data_collector;
pub mod favorite_songs;
pub mod internal_lv_estimator;
pub mod parser;
pub mod rating;
pub mod schema;
pub mod song_list;
pub mod version;

use hashbrown::HashMap;
use maimai_scraping_utils::selector;
use parser::song_score::ScoreIdx;
use schema::latest::SongIcon;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    cookie_store::{AimeIdx, FriendCode},
    maimai::{
        parser::{play_record::parse_record_index, rating_target::RatingTargetFile},
        schema::latest::{Idx, PlayRecord, PlayTime, PlayedAt},
    },
    sega_trait::{
        record_map_serde, AimeEntry, PlayRecordTrait, RecordMap, SegaJapaneseAuth, SegaTrait,
        SegaUserData,
    },
};

use self::parser::aime_selection::ResetChargedAimeForm;

pub struct Maimai;
impl SegaJapaneseAuth for Maimai {
    const LOGIN_FORM_URL: &'static str = "https://maimaidx.jp/maimai-mobile/";
    fn login_form_token_selector() -> &'static Selector {
        selector!(r#"form[action="https://maimaidx.jp/maimai-mobile/submit/"] input[name="token"]"#)
    }
    const LOGIN_URL: &'static str = "https://maimaidx.jp/maimai-mobile/submit/";

    const AIME_LIST_URL: &'static str = "https://maimaidx.jp/maimai-mobile/aimeList/";
    fn select_aime_list_url(aime_idx: AimeIdx) -> String {
        format!(
            "https://maimaidx.jp/maimai-mobile/aimeList/submit/?idx={}",
            aime_idx
        )
    }
    fn parse_aime_selection_page(html: &Html) -> anyhow::Result<Vec<AimeEntry>> {
        parser::aime_selection::parse(html)
    }
    const AIME_SUBMIT_PATH: &'static str = "/maimai-mobile/aimeList/submit/";

    const FRIEND_CODE_URL: &'static str =
        "https://maimaidx.jp/maimai-mobile/friend/userFriendCode/";
    fn parse_friend_code_page(html: &Html) -> anyhow::Result<FriendCode> {
        parser::friend_code::parse(html)
    }

    const HOME_URL: &'static str = "https://maimaidx.jp/maimai-mobile/home/";
    fn switch_to_paid_url(aime_idx: AimeIdx) -> String {
        format!("https://maimaidx.jp/maimai-mobile/resetChargeAime/?idx={aime_idx}")
    }
    type ResetChargedAimeForm = ResetChargedAimeForm;
    fn parse_paid_confirmation(html: &Html) -> anyhow::Result<ResetChargedAimeForm> {
        parser::aime_selection::parse_paid_confirmation(html)
    }
    const SWITCH_PAID_CONFIRMATION_URL: &'static str =
        "https://maimaidx.jp/maimai-mobile/resetChargeAime/submit/";
}
impl SegaTrait for Maimai {
    const ERROR_PATH: &'static str = "/maimai-mobile/error/";
    const RECORD_URL: &'static str = "https://maimaidx.jp/maimai-mobile/record/";

    type UserData = MaimaiUserData;

    fn play_log_detail_url(idx: Idx) -> String {
        format!(
            "https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx={}",
            idx
        )
    }

    fn parse_record_index(html: &Html) -> anyhow::Result<Vec<(PlayTime, Idx)>> {
        parse_record_index(html)
    }

    type PlayRecord = PlayRecord;
    fn parse(html: &Html, idx: Idx) -> anyhow::Result<PlayRecord> {
        parser::play_record::parse(html, idx, true)
    }

    fn play_log_detail_not_found(location: &Url) -> bool {
        location.path() == "/maimai-mobile/record/"
    }

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials_maimai.json";
    const COOKIE_STORE_PATH: &'static str = "./ignore/cookie_store_maimai.json";

    type ForcePaidFlag = bool;
}

pub struct MaimaiIntl;
impl SegaTrait for MaimaiIntl {
    const ERROR_PATH: &'static str = "/maimai-mobile/error/";
    const RECORD_URL: &'static str = "https://maimaidx-eng.com/maimai-mobile/record/";

    type UserData = MaimaiUserData;

    fn play_log_detail_url(idx: Idx) -> String {
        format!(
            "https://maimaidx-eng.com/maimai-mobile/record/playlogDetail/?idx={}",
            idx
        )
    }

    fn parse_record_index(html: &Html) -> anyhow::Result<Vec<(PlayTime, Idx)>> {
        parse_record_index(html)
    }

    type PlayRecord = PlayRecord;
    fn parse(html: &Html, idx: Idx) -> anyhow::Result<PlayRecord> {
        parser::play_record::parse(html, idx, false)
    }

    fn play_log_detail_not_found(location: &Url) -> bool {
        location.path() == "/maimai-mobile/record/"
    }

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials_maimai_intl.json";
    const COOKIE_STORE_PATH: &'static str = "./ignore/cookie_store_maimai_intl.json";

    type ForcePaidFlag = ();
}

#[derive(Default, Serialize, Deserialize)]
pub struct MaimaiUserData {
    #[serde(default)]
    #[serde(serialize_with = "record_map_serde::serialize::<_, Maimai>")]
    #[serde(deserialize_with = "record_map_serde::deserialize::<_, Maimai>")]
    pub records: RecordMap<Maimai>,
    #[serde(default)]
    pub rating_targets: RatingTargetFile,
    #[serde(default)]
    pub idx_to_icon_map: IdxToIconMap,
}
pub type IdxToIconMap = HashMap<ScoreIdx, SongIcon>;
impl SegaUserData<Maimai> for MaimaiUserData {
    fn records_mut(&mut self) -> &mut RecordMap<Maimai> {
        &mut self.records
    }
}
impl SegaUserData<MaimaiIntl> for MaimaiUserData {
    fn records_mut(&mut self) -> &mut RecordMap<MaimaiIntl> {
        &mut self.records
    }
}

impl PlayRecordTrait for PlayRecord {
    type PlayedAt = PlayedAt;
    fn played_at(&self) -> &PlayedAt {
        self.played_at()
    }
    type PlayTime = PlayTime;
    fn time(&self) -> PlayTime {
        (self.idx().timestamp_jst()).unwrap_or(self.played_at().time())
    }
    type Idx = Idx;
    fn idx(&self) -> Idx {
        self.played_at().idx()
    }
}

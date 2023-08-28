pub mod estimate_rating;
pub mod load_score_level;
pub mod play_record_parser;
pub mod rating;
pub mod rating_target_parser;
pub mod schema;
pub mod song_score_parser;

use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use play_record_parser::parse_record_index;
use schema::latest::{Idx, PlayRecord, PlayTime, PlayedAt};

use crate::{
    cookie_store::AimeIdx,
    sega_trait::{record_map_serde, PlayRecordTrait, RecordMap, SegaTrait, SegaUserData},
};

use self::rating_target_parser::RatingTargetFile;

pub struct Maimai;
impl SegaTrait for Maimai {
    const ERROR_PATH: &'static str = "/maimai-mobile/error/";
    const AIME_SUBMIT_PATH: &'static str = "/maimai-mobile/aimeList/submit/";
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
        play_record_parser::parse(html, idx)
    }

    fn play_log_detail_not_found(location: &Url) -> bool {
        location.path() == "/maimai-mobile/record/"
    }

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

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials_maimai.json";
    const COOKIE_STORE_PATH: &'static str = "./ignore/cookie_store_maimai.json";
}

#[derive(Default, Serialize, Deserialize)]
pub struct MaimaiUserData {
    #[serde(default)]
    #[serde(serialize_with = "record_map_serde::serialize::<_, Maimai>")]
    #[serde(deserialize_with = "record_map_serde::deserialize::<_, Maimai>")]
    pub records: RecordMap<Maimai>,
    #[serde(default)]
    pub rating_targets: RatingTargetFile,
}
impl SegaUserData<Maimai> for MaimaiUserData {
    fn records_mut(&mut self) -> &mut RecordMap<Maimai> {
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
        self.played_at().time()
    }
    type Idx = Idx;
    fn idx(&self) -> Idx {
        self.played_at().idx()
    }
}

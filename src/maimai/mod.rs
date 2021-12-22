use chrono::NaiveDateTime;
use scraper::{Html, Selector};
use url::Url;

use crate::{
    cookie_store::AimeIdx,
    play_record_parser::parse_record_index,
    schema::latest::{Idx, PlayRecord, PlayedAt},
    sega_trait::{PlayRecordTrait, SegaTrait},
};

pub struct Maimai;
impl SegaTrait for Maimai {
    const ERROR_PATH: &'static str = "/maimai-mobile/error/";
    const AIME_SUBMIT_PATH: &'static str = "/maimai-mobile/aimeList/submit/";
    const RECORD_URL: &'static str = "https://maimaidx.jp/maimai-mobile/record/";

    type Idx = Idx;
    fn play_log_detail_url(idx: Idx) -> String {
        format!(
            "https://maimaidx.jp/maimai-mobile/record/playlogDetail/?idx={}",
            idx
        )
    }

    fn parse_record_index(html: &Html) -> anyhow::Result<Vec<(NaiveDateTime, Idx)>> {
        parse_record_index(html)
    }

    type PlayRecord = PlayRecord;
    fn parse(html: &Html, idx: Idx) -> anyhow::Result<PlayRecord> {
        crate::play_record_parser::parse(html, idx)
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

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials.json";
}

impl PlayRecordTrait for PlayRecord {
    type PlayedAt = PlayedAt;
    fn played_at(&self) -> &PlayedAt {
        self.played_at()
    }
    fn time(&self) -> NaiveDateTime {
        *self.played_at().time()
    }
    type Idx = Idx;
    fn idx(&self) -> Idx {
        *self.played_at().idx()
    }
}

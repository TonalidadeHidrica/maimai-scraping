use chrono::NaiveDateTime;
use scraper::{Html, Selector};
use url::Url;

use crate::cookie_store::AimeIdx;

pub trait SegaTrait: Sized {
    const ERROR_PATH: &'static str;
    const AIME_SUBMIT_PATH: &'static str;
    const RECORD_URL: &'static str;

    type Idx: Copy;
    fn play_log_detail_url(idx: Self::Idx) -> String;

    fn parse_record_index(html: &Html) -> anyhow::Result<Vec<(NaiveDateTime, Self::Idx)>>;

    type PlayRecord: PlayRecordTrait<Idx = Self::Idx>;
    fn parse(html: &Html, idx: Self::Idx) -> anyhow::Result<Self::PlayRecord>;

    fn play_log_detail_not_found(url: &Url) -> bool;

    const LOGIN_FORM_URL: &'static str;
    fn login_form_token_selector() -> &'static Selector;
    const LOGIN_URL: &'static str;
    const AIME_LIST_URL: &'static str;
    fn select_aime_list_url(idx: AimeIdx) -> String;

    const CREDENTIALS_PATH: &'static str;
    const COOKIE_STORE_PATH: &'static str;
}

pub trait PlayRecordTrait {
    type PlayedAt;
    fn played_at(&self) -> &Self::PlayedAt;
    fn time(&self) -> NaiveDateTime;
    type Idx;
    fn idx(&self) -> Self::Idx;
}

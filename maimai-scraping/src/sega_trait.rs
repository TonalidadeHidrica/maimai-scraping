use scraper::{Html, Selector};
use url::Url;

use crate::cookie_store::AimeIdx;

pub type Idx<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::Idx;
pub type PlayTime<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::PlayTime;
pub type PlayedAt<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::PlayedAt;
pub trait SegaTrait: Sized {
    const ERROR_PATH: &'static str;
    const AIME_SUBMIT_PATH: &'static str;
    const RECORD_URL: &'static str;

    // type Idx: Copy;
    // type PlayTime: Ord + Display;
    fn play_log_detail_url(idx: Idx<Self>) -> String;

    fn parse_record_index(html: &Html) -> anyhow::Result<Vec<(PlayTime<Self>, Idx<Self>)>>;

    type PlayRecord: PlayRecordTrait;
    fn parse(html: &Html, idx: Idx<Self>) -> anyhow::Result<Self::PlayRecord>;

    fn play_log_detail_not_found(url: &Url) -> bool;

    const LOGIN_FORM_URL: &'static str;
    fn login_form_token_selector() -> &'static Selector;
    const LOGIN_URL: &'static str;
    const AIME_LIST_URL: &'static str;
    fn select_aime_list_url(idx: AimeIdx) -> String;

    const CREDENTIALS_PATH: &'static str;
    const COOKIE_STORE_PATH: &'static str;
}

pub trait SegaUserData<T: SegaTrait> {
    fn records(&mut self) -> &mut Vec<T::PlayRecord>;
}

pub trait PlayRecordTrait {
    type PlayedAt;
    fn played_at(&self) -> &Self::PlayedAt;
    type PlayTime;
    fn time(&self) -> Self::PlayTime;
    type Idx;
    fn idx(&self) -> Self::Idx;
}

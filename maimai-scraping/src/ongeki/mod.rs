use anyhow::Context;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

use crate::{
    compare_htmls::elements_are_equivalent,
    sega_trait::{PlayRecordTrait, SegaTrait, SegaUserData, RecordMap},
};

use self::{
    play_record_parser::parse_record_index,
    schema::latest::{Idx, PlayRecord, PlayTime, PlayedAt},
};

pub mod play_record_parser;
pub mod play_record_reconstructor;
pub mod schema;

pub fn check_no_loss(html: &scraper::Html, record: &PlayRecord) -> anyhow::Result<()> {
    let html_reconstructed = play_record_reconstructor::reconstruct(record);
    let html_reconstructed = Html::parse_fragment(&html_reconstructed.to_string());
    let html_reconstructed = ElementRef::wrap(
        html_reconstructed
            .root_element()
            .first_child()
            .context("Reconstructed HTML does not have a child")?,
    )
    .context("Reconstructed HTML is not an element")?;
    let html_actual = html
        .select(selector!(".container3"))
        .next()
        .context(".container3 not found")?;
    elements_are_equivalent(html_reconstructed, html_actual)
}

pub struct Ongeki;
impl SegaTrait for Ongeki {
    const ERROR_PATH: &'static str = "/ongeki-mobile/error/";
    const AIME_SUBMIT_PATH: &'static str = "/ongeki-mobile/aimeList/submit/";
    const RECORD_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/record/playlog/";

    type UserData = OngekiUserData;

    fn play_log_detail_url(idx: Idx) -> String {
        format!(
            "https://ongeki-net.com/ongeki-mobile/record/playlogDetail/?idx={}",
            idx,
        )
    }

    fn parse_record_index(html: &scraper::Html) -> anyhow::Result<Vec<(PlayTime, Idx)>> {
        parse_record_index(html)
    }

    type PlayRecord = PlayRecord;
    fn parse(html: &Html, idx: Idx) -> anyhow::Result<PlayRecord> {
        let res = play_record_parser::parse(html, idx)?;
        check_no_loss(html, &res)?;
        Ok(res)
    }

    fn play_log_detail_not_found(url: &reqwest::Url) -> bool {
        url.path() == "/ongeki-mobile/record/playlog/"
    }

    const LOGIN_FORM_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/";
    fn login_form_token_selector() -> &'static Selector {
        selector!(
            r#"form[action="https://ongeki-net.com/ongeki-mobile/submit/"] input[name="token"]"#
        )
    }
    const LOGIN_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/submit/";
    const AIME_LIST_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/aimeList/";
    fn select_aime_list_url(idx: crate::cookie_store::AimeIdx) -> String {
        format!(
            "https://ongeki-net.com/ongeki-mobile/aimeList/submit/?idx={}",
            idx,
        )
    }

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials_ongeki.json";
    const COOKIE_STORE_PATH: &'static str = "./ignore/cookie_store_ongeki.json";
}

#[derive(Default, Serialize, Deserialize)]
pub struct OngekiUserData {
    pub records: RecordMap<Ongeki>,
}
impl SegaUserData<Ongeki> for OngekiUserData {
    fn records_mut(&mut self) -> &mut RecordMap<Ongeki> {
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

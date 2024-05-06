use std::{
    fmt::Display,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use fs_err::File;
use log::{error, info, trace};
use maimai_scraping_utils::selector;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};

use crate::{
    chrono_util::jst_now,
    compare_htmls::elements_are_equivalent,
    cookie_store::{AimeIdx, FriendCode, PlayerName},
    sega_trait::{
        record_map_serde, PlayRecordTrait, RecordMap, SegaJapaneseAuth, SegaTrait, SegaUserData,
    },
};

use self::{
    play_record_parser::parse_record_index,
    schema::latest::{Idx, PlayRecord, PlayTime, PlayedAt},
};

pub mod aime_selection_parser;
pub mod friend_code_parser;
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
    let res = elements_are_equivalent(html_reconstructed, html_actual);
    if let Err(e) = &res {
        error!("{e:#}");
        let dir = PathBuf::from(format!("ignore/maimai-watcher-log-{}", jst_now()));
        match fs_err::create_dir_all(&dir) {
            Err(e) => error!("Failed to create directory: {e:#}"),
            Ok(_) => {
                try_write(&dir.join("page.html"), "full HTML document", html.html());
                try_write(
                    &dir.join("reconstructed.html"),
                    "parsed HTML element",
                    html_reconstructed.html(),
                );
                try_write(
                    &dir.join("actual.html"),
                    "reconstructed HTML element",
                    html_actual.html(),
                );
                try_write(
                    &dir.join("parsed.rs"),
                    "parse result",
                    format_args!("{record:#?}"),
                );
            }
        };
    }
    res
}

fn try_write(path: &Path, description: &'static str, content: impl Display) {
    let write_result = (|| {
        writeln!(BufWriter::new(File::create(path)?), "{content}")?;
        anyhow::Ok(())
    })();
    match write_result {
        Ok(_) => {
            info!("Saved {description} to {path:?}")
        }
        Err(e) => {
            error!("Failed to write contents to {path:?}: {e:#}");
            trace!("Tried to write: {content}")
        }
    }
}

pub struct Ongeki;
impl SegaJapaneseAuth for Ongeki {
    const LOGIN_FORM_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/";
    fn login_form_token_selector() -> &'static Selector {
        selector!(
            r#"form[action="https://ongeki-net.com/ongeki-mobile/submit/"] input[name="token"]"#
        )
    }
    const LOGIN_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/submit/";

    const AIME_LIST_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/aimeList/";
    fn parse_aime_selection_page(html: &Html) -> anyhow::Result<Vec<(AimeIdx, PlayerName)>> {
        aime_selection_parser::parse(html)
    }
    fn select_aime_list_url(idx: AimeIdx) -> String {
        format!(
            "https://ongeki-net.com/ongeki-mobile/aimeList/submit/?idx={}",
            idx,
        )
    }
    const AIME_SUBMIT_PATH: &'static str = "/ongeki-mobile/aimeList/submit/";

    const FRIEND_CODE_URL: &'static str =
        "https://ongeki-net.com/ongeki-mobile/friend/userFriendCode/";
    fn parse_friend_code_page(html: &Html) -> anyhow::Result<FriendCode> {
        friend_code_parser::parse(html)
    }

    const HOME_URL: &'static str = "https://ongeki-net.com/ongeki-mobile/home/";
}
impl SegaTrait for Ongeki {
    const ERROR_PATH: &'static str = "/ongeki-mobile/error/";
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

    const CREDENTIALS_PATH: &'static str = "./ignore/credentials_ongeki.json";
    const COOKIE_STORE_PATH: &'static str = "./ignore/cookie_store_ongeki.json";

    type ForcePaidFlag = bool;
}

#[derive(Default, Serialize, Deserialize)]
pub struct OngekiUserData {
    #[serde(default)]
    #[serde(serialize_with = "record_map_serde::serialize::<_, Ongeki>")]
    #[serde(deserialize_with = "record_map_serde::deserialize::<_, Ongeki>")]
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

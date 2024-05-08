use std::{collections::BTreeSet, path::Path, thread::sleep, time::Duration};

use anyhow::{anyhow, bail, Context};
use chrono::NaiveDateTime;
use headless_chrome::{
    browser::tab::element::BoxModel,
    protocol::cdp::Page::{CaptureScreenshotFormatOption::Png, Viewport},
    Browser, LaunchOptionsBuilder,
};
use itertools::Itertools;
use log::info;
use maimai_scraping::{
    api::find_aime_idx,
    cookie_store::UserIdentifier,
    maimai::{
        data_collector::RATING_TARGET_URL,
        parser::{self, aime_selection, play_record},
        Maimai,
    },
    sega_trait::{SegaJapaneseAuth, SegaTrait},
};
use maimai_scraping_utils::sega_id::Credentials;
use scraper::Html;

const TIMESTAMP_FORMAT: &str = "%Y-%m-%d_%H-%M-%S";

pub fn generate(
    img_save_dir: &Path,
    credentials: Credentials,
    user_identifier: UserIdentifier,
    port: Option<u16>,
) -> anyhow::Result<()> {
    let wait = || sleep(Duration::from_secs(1));

    fs_err::create_dir_all(img_save_dir)?;
    let files: Vec<_> = fs_err::read_dir(img_save_dir)?
        .map(|entry| {
            anyhow::Ok(
                entry?
                    .file_name()
                    .into_string()
                    .map_err(|e| anyhow!("Non-Unicode filename: {e:?}"))?,
            )
        })
        .try_collect()?;
    let get_timestamp = |delimiter: &'static str| {
        move |s: &String| {
            NaiveDateTime::parse_from_str(s.split_once(delimiter)?.0, TIMESTAMP_FORMAT).ok()
        }
    };
    let playlog_existing: BTreeSet<_> = files
        .iter()
        .filter_map(get_timestamp("_playlogDetail_"))
        .collect();
    let rating_target_existing: BTreeSet<_> = files
        .iter()
        .filter_map(get_timestamp("_ratingTarget.png"))
        .collect();

    info!("Logging in...");
    let browser = Browser::new(
        LaunchOptionsBuilder::default()
            .port(port)
            .window_size(Some((1920, 19200))) // Wow, huge window...
            .build()?,
    )
    .expect("Failed to create browser");
    let tab = browser.new_tab()?;
    tab.navigate_to(Maimai::LOGIN_FORM_URL)?;
    tab.wait_for_element("input[name='segaId']")?
        .type_into(credentials.sega_id.as_ref())?;
    tab.wait_for_element("input[name='password']")?
        .type_into(credentials.password.as_ref())?;
    wait();
    tab.wait_for_element("button[type='submit']")?.click()?;

    tab.wait_for_element(aime_selection::DIV)?;
    let aime_list = aime_selection::parse(&Html::parse_document(&tab.get_content()?))?;
    info!("{aime_list:?}");
    let aime_entry = find_aime_idx(&aime_list, user_identifier.player_name.as_ref())?;
    wait();
    tab.wait_for_element(&format!(
        r#"input[name="idx"][value="{}"] + button"#,
        aime_entry.idx,
    ))?
    .click()?;

    tab.wait_for_element("div.see_through_block")?;
    if tab.get_url() != Maimai::HOME_URL {
        bail!("Failed to log in");
    }
    if let Some(friend_code) = &user_identifier.friend_code {
        wait();
        tab.navigate_to(Maimai::FRIEND_CODE_URL)?;
        let found = tab
            .wait_for_element(parser::friend_code::DIV)?
            .get_inner_text()?;
        if &found != friend_code.as_ref() {
            bail!("Unexpected friend code: expected {friend_code:?}, found {found:?}")
        }
    }
    info!("Successfully logged in.");

    info!("Retrieving play records.");
    wait();
    tab.navigate_to(Maimai::RECORD_URL)?;
    tab.wait_until_navigated()?;
    let records = play_record::parse_record_index(&Html::parse_document(&tab.get_content()?))?;
    for &(_time, idx) in &records {
        let timestamp = idx.timestamp_jst().context("Timestamp exists")?.get();
        if playlog_existing.contains(&timestamp) {
            info!("The following playlog is already saved: {idx:?}");
            continue;
        }
        wait();
        tab.navigate_to(&Maimai::play_log_detail_url(idx))?;
        tab.wait_until_navigated()?;
        let viewport = {
            let top = tab
                .wait_for_element("div.playlog_top_container")?
                .get_box_model()?;
            let bottom = tab
                .wait_for_element(".gray_block:has(.playlog_fl_block)")?
                .get_box_model()?;
            let margin = 10.;
            viewport_by_top_and_bottom(top, bottom, margin)
        };
        let png_path = {
            let title_escaped = {
                let record =
                    play_record::parse(&Html::parse_document(&tab.get_content()?), idx, true)?;
                let title: &str = record.song_metadata().name().as_ref();
                title.replace(disallowed_for_filename, "_")
            };
            let timestamp = timestamp.format(TIMESTAMP_FORMAT);
            img_save_dir
                .to_owned()
                .join(format!("{timestamp}_playlogDetail_{title_escaped}.png"))
        };
        let screenshot = tab.capture_screenshot(Png, None, Some(viewport), true)?;
        fs_err::write(png_path, screenshot)?;
    }

    let latest_timestamp = records
        .iter()
        .map(|r| r.1.timestamp_jst().unwrap().get())
        .max()
        .context("No record?  Unlikely to happen.")?;
    if !rating_target_existing.contains(&latest_timestamp) {
        info!("Retrieving rating targets.");
        wait();
        tab.navigate_to(RATING_TARGET_URL)?;
        tab.wait_until_navigated()?;
        if tab.get_url().as_str() != RATING_TARGET_URL {
            bail!("Failed to navigate to rating target");
        }

        let viewport = {
            let top = tab.wait_for_element("img.title")?.get_box_model()?;
            let bottom = tab.wait_for_element("div:has(+footer)")?.get_box_model()?;
            viewport_by_top_and_bottom(top, bottom, 10.)
        };
        let png_path = {
            let timestamp = latest_timestamp.format(TIMESTAMP_FORMAT);
            img_save_dir
                .to_owned()
                .join(format!("{timestamp}_ratingTarget.png"))
        };
        let screenshot = tab.capture_screenshot(Png, None, Some(viewport), true)?;
        fs_err::write(png_path, screenshot)?;
    } else {
        info!("Rating target is already saved.");
    }

    info!("Done.");
    Ok(())
}

fn viewport_by_top_and_bottom(top: BoxModel, bottom: BoxModel, margin: f64) -> Viewport {
    let y = top.content.most_top();
    Viewport {
        x: bottom.content.most_left() - margin,
        y: y - margin,
        width: bottom.content.width() + margin * 2.,
        height: bottom.padding.bottom_right.y - y + margin * 2.,
        scale: 1.,
    }
}

fn disallowed_for_filename(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{1F}' | '<' | '>' | ':' | '\\' | '|' | '?' | '*' | '"' | '/'
    )
}

use std::{
    collections::BTreeSet,
    path::Path,
    sync::{mpsc, Arc},
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, bail, Context};
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use headless_chrome::{
    browser::tab::{element::BoxModel, RequestPausedDecision},
    protocol::cdp::{
        Fetch::{events::RequestPausedEvent, RequestPattern, RequestStage},
        Page::{CaptureScreenshotFormatOption::Png, Viewport},
    },
    Browser, LaunchOptionsBuilder, Tab,
};
use itertools::Itertools;
use log::info;
use maimai_scraping::{
    api::find_aime_idx,
    cookie_store::UserIdentifier,
    maimai::{
        data_collector::RATING_TARGET_URL,
        parser::{self, aime_selection, play_record},
        schema::latest::{Idx, PlayTime},
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
    records: Option<Vec<(PlayTime, Idx)>>,
    port: Option<u16>,
    run_tool: bool,
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
    let files_existing = BTreeSet::from_iter(files);

    info!("Logging in...");
    let browser = Browser::new(
        LaunchOptionsBuilder::default()
            .port(port)
            .window_size(Some((1920, 19200))) // Wow, huge window...
            .build()?,
    )
    .expect("Failed to create browser");
    let tab = browser.new_tab()?;
    tab.enable_fetch(
        Some(&[RequestPattern {
            url_pattern: None,
            resource_Type: None,
            request_stage: Some(RequestStage::Response),
        }]),
        None,
    )?;

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
    let records = match records {
        Some(records) => records,
        None => {
            wait();
            tab.navigate_to(Maimai::RECORD_URL)?;
            tab.wait_until_navigated()?;
            play_record::parse_record_index(&Html::parse_document(&tab.get_content()?))?
        }
    };
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

    let latest_timestamp_fmt = {
        let latest_timestamp = records
            .iter()
            .map(|r| r.1.timestamp_jst().unwrap().get())
            .max()
            .context("No record?  Unlikely to happen.")?;
        latest_timestamp.format(TIMESTAMP_FORMAT)
    };
    let png_name = format!("{latest_timestamp_fmt}_ratingTarget.png");
    if !files_existing.contains(&png_name) {
        info!("Retrieving rating targets.");
        wait();
        tab.navigate_to(RATING_TARGET_URL)?;
        tab.wait_until_navigated()?;
        if tab.get_url().as_str() != RATING_TARGET_URL {
            bail!("Failed to navigate to rating target");
        }

        let screenshot = screenshot_rating_target(&tab)?;
        fs_err::write(img_save_dir.to_owned().join(png_name), screenshot)?;
    } else {
        info!("Rating target is already saved.");
    }

    if run_tool {
        info!("Running the tool.");
        if tab.get_url() != RATING_TARGET_URL {
            info!("Not in the rating target page!  Navigating there first.");
            tab.navigate_to(RATING_TARGET_URL)?;
            tab.wait_until_navigated()?;
            wait();
            info!("Navigation done.");
        }
        let update_time = {
            // Run the tool, and get the date
            let (rx, tx) = mpsc::channel();
            tab.enable_request_interception(Arc::new(move |_, _, request: RequestPausedEvent| {
                if let Some(time) = get_last_modified(request) {
                    let _ = rx.send(time);
                }
                RequestPausedDecision::Continue(None)
            }))?;
            tab.evaluate(include_str!("bookmarklet.js"), true)?;
            match tx.recv_timeout(Duration::from_secs(20)) {
                Ok(date) => date.format(TIMESTAMP_FORMAT),
                Err(err) => bail!("Failed to get last-modified header: {err:?}"),
            }
        };
        info!("The tool was updated at {update_time}.");

        let png_name = format!("{latest_timestamp_fmt}_tool_{update_time}_list.png");
        if !files_existing.contains(&png_name) {
            info!("Getting the screenshot of song list in text format.");
            wait_until_loaded(&tab)?;
            let screenshot = screenshot_rating_target(&tab)?;
            fs_err::write(img_save_dir.to_owned().join(png_name), screenshot)?;
            info!("List view has been captured.");
        } else {
            info!("Screenshot of song list in text format is already retrieved.");
        }

        let png_name = format!("{latest_timestamp_fmt}_tool_{update_time}_tiles.png");
        if !files_existing.contains(&png_name) {
            info!("Getting the screenshot of song list as icon grid.");
            tab.wait_for_element("img.title")?.click()?;
            wait_until_loaded(&tab)?;
            let screenshot = screenshot_rating_target(&tab)?;
            fs_err::write(img_save_dir.to_owned().join(png_name), screenshot)?;
            info!("Grid view has been captured.");
        } else {
            info!("Screenshot of song list in grid view is already retrieved.");
        }
    }

    info!("Done.");
    Ok(())
}

fn screenshot_rating_target(tab: &Arc<Tab>) -> anyhow::Result<Vec<u8>> {
    let viewport = {
        let top = tab.wait_for_element("img.title")?.get_box_model()?;
        let screen = tab.wait_for_element(".screw_block")?.get_box_model()?;
        let bottom = tab.wait_for_element("div:has(+footer)")?.get_box_model()?;
        let margin = 10.;
        let y = top.content.most_top();
        Viewport {
            x: screen.border.most_left() - margin,
            y: y - margin,
            width: screen.border.width() + margin * 2.,
            height: bottom.border.bottom_right.y - y + margin * 2.,
            scale: 1.,
        }
    };
    let screenshot = tab.capture_screenshot(Png, None, Some(viewport), true)?;
    Ok(screenshot)
}

fn wait_until_loaded(tab: &Arc<Tab>) -> anyhow::Result<()> {
    while {
        info!("Waiting for DOM to be drawn");
        sleep(Duration::from_secs(1));
        tab.evaluate("document.readyState", false)?.value
            != Some(serde_json::Value::String("complete".to_owned()))
    } {}
    info!("The page is ready to be captured.");
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

fn get_last_modified(request: RequestPausedEvent) -> Option<DateTime<FixedOffset>> {
    if request.params.request.url != "https://sgimera.github.io/mai_RatingAnalyzer/maidx_tools.js" {
        return None;
    }
    let mut headers = request.params.response_headers.iter().flatten();
    let header = headers.find(|header| (header.name.to_lowercase() == "last-modified"))?;
    let time = NaiveDateTime::parse_from_str(&header.value, "%a, %d %b %Y %H:%M:%S GMT").ok()?;
    let timezone = FixedOffset::east_opt(9 * 60 * 60).unwrap();
    Some(time.and_utc().with_timezone(&timezone))
}

use std::{path::Path, thread::sleep, time::Duration};

use anyhow::{bail, Context};
use headless_chrome::{
    protocol::cdp::Page::{CaptureScreenshotFormatOption::Png, Viewport},
    Browser, LaunchOptionsBuilder,
};
use log::info;
use maimai_scraping::{
    api::find_aime_idx,
    cookie_store::UserIdentifier,
    maimai::{
        parser::{self, aime_selection, play_record},
        Maimai,
    },
    sega_trait::{SegaJapaneseAuth, SegaTrait},
};
use maimai_scraping_utils::sega_id::Credentials;
use scraper::Html;

pub fn generate(
    img_save_dir: &Path,
    credentials: Credentials,
    user_identifier: UserIdentifier,
    port: Option<u16>,
) -> anyhow::Result<()> {
    let wait = || sleep(Duration::from_secs(1));

    fs_err::create_dir_all(img_save_dir)?;

    let browser = Browser::new(
        LaunchOptionsBuilder::default()
            .port(port)
            .window_size(Some((1920, 1920)))
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
    let aime_idx = find_aime_idx(&aime_list, user_identifier.player_name.as_ref())?;
    wait();
    tab.wait_for_element(&format!(
        r#"input[name="idx"][value="{aime_idx}"] + button"#
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

    wait();
    tab.navigate_to(Maimai::RECORD_URL)?;
    tab.wait_until_navigated()?;
    let records = play_record::parse_record_index(&Html::parse_document(&tab.get_content()?))?;
    for (_time, idx) in records {
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
            let margin = 5.;
            let y = top.content.most_top();
            Viewport {
                x: top.content.most_left() - margin,
                y: y - margin,
                width: top.content.width() + margin * 2.,
                height: bottom.padding.bottom_right.y - y + margin * 2.,
                scale: 1.,
            }
        };
        let png_path = {
            let title_escaped = {
                let record =
                    play_record::parse(&Html::parse_document(&tab.get_content()?), idx, true)?;
                let title: &str = record.song_metadata().name().as_ref();
                title.replace(disallowed_for_filename, "_")
            };
            let timestamp = {
                let time = idx.timestamp_jst().context("Timestamp exists")?.get();
                time.format("%Y-%m-%d_%H-%M-%S")
            };
            img_save_dir
                .to_owned()
                .join(format!("{timestamp}_playlogDetail_{title_escaped}.png"))
        };
        let screenshot = tab.capture_screenshot(Png, None, Some(viewport), true)?;
        fs_err::write(png_path, screenshot)?;
    }

    Ok(())
}

fn disallowed_for_filename(c: char) -> bool {
    matches!(
        c,
        '\u{0}'..='\u{1F}' | '<' | '>' | ':' | '\\' | '|' | '?' | '*' | '"' | '/'
    )
}

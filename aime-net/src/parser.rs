use anyhow::{anyhow, Context};
use getset::{CopyGetters, Getters};
use itertools::Itertools;
use maimai_scraping_utils::selector;
use scraper::{selectable::Selectable, ElementRef, Html};
use serde::Serialize;

use crate::schema::{AccessCode, BlockId, SlotNo, VcToken};

#[derive(Debug, Getters)]
#[getset(get = "pub")]
pub struct AimeIndex {
    slots: [AimeSlot; 3],
}

#[derive(Debug)]
pub enum AimeSlot {
    Empty(EmptySlot),
    Filled(FilledSlot),
}

#[derive(Debug, Getters, CopyGetters)]
pub struct FilledSlot {
    #[getset(get_copy = "pub")]
    access_code: AccessCode,
    #[getset(get = "pub")]
    block_id: BlockId,
}

#[derive(Debug, CopyGetters)]
pub struct EmptySlot {
    #[getset(get_copy = "pub")]
    slot_no: SlotNo,
}

pub fn parse_aime_index(html: &Html) -> anyhow::Result<AimeIndex> {
    fs_err::write("ignore/aime_index.html", html.html())?;
    let slots = html
        .select(selector!("li.c-myaime__target__item"))
        .enumerate()
        .map(|(slot_no, li)| {
            anyhow::Ok(match parse_aime_slot(li)? {
                None => AimeSlot::Empty(EmptySlot {
                    slot_no: slot_no.into(),
                }),
                Some(slot) => AimeSlot::Filled(slot),
            })
        })
        .try_collect::<_, Vec<_>, _>()?
        .try_into()
        .map_err(|_| anyhow!("Unexpected number of aime elements found"))?;
    Ok(AimeIndex { slots })
}

fn parse_aime_slot(li: ElementRef) -> anyhow::Result<Option<FilledSlot>> {
    let access_code = match li.select(selector!("dd.c-aime__info__data")).next() {
        Some(dd) => dd.text().collect::<String>().parse()?,
        None => return Ok(None),
    };
    let block_id = li
        .select(selector!(r#"input[name="blockId"]"#))
        .next()
        .context("Block id not found")?
        .attr("value")
        .context("Attribute `value` not found for block id")?
        .to_owned()
        .into();
    Ok(Some(FilledSlot {
        access_code,
        block_id,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveConfirmForm {
    block_id: BlockId,
    #[serde(rename = "vctoken")]
    vc_token: VcToken,
}

pub fn parse_remove_confirm_page(html: &Html) -> anyhow::Result<RemoveConfirmForm> {
    let block_id = html
        .select(selector!(r#"input[name="blockId"]"#))
        .next()
        .context("blockId element not found")?
        .attr("value")
        .context("blockId value not found")?
        .to_owned()
        .into();
    let vc_token = html
        .select(selector!(r#"input[name="vctoken"]"#))
        .next()
        .context("vctoken element not found")?
        .attr("value")
        .context("vctoken value not found")?
        .to_owned()
        .into();
    Ok(RemoveConfirmForm { block_id, vc_token })
}

#[derive(Getters)]
#[getset(get = "pub")]
pub struct AddInputPage {
    error: Option<AddInputPageError>,
}

#[derive(Debug)]
pub struct AddInputPageError(#[allow(unused)] String);

pub fn parse_add_input_page(html: &Html) -> anyhow::Result<AddInputPage> {
    let error = html
        .select(selector!("c-form__error"))
        .next()
        .map(|x| AddInputPageError(x.text().collect::<String>()));
    Ok(AddInputPage { error })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddConfirmForm<'a> {
    #[serde(rename = "vctoken")]
    vc_token: &'a str,
    slot_no: &'a str,
    access_code: &'a str,
    comment: &'a str,
}

pub fn parse_add_confirm_form(html: &Html) -> anyhow::Result<AddConfirmForm> {
    let vc_token = html
        .select(selector!(r#"input[name="vctoken"]"#))
        .next()
        .context("vctoken element not found")?
        .attr("value")
        .context("vctoken value not found")?;
    let slot_no = html
        .select(selector!(r#"input[name="slotNo"]"#))
        .next()
        .context("slotNo element not found")?
        .attr("value")
        .context("slotNo value not found")?;
    let access_code = html
        .select(selector!(r#"input[name="accessCode"]"#))
        .next()
        .context("accessCode element not found")?
        .attr("value")
        .context("accessCode value not found")?;
    let comment = html
        .select(selector!(r#"input[name="comment"]"#))
        .next()
        .context("comment element not found")?
        .attr("value")
        .context("comment value not found")?;
    Ok(AddConfirmForm {
        vc_token,
        slot_no,
        access_code,
        comment,
    })
}

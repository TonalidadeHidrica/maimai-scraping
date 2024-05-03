use anyhow::{anyhow, Context};
use getset::{CopyGetters, Getters};
use itertools::Itertools;
use maimai_scraping_utils::selector;
use scraper::{selectable::Selectable, ElementRef, Html};

use crate::schema::{AccessCode, BlockId};

#[derive(Getters)]
#[getset(get = "pub")]
pub struct AimeIndex {
    slots: [Option<AimeSlot>; 3],
}

#[derive(Getters, CopyGetters)]
pub struct AimeSlot {
    #[getset(get_copy = "pub")]
    access_code: AccessCode,
    #[getset(get = "pub")]
    block_id: BlockId,
}

pub fn aime_index(html: &Html) -> anyhow::Result<AimeIndex> {
    let slots = html
        .select(selector!("li.c-myaime__target__item"))
        .map(parse_aime_slot)
        .try_collect::<_, Vec<_>, _>()?
        .try_into()
        .map_err(|_| anyhow!("Unexpected number of aime elements found"))?;
    Ok(AimeIndex { slots })
}

fn parse_aime_slot(li: ElementRef) -> anyhow::Result<Option<AimeSlot>> {
    let access_code = match li.select(selector!("dd.c-aime__info__data")).next() {
        Some(dd) => dd.text().collect::<String>().parse()?,
        None => return Ok(None),
    };
    let block_id = li
        .select(selector!(r#"input[name="blockId"]"#))
        .next()
        .context("Block id not found")?
        .text()
        .collect::<String>()
        .into();
    Ok(Some(AimeSlot {
        access_code,
        block_id,
    }))
}

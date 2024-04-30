use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongRaw {
    pub title: String,
    pub title_kana: String,
    pub artist: String,
    /// Category (in Japanese, can be enum)
    pub catcode: String,
    pub image_url: String,

    /// Release date? (Can be "000000", unclear if it's reliable)
    pub release: String,
    /// Integer that decides default song order
    #[serde_as(as = "DisplayFromStr")]
    pub sort: u64,
    /// Five-digit integer that seeminlgy corresponds to the release date of score
    pub version: String,

    /// "NEW" if new song (or score?)
    pub date: Option<String>,
    pub dx_lev_adv: Option<String>,
    pub dx_lev_bas: Option<String>,
    pub dx_lev_exp: Option<String>,
    pub dx_lev_mas: Option<String>,
    pub dx_lev_remas: Option<String>,
    /// "○" if unlocking song is required
    pub key: Option<String>,
    pub lev_adv: Option<String>,
    pub lev_bas: Option<String>,
    pub lev_exp: Option<String>,
    pub lev_mas: Option<String>,
    pub lev_remas: Option<String>,

    /// Succeeded by "？" if utage
    pub lev_utage: Option<String>,
    /// Comment for utage score (perhaps)
    pub comment: Option<String>,
    /// Utage kanji
    pub kanji: Option<String>,
    /// "○" if the score is buddy
    pub buddy: Option<String>,
}

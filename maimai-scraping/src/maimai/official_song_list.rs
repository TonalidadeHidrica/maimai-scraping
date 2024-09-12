use std::{cmp::Ordering, path::PathBuf, str::FromStr};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use deranged::RangedU8;
use derive_more::{AsRef, From};
use getset::{CopyGetters, Getters};
use itertools::Itertools;
use maimai_scraping_utils::fs_json_util::read_json;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};

use super::{
    load_score_level::MaimaiVersion,
    rating::ScoreLevel,
    schema::latest::{SongIcon, SongName, UtageKindRaw},
};

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
    /// "○" if unlocking song is required
    pub key: Option<String>,

    pub dx_lev_bas: Option<String>,
    pub dx_lev_adv: Option<String>,
    pub dx_lev_exp: Option<String>,
    pub dx_lev_mas: Option<String>,
    pub dx_lev_remas: Option<String>,

    pub lev_bas: Option<String>,
    pub lev_adv: Option<String>,
    pub lev_exp: Option<String>,
    pub lev_mas: Option<String>,
    pub lev_remas: Option<String>,

    /// Succeeded by "?" if utage
    pub lev_utage: Option<String>,
    /// Comment for utage score (perhaps)
    pub comment: Option<String>,
    /// Utage kanji
    pub kanji: Option<String>,
    /// "○" if the score is buddy
    pub buddy: Option<String>,
}

#[derive(Debug, Getters, CopyGetters)]
pub struct Song {
    #[getset(get = "pub")]
    title: SongName,
    #[getset(get = "pub")]
    title_kana: SongKana,
    #[getset(get = "pub")]
    artist: ArtistName,
    #[getset(get = "pub")]
    image: SongIcon,

    #[getset(get_copy = "pub")]
    release: Option<NaiveDate>,
    #[getset(get_copy = "pub")]
    sort: SortIndex,
    #[getset(get_copy = "pub")]
    version: Version,

    #[getset(get_copy = "pub")]
    new: bool,
    #[getset(get_copy = "pub")]
    locked: bool,

    #[getset(get = "pub")]
    details: ScoreDetails,
}

#[derive(Debug, From, AsRef)]
pub struct SongKana(String);

#[derive(Debug, From, AsRef)]
pub struct ArtistName(String);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, From)]
pub struct SortIndex(u64);

#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct Version {
    version: MaimaiVersion,
    suffix: RangedU8<0, 99>,
}

#[derive(Debug)]
pub enum ScoreDetails {
    Ordinary(OrdinaryScore),
    Utage(UtageScore),
}

// Either standard or deluxe is Some
#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct OrdinaryScore {
    category: Category,
    standard: Option<Levels>,
    deluxe: Option<Levels>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Category {
    GamesVariety,
    PopsAnime,
    MaimaiOriginal,
    NiconicoVocaloid,
    OngekiChunithm,
    TouhouProject,
}

#[derive(Clone, Copy, Debug, CopyGetters)]
#[getset(get_copy = "pub")]
pub struct Levels {
    basic: ScoreLevel,
    advanced: ScoreLevel,
    expert: ScoreLevel,
    master: ScoreLevel,
    re_master: Option<ScoreLevel>,
}

#[derive(Debug, Getters, CopyGetters)]
pub struct UtageScore {
    #[getset(get_copy = "pub")]
    level: ScoreLevel,
    #[getset(get = "pub")]
    comment: UtageComment,
    #[getset(get = "pub")]
    kanji: UtageKindRaw,
    #[getset(get_copy = "pub")]
    buddy: bool,
}

#[derive(Debug, From, AsRef)]
pub struct UtageComment(String);

impl TryFrom<SongRaw> for Song {
    type Error = anyhow::Error;

    fn try_from(song: SongRaw) -> anyhow::Result<Self> {
        let standard = Levels::parse([
            &song.lev_bas,
            &song.lev_adv,
            &song.lev_exp,
            &song.lev_mas,
            &song.lev_remas,
        ])?;
        let deluxe = Levels::parse([
            &song.dx_lev_bas,
            &song.dx_lev_adv,
            &song.dx_lev_exp,
            &song.dx_lev_mas,
            &song.dx_lev_remas,
        ])?;
        let buddy = match song.buddy.as_deref() {
            Some("○") => true,
            None => false,
            _ => bail!("Unexpected `buddy`: {:?}", song.buddy),
        };
        let utage = UtageScore::parse(song.lev_utage, song.comment, song.kanji, buddy)?;
        let details = if standard.is_none() && deluxe.is_none() {
            match (&song.catcode[..], utage) {
                ("宴会場", Some(utage)) => ScoreDetails::Utage(utage),
                x => bail!("Wrong category code or utage data not found: {x:?}"),
            }
        } else if utage.is_none() && !buddy {
            ScoreDetails::Ordinary(OrdinaryScore {
                category: song.catcode.parse()?,
                standard,
                deluxe,
            })
        } else {
            bail!("Ordinary score with utage: {utage:?}, buddy={buddy}");
        };
        Ok(Song {
            title: song.title.into(),
            title_kana: song.title_kana.into(),
            artist: song.artist.into(),
            image: format!(
                "https://maimaidx.jp/maimai-mobile/img/Music/{}",
                song.image_url
            )
            .parse()?,

            release: match &song.release[..] {
                "000000" => None,
                s => Some(
                    NaiveDate::parse_from_str(s, "%y%m%d")
                        .with_context(|| format!("While trying to parse {s:?}"))?,
                ),
            },
            sort: song.sort.into(),
            version: song.version.parse()?,

            new: match song.date.as_deref() {
                Some("NEW") => true,
                None => false,
                x => bail!("Unexpected `date`: {x:?}"),
            },
            locked: match song.key.as_deref() {
                Some("○") => true,
                None => false,
                x => bail!("Unexpected `key`: {x:?}"),
            },

            details,
        })
    }
}

impl FromStr for Version {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        use MaimaiVersion::*;
        // let (x, y) = (value / 100, value % 100);
        if !value.is_char_boundary(3) {
            bail!("Unexpected version: {value:?}");
        }
        let (x, y) = value.split_at(3);
        let version = match x {
            "100" => Maimai,
            "110" => MaimaiPlus,
            "120" => Green,
            "130" => GreenPlus,
            "140" => Orange,
            "150" => OrangePlus,
            "160" => Pink,
            "170" => PinkPlus,
            "180" => Murasaki,
            "185" => MurasakiPlus,
            "190" => Milk,
            "195" => MilkPlus,
            "199" => Finale,
            "200" => Deluxe,
            "205" => DeluxePlus,
            "210" => Splash,
            "215" => SplashPlus,
            "220" => Universe,
            "225" => UniversePlus,
            "230" => Festival,
            "235" => FestivalPlus,
            "240" => Buddies,
            "245" => BuddiesPlus,
            "250" => Prism,
            _ => bail!("Unexpected version: {value:?}"),
        };
        let suffix = y.parse()?; // Guaranteed to be in [0, 100)
        Ok(Self { version, suffix })
    }
}

impl FromStr for Category {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        use Category::*;
        Ok(match s {
            "ゲーム＆バラエティ" | "GAME＆VARIETY" => GamesVariety,
            "POPS＆アニメ" | "POPS＆ANIME" => PopsAnime,
            "maimai" => MaimaiOriginal,
            "niconico＆ボーカロイド" | "niconico＆VOCALOID™" => NiconicoVocaloid,
            "オンゲキ＆CHUNITHM" => OngekiChunithm,
            "東方Project" => TouhouProject,
            _ => bail!("Unexpected category: {s:?}"),
        })
    }
}

impl Levels {
    fn parse(levels: [&Option<String>; 5]) -> anyhow::Result<Option<Self>> {
        Ok(match levels.map(|s| s.as_deref().map(str::parse)) {
            [None, None, None, None, None] => None,
            [Some(b), Some(a), Some(e), Some(m), r] => Some(Self {
                basic: b?,
                advanced: a?,
                expert: e?,
                master: m?,
                re_master: r.transpose()?,
            }),
            _ => bail!("Unexpected levels: {levels:?}"),
        })
    }
}

impl UtageScore {
    fn parse(
        lev_utage: Option<String>,
        comment: Option<String>,
        kanji: Option<String>,
        buddy: bool,
    ) -> anyhow::Result<Option<Self>> {
        Ok(match (lev_utage, comment, kanji) {
            (Some(level), Some(comment), Some(kanji)) => Some(Self {
                level: level
                    .strip_suffix('?')
                    .with_context(|| format!("Utage level does not end with `？`: {level:?}"))?
                    .parse()?,
                comment: comment.into(),
                kanji: kanji.into(),
                buddy,
            }),
            (None, None, None) => None,
            x => bail!("Unexpected type of song: {x:?}"),
        })
    }
}

pub fn load(path: impl Into<PathBuf>) -> anyhow::Result<Vec<Song>> {
    let official_songs: Vec<SongRaw> = read_json(path.into())?;
    official_songs.into_iter().map(TryInto::try_into).collect()
}

impl PartialEq for SongKana {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for SongKana {}
impl PartialOrd for SongKana {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for SongKana {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use Ordering::*;
        self.0
            .chars()
            .zip_longest(other.0.chars())
            .map(|x| match x {
                itertools::EitherOrBoth::Both(x, y) => {
                    maimai_char_order(x).cmp(&maimai_char_order(y))
                }
                itertools::EitherOrBoth::Left(_) => Greater,
                itertools::EitherOrBoth::Right(_) => Less,
            })
            .find(|x| x.is_ne())
            .unwrap_or(Equal)
    }
}
fn maimai_char_order(c: char) -> (u8, char) {
    match c {
        'ア'..='ン' => (0, c),
        'A'..='Z' => (1, c),
        '0'..='9' => (2, c),
        _ => (3, c),
    }
}

#[cfg(test)]
mod tests {
    use super::SongKana;

    #[test]
    fn test_song_kana_cmp() {
        let [x, y]: [SongKana; 2] =
            ["ソウキユウフカク", "YETANOTHERDRIZZLYRAIN"].map(|x| x.to_owned().into());
        assert!(x < y);
    }
}

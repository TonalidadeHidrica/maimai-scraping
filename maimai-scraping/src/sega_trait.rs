use std::collections::BTreeMap;

use scraper::{Html, Selector};
use url::Url;

use crate::cookie_store::{AimeIdx, PlayerName};

pub type Idx<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::Idx;
pub type PlayTime<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::PlayTime;
pub type PlayedAt<T> = <<T as SegaTrait>::PlayRecord as PlayRecordTrait>::PlayedAt;
pub trait SegaTrait: Sized {
    const ERROR_PATH: &'static str;
    const AIME_SUBMIT_PATH: &'static str;
    const RECORD_URL: &'static str;

    type UserData: SegaUserData<Self>;

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

    fn parse_aime_selection_page(html: &Html) -> anyhow::Result<Vec<(AimeIdx, PlayerName)>>;

    const CREDENTIALS_PATH: &'static str;
    const COOKIE_STORE_PATH: &'static str;
}

pub type RecordMap<T> = BTreeMap<PlayTime<T>, <T as SegaTrait>::PlayRecord>;
pub trait SegaUserData<T: SegaTrait> {
    fn records_mut(&mut self) -> &mut RecordMap<T>;
}

pub trait PlayRecordTrait {
    type PlayedAt;
    fn played_at(&self) -> &Self::PlayedAt;
    type PlayTime;
    fn time(&self) -> Self::PlayTime;
    type Idx;
    fn idx(&self) -> Self::Idx;
}

pub mod record_map_serde {
    use serde::{de::SeqAccess, Deserialize};
    use std::marker::PhantomData;

    use serde::{de::Visitor, ser::SerializeSeq, Deserializer, Serialize, Serializer};

    use super::{PlayRecordTrait, PlayTime, RecordMap, SegaTrait};

    pub fn serialize<S, T>(map: &RecordMap<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: SegaTrait,
        T::PlayRecord: Serialize,
    {
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for record in map.values() {
            seq.serialize_element(record)?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<RecordMap<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: SegaTrait,
        T::PlayRecord: Deserialize<'de>,
        PlayTime<T>: Ord,
    {
        deserializer.deserialize_seq(MyVisitor::<T>(PhantomData))
    }

    struct MyVisitor<T>(PhantomData<fn() -> T>);
    impl<'de, T: SegaTrait> Visitor<'de> for MyVisitor<T>
    where
        T::PlayRecord: Deserialize<'de>,
        PlayTime<T>: Ord,
    {
        type Value = RecordMap<T>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a sequence of records")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut map = RecordMap::<T>::new();
            while let Some(elem) = seq.next_element::<T::PlayRecord>()? {
                map.insert(elem.time(), elem);
            }
            Ok(map)
        }
    }
}

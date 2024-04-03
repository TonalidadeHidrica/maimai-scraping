use anyhow::bail;
use hashbrown::HashMap;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use scraper::Html;
use serde::Serialize;
use typed_builder::TypedBuilder;
use url::Url;

use crate::{
    api::SegaClient,
    maimai::{parser::favorite_songs, Maimai},
};

use super::{
    parser::favorite_songs::{Idx, Page},
    schema::latest::SongName,
};

pub async fn fetch_favorite_songs_form(
    client: &mut SegaClient<'_, Maimai>,
) -> anyhow::Result<favorite_songs::Page> {
    // TODO: when supporting international ver., the domain should be updated
    favorite_songs::parse(&Html::parse_document(
        &client
            .fetch_authenticated(Url::parse(
                "https://maimaidx.jp/maimai-mobile/home/userOption/favorite/updateMusic",
            )?)
            .await?
            .0
            .text()
            .await?,
    ))
}

pub fn song_name_to_idx_map(page: &Page) -> HashMap<&SongName, Vec<&Idx>> {
    let mut ret = HashMap::<_, Vec<_>>::new();
    for genre in &page.genres {
        for song in &genre.songs {
            ret.entry(&song.name).or_default().push(&song.idx);
        }
    }
    ret
}

#[derive(Serialize, TypedBuilder)]
pub struct SetFavoriteSong<'a> {
    #[builder(default = 99)]
    idx: u8,
    #[serde(rename = "music[]")]
    music: Vec<&'a favorite_songs::Idx>,
    token: &'a favorite_songs::Token,
}
impl<'a> SetFavoriteSong<'a> {
    fn query_string(&self) -> anyhow::Result<String> {
        Ok(serde_html_form::to_string(self)?)
    }

    pub async fn send(&self, client: &mut SegaClient<'_, Maimai>) -> anyhow::Result<()> {
        let (_, location) =  client
            .request_authenticated(|client| {
                Ok(client
                    .post(Url::parse(
                        "https://maimaidx.jp/maimai-mobile/home/userOption/favorite/updateMusic/set",
                    )?)
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/x-www-form-urlencoded"),
                    )
                    .body(self.query_string()?))
            }, &format!("; _t={}", self.token))
            .await?;
        let expected_url =
            Url::parse("https://maimaidx.jp/maimai-mobile/home/userOption/favorite/musicList")
                .unwrap();
        if location != Some(expected_url) {
            bail!("Unexpected redirect to {location:?}");
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::SetFavoriteSong;

    #[test]
    fn test() {
        let token = "token".to_owned().into();
        let idx0 = "idx0".to_owned().into();
        let idx1 = "idx1".to_owned().into();
        let query = SetFavoriteSong::builder()
            .token(&token)
            .music(vec![&idx0, &idx1])
            .build();
        assert_eq!(
            &query.query_string().unwrap(),
            "idx=99&music%5B%5D=idx0&music%5B%5D=idx1&token=token"
        );
    }
}

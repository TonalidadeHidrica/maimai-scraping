use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use typed_builder::TypedBuilder;
use url::Url;

use crate::{
    api::SegaClient,
    maimai::{parser::favorite_songs, Maimai},
};

#[derive(Serialize, TypedBuilder)]
pub struct SetFavoriteSong {
    #[builder(default = 99)]
    idx: u8,
    #[serde(rename = "music[]")]
    music: Vec<favorite_songs::Idx>,
    token: favorite_songs::Token,
}
impl SetFavoriteSong {
    fn query_string(&self) -> anyhow::Result<String> {
        Ok(serde_html_form::to_string(self)?)
    }

    pub async fn send(&self, client: &mut SegaClient<'_, Maimai>) -> anyhow::Result<()> {
        client
            .request_authenticated(|client| {
                Ok(client
                    .post(
                        Url::parse(
                            "https://maimaidx.jp/maimai-mobile/home/userOption/favorite/updateMusic/set",
                        )?,
                    )
                    .header(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/x-www-form-urlencoded"),
                    )
                    .body(self.query_string()?))
            })
            .await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::SetFavoriteSong;

    #[test]
    fn test() {
        let query = SetFavoriteSong::builder()
            .token("token".to_owned().into())
            .music(vec!["idx0".to_owned().into(), "idx1".to_owned().into()])
            .build();
        assert_eq!(
            &query.query_string().unwrap(),
            "idx=99&music%5B%5D=idx0&music%5B%5D=idx1&token=token"
        );
    }
}

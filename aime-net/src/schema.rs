use std::{fmt::Display, str::FromStr};

use anyhow::{bail, Context};
use derive_more::{Display, From, FromStr};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AccessCode([u16; 5]);
impl TryFrom<[u16; 5]> for AccessCode {
    type Error = anyhow::Error;

    fn try_from(value: [u16; 5]) -> anyhow::Result<Self> {
        for (i, &value) in value.iter().enumerate() {
            #[allow(clippy::nonminimal_bool)]
            if !(value < 1_0000) {
                bail!("The {i}-th part out of range: {value}");
            }
        }
        Ok(Self(value))
    }
}
impl Display for AccessCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, value) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{value:04}")?;
        }
        Ok(())
    }
}
impl FromStr for AccessCode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        let mut chars = s.chars().filter(|&c| c != ' ');
        let mut res = [0; 5];
        for res in res.iter_mut() {
            for _ in 0..4 {
                *res = *res * 10
                    + chars
                        .next()
                        .with_context(|| format!("Not enough digits: {s:?}"))?
                        .to_digit(10)
                        .with_context(|| format!("Non-digit was found: {s:?}"))?
                        as u16;
            }
        }
        if chars.next().is_some() {
            bail!("Excessive character was found: {s:?}");
        }
        Ok(Self(res))
    }
}

#[derive(Clone, Debug, From, FromStr, Serialize, Deserialize)]
pub struct CardName(String);

#[derive(Debug, From, Serialize)]
pub struct BlockId(String);

#[derive(Debug, From, Serialize)]
pub struct VcToken(String);

#[derive(Clone, Copy, Debug, From, FromStr, Display, Serialize)]
pub struct SlotNo(usize);

#[cfg(test)]
mod tests {
    use crate::schema::AccessCode;

    #[test]
    fn parse_and_display_access_code() {
        let s = "0123 4567 8901 2345 6789";
        let t: AccessCode = s.parse().unwrap();
        assert_eq!(t, AccessCode([123, 4567, 8901, 2345, 6789]));
        assert_eq!(&t.to_string(), s);
    }
}

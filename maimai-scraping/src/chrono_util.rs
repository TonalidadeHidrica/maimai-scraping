use chrono::{FixedOffset, NaiveDateTime, Utc};

pub fn jst_now() -> NaiveDateTime {
    Utc::now()
        .with_timezone(&FixedOffset::east_opt(9 * 60 * 60).unwrap())
        .naive_local()
}

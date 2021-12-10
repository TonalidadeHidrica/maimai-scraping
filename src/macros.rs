#[macro_export]
macro_rules! selector {
    ($e: expr) => {{
        use ::once_cell::sync::Lazy;
        use ::scraper::Selector;
        static SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse($e).unwrap());
        &*SELECTOR
    }};
}

#[macro_export]
macro_rules! regex {
    ($e: expr) => {{
        use ::once_cell::sync::Lazy;
        use ::regex::Regex;
        static PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new($e).unwrap());
        &*PATTERN
    }};
}

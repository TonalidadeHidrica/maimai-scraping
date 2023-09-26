use std::path::PathBuf;

use actix_web::{
    get,
    http::header::{self, ContentType},
    middleware::Logger,
    web, App, HttpResponse, HttpServer,
};
use chrono::NaiveDateTime;
use clap::Parser;
use itertools::Itertools;
use maimai_scraping::{
    fs_json_util::read_json,
    maimai::{
        parser::rating_target::{RatingTargetEntry, RatingTargetFile},
        schema::latest::{PlayTime, ScoreDifficulty, ScoreGeneration},
        MaimaiUserData,
    },
};

#[derive(Parser)]
struct Opts {
    maimai_user_data_path: PathBuf,
    #[clap(default_value = "19405")]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let opts = Opts::parse();
    Ok(HttpServer::new(move || {
        let rating_targets = read_json::<_, MaimaiUserData>(&opts.maimai_user_data_path)
            .unwrap()
            .rating_targets;
        App::new()
            .app_data(web::Data::new(Data { rating_targets }))
            .service(get)
            .service(other_paths)
            .wrap(Logger::default())
    })
    .bind(("127.0.0.1", opts.port))?
    .run()
    .await?)
}

struct Data {
    rating_targets: RatingTargetFile,
}

#[get("/entry/{time}")]
async fn get(web_data: web::Data<Data>, play_time: web::Path<PlayTime>) -> HttpResponse {
    let Some(data) = web_data.rating_targets.get(&play_time) else {
        return HttpResponse::NotFound().body(format!("No data found: {play_time:?}"));
    };
    let make = |entry: &RatingTargetEntry| {
        let m = entry.score_metadata();
        use ScoreDifficulty::*;
        let difficulty = match m.difficulty() {
            Basic => "basic",
            Advanced => "advanced",
            Expert => "expert",
            Master => "master",
            ReMaster => "remaster",
            Utage => "utage",
        };
        use ScoreGeneration::*;
        let generation = match m.generation() {
            Standard => "standard",
            Deluxe => "dx",
        };
        format!(
            include_str!("entry_template.html"),
            generation = generation,
            difficulty = difficulty,
            level = entry.level(),
            name = entry.song_name(),
            achievement = entry.achievement(),
            idx = entry.idx(),
        )
    };
    let make = |entries: &[RatingTargetEntry]| entries.iter().map(make).join("");
    let choices = web_data
        .rating_targets
        .keys()
        .map(|&time| {
            format!(
                r#"<li><a href="/entry/{:?}">{time}</a></li>"#,
                NaiveDateTime::from(time),
            )
        })
        .join("");
    let html = format!(
        include_str!("rating_target_template.html"),
        date = play_time,
        rating = data.rating(),
        target_new = make(data.target_new()),
        target_old = make(data.target_old()),
        candidates_new = make(data.candidates_new()),
        candidates_old = make(data.candidates_old()),
        choices = choices,
    );
    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(html)
}

#[get("{_:.*}")]
async fn other_paths(web_data: web::Data<Data>) -> HttpResponse {
    match web_data.rating_targets.keys().last() {
        Some(latest) => HttpResponse::MovedPermanently()
            .insert_header((header::LOCATION, format!("/entry/{:?}", latest.get())))
            .body(()),
        None => HttpResponse::NotFound().body("No data yet"),
    }
}

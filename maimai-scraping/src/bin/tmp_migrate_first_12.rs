use std::path::PathBuf;

use clap::Parser;
use maimai_scraping::maimai::schema::latest::PlayRecord;
use maimai_scraping_utils::fs_json_util::{read_json, write_json};
use serde_json::Value;

#[derive(Parser)]
struct Opts {
    old_file: PathBuf,
    new_file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let new_records: Vec<PlayRecord> = read_json(&opts.new_file)?;
    assert_eq!(new_records.len(), 65);

    let mut value: Value = read_json(opts.old_file)?;

    let mut result = vec![];

    for (i, obj) in value
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
        .take(12)
    {
        let obj = obj.as_object_mut().unwrap();

        // insert idx
        let played_at = obj.get_mut("played_at").unwrap().as_object_mut().unwrap();
        played_at.insert("idx".into(), i.into());

        // modify perfect_challenge_result
        let perfect_challenge_result = obj.get("perfect_challenge_result").unwrap();
        assert!(perfect_challenge_result.is_null());
        obj.insert("life_result".into(), "Nothing".into());

        // remove grade_icon
        let rating_result = obj
            .get_mut("rating_result")
            .unwrap()
            .as_object_mut()
            .unwrap();
        rating_result
            .remove("grade_icon")
            .expect("grade_icon not found");

        let battle_result = obj.get_mut("battle_result").unwrap();
        match battle_result {
            Value::Null => {}
            Value::Object(battle_result) => {
                let battle_opponent = battle_result
                    .get_mut("opponent")
                    .unwrap()
                    .as_object_mut()
                    .unwrap();
                battle_opponent.remove("grade_icon");
            }
            _ => panic!("battle_result is wrong"),
        };

        let record: PlayRecord = serde_json::from_value(obj.to_owned().into()).unwrap();
        result.push(record);
    }

    result.extend(new_records);

    write_json(&opts.new_file, &result)?;

    Ok(())
}

//! The API client's cache rules: what gets stored, what gets refetched, and
//! what's served when the network is down.

mod common;

use std::collections::BTreeMap;
use std::fs;

use ffpr::sleeper::{week_has_scores, SleeperClient};
use serde_json::{json, Value};

use common::{serve_routes, LEAGUE_2025};

fn scored() -> Value {
    json!([{"roster_id": 1, "points": 101.5}, {"roster_id": 2, "points": 97.0}])
}

fn unplayed() -> Value {
    json!([{"roster_id": 1, "points": 0.0}, {"roster_id": 2, "points": 0.0}])
}

fn matchups_path(week: i64) -> String {
    format!("/v1/league/{LEAGUE_2025}/matchups/{week}")
}

#[test]
fn week_has_scores_tells_played_from_scheduled() {
    assert!(week_has_scores(&scored()));
    assert!(!week_has_scores(&unplayed()));
    assert!(!week_has_scores(&json!([])));
}

#[test]
fn a_completed_week_is_fetched_once_then_served_from_cache() {
    let api = serve_routes(BTreeMap::from([(matchups_path(3), scored())]));
    let dir = tempfile::tempdir().expect("temp dir");
    let client = SleeperClient::with_base_urls(dir.path(), &api.base, &api.schedule_base);

    let first = client
        .get_matchups(LEAGUE_2025, "2025", 3, true)
        .expect("fetch");
    let second = client
        .get_matchups(LEAGUE_2025, "2025", 3, true)
        .expect("cache");
    assert_eq!(first, second);
    assert_eq!(api.hits.lock().unwrap()[&matchups_path(3)], 1);
    assert!(dir.path().join("2025/raw/matchups_week3.json").exists());
}

#[test]
fn an_unplayed_week_is_returned_but_never_cached() {
    let api = serve_routes(BTreeMap::from([(matchups_path(7), unplayed())]));
    let dir = tempfile::tempdir().expect("temp dir");
    let client = SleeperClient::with_base_urls(dir.path(), &api.base, &api.schedule_base);

    assert_eq!(
        client.get_matchups(LEAGUE_2025, "2025", 7, false).unwrap(),
        unplayed()
    );
    assert_eq!(
        client.get_matchups(LEAGUE_2025, "2025", 7, false).unwrap(),
        unplayed()
    );
    assert_eq!(api.hits.lock().unwrap()[&matchups_path(7)], 2);
    assert!(!dir.path().join("2025/raw/matchups_week7.json").exists());
}

#[test]
fn a_zero_placeholder_in_the_cache_is_refetched_for_a_completed_week() {
    let api = serve_routes(BTreeMap::from([(matchups_path(4), scored())]));
    let dir = tempfile::tempdir().expect("temp dir");
    let raw = dir.path().join("2025/raw");
    fs::create_dir_all(&raw).unwrap();
    fs::write(raw.join("matchups_week4.json"), unplayed().to_string()).unwrap();
    let client = SleeperClient::with_base_urls(dir.path(), &api.base, &api.schedule_base);

    assert_eq!(
        client.get_matchups(LEAGUE_2025, "2025", 4, true).unwrap(),
        scored()
    );
    assert_eq!(api.hits.lock().unwrap()[&matchups_path(4)], 1);
    let cached: Value =
        serde_json::from_str(&fs::read_to_string(raw.join("matchups_week4.json")).unwrap())
            .unwrap();
    assert_eq!(cached, scored());
}

#[test]
fn the_in_progress_week_refreshes_a_stale_cache() {
    let api = serve_routes(BTreeMap::from([(matchups_path(6), scored())]));
    let dir = tempfile::tempdir().expect("temp dir");
    let raw = dir.path().join("2025/raw");
    fs::create_dir_all(&raw).unwrap();
    fs::write(
        raw.join("matchups_week6.json"),
        json!([{"roster_id": 9, "points": 1.0}]).to_string(),
    )
    .unwrap();
    let client = SleeperClient::with_base_urls(dir.path(), &api.base, &api.schedule_base);

    assert_eq!(
        client.get_matchups(LEAGUE_2025, "2025", 6, false).unwrap(),
        scored()
    );
}

#[test]
fn a_failed_fetch_falls_back_to_the_cached_copy() {
    let dir = tempfile::tempdir().expect("temp dir");
    let league_dir = dir.path().join("leagues");
    fs::create_dir_all(&league_dir).unwrap();
    fs::write(
        league_dir.join(format!("{LEAGUE_2025}.json")),
        json!({"name": "cached"}).to_string(),
    )
    .unwrap();
    // Nothing listens on port 9, so the request fails.
    let client =
        SleeperClient::with_base_urls(dir.path(), "http://127.0.0.1:9/v1", "http://127.0.0.1:9");
    let league = client.get_league(LEAGUE_2025).expect("served from cache");
    assert_eq!(league["name"], "cached");
}

#[test]
fn the_nfl_schedule_is_fetched_once() {
    let schedule = json!([{"week": 1, "home": "KC", "away": "BAL"}]);
    let url = "/schedule/nfl/regular/2025".to_string();
    let api = serve_routes(BTreeMap::from([(url.clone(), schedule.clone())]));
    let dir = tempfile::tempdir().expect("temp dir");
    let client = SleeperClient::with_base_urls(dir.path(), &api.base, &api.schedule_base);

    assert_eq!(client.get_nfl_schedule("2025").unwrap(), schedule);
    assert_eq!(client.get_nfl_schedule("2025").unwrap(), schedule);
    assert_eq!(api.hits.lock().unwrap()[&url], 1);
}

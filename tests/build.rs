//! Rendering: which pages a build writes, what's in them, and how text is
//! escaped, checked against a season built from the committed fixtures.

mod common;

use std::fs;
use std::path::Path;

use ffpr::build::{build_data_js, render_site, SiteOptions};
use ffpr::config::Settings;
use ffpr::models::{SeasonSummary, Weights};
use ffpr::pipeline;
use ffpr::sleeper::{ProxySettings, SleeperClient};
use serde_json::Value;

use common::{season_2025_routes, serve_routes, LEAGUE_2025};

const SITE_URL: &str = "https://example.test/league/";

fn settings() -> Settings {
    Settings {
        weights: Weights::default(),
        form_window: 3,
        site_url: SITE_URL.into(),
    }
}

/// The 2025 fixture season, built through the same pipeline the CLI uses.
fn fixture_season(data: &Path) -> SeasonSummary {
    let api = serve_routes(season_2025_routes());
    let client = SleeperClient::with_base_urls(
        data,
        &api.base,
        &api.schedule_base,
        ProxySettings::default(),
    );
    let mut seasons = pipeline::build_all(&client, LEAGUE_2025, None, None, false, &settings())
        .expect("fixture season builds");
    seasons.remove(0)
}

fn render(season: &SeasonSummary, out: &Path) {
    let all = vec![season.season.clone()];
    let rivalries = ffpr::compute::rivalries::build_rivalries(std::slice::from_ref(season));
    let opts = SiteOptions {
        site_url: SITE_URL,
        all_seasons: &all,
        site_root_prefix: "",
        rivalries: &rivalries,
    };
    render_site(season, out, &opts).expect("site renders");
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn payload(data_js: &str) -> Value {
    assert!(
        data_js.starts_with("window.FFPR = "),
        "data.js sets window.FFPR"
    );
    assert!(data_js.ends_with(";\n"));
    serde_json::from_str(&data_js["window.FFPR = ".len()..data_js.len() - 2])
        .expect("payload is JSON")
}

#[test]
fn a_completed_season_writes_every_page() {
    let data = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let season = fixture_season(data.path());
    render(&season, out.path());

    for page in [
        "index.html",
        "season.html",
        "rivalries.html",
        "rosters.html",
        "how-it-works.html",
        "weeks/week-5.html",
        "data.js",
        "rosters.json",
        "static/style.css",
        "static/app.js",
    ] {
        assert!(out.path().join(page).exists(), "{page} is written");
    }
    // The regular season isn't over, so there's no recap page yet.
    assert!(!out.path().join("weeks/week-15.html").exists());
    let week = read(&out.path().join("weeks/week-5.html"));
    assert!(week.contains("Week 5"));
    assert!(
        week.contains("href=\"../static/style.css?v="),
        "assets are relative"
    );
}

#[test]
fn data_js_is_a_pure_ascii_payload_with_the_week_series() {
    let data = tempfile::tempdir().unwrap();
    let season = fixture_season(data.path());
    let js = build_data_js(&season);
    assert!(
        js.is_ascii(),
        "non-ASCII is escaped the way Python's json.dumps does"
    );
    let parsed = payload(&js);
    assert_eq!(parsed["teams"].as_object().unwrap().len(), 12);
    // The fixture season is weeks 1-5 (the fixture week stands in for each).
    assert_eq!(
        parsed["rankTrajectory"]["weeks"],
        serde_json::json!([1, 2, 3, 4, 5])
    );
    assert_eq!(parsed["scoringSpread"].as_array().unwrap().len(), 5);
    assert!(
        parsed.get("trade").is_none(),
        "past seasons have no trade calculator"
    );
}

#[test]
fn a_season_with_no_completed_weeks_gets_the_landing_page() {
    let data = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let api = serve_routes(season_2025_routes());
    let client = SleeperClient::with_base_urls(
        data.path(),
        &api.base,
        &api.schedule_base,
        ProxySettings::default(),
    );
    let season = pipeline::build_season(&client, LEAGUE_2025, None, Some(0), false, &settings())
        .expect("season builds through week 0");
    render(&season, out.path());

    let index = read(&out.path().join("index.html"));
    assert!(index.contains("Week 1 rankings land Tuesday morning"));
    assert!(
        !index.contains("Pre-season power rankings"),
        "no preseason data, no table"
    );
    assert!(!out.path().join("season.html").exists());
}

#[test]
fn text_is_escaped_the_way_jinja2_escapes_it() {
    let data = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut season = fixture_season(data.path());
    season.teams.get_mut(&1).unwrap().name = "Ann & Bob's <Crew> \"x\"".into();
    render(&season, out.path());

    let week = read(&out.path().join("weeks/week-5.html"));
    assert!(week.contains("Ann &amp; Bob&#39;s &lt;Crew&gt; &#34;x&#34;"));
    assert!(
        !week.contains("Bob's"),
        "no raw apostrophe from a team name"
    );
}

#[test]
fn data_js_escapes_non_ascii_names_like_python() {
    let data = tempfile::tempdir().unwrap();
    let mut season = fixture_season(data.path());
    season.teams.get_mut(&2).unwrap().name = "Café ☕".into();
    let js = build_data_js(&season);
    // Python's json.dumps writes these characters as backslash-u escapes, so the
    // payload stays ASCII. The escapes are spelled out from a backslash on purpose.
    let backslash = '\\';
    let expected = format!("\"name\": \"Caf{backslash}u00e9 {backslash}u2615\"");
    assert!(js.contains(&expected), "{js:.200}");
}

#[test]
fn rosters_json_is_pretty_printed_with_a_trailing_newline() {
    let data = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    render(&fixture_season(data.path()), out.path());
    let text = read(&out.path().join("rosters.json"));
    assert!(text.starts_with("{\n  \"season\": \"2025\""));
    assert!(text.ends_with("}\n"));
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["teams"].as_array().unwrap().len(), 12);
}

#[test]
fn the_trade_worked_example_rounds_its_share_like_python() {
    // Four regular-season weeks and four playoff weeks weighted 1/3, with one
    // bye: Python's compensated sums give exactly 81.25%, shown as "81.2%".
    // Plain left-to-right sums land a hair above and print "81.3%".
    use ffpr::models::{PlayerScore, PlayerValue, TradeValues};
    use std::collections::BTreeMap;

    let data = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let mut season = fixture_season(data.path());
    let third = 1.0 / 3.0;
    let player = PlayerValue {
        player: PlayerScore {
            player_id: "p1".into(),
            name: "Example Receiver".into(),
            position: "WR".into(),
            nfl_team: Some("KC".into()),
            points: 20.0,
        },
        roster_id: Some(1),
        games: 4,
        ppg: 21.0,
        prior: 18.0,
        value: 11.85,
        injury: None,
        active: true,
        surplus: 14.58,
        availability: vec![1.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        projection: 20.0,
        weeks_available: 7.0,
    };
    season.trade_values = Some(TradeValues {
        players: BTreeMap::from([("p1".to_string(), player)]),
        replacement: BTreeMap::from([("WR".to_string(), 5.42)]),
        rosters: vec![(1, vec!["p1".to_string()])],
        strength: vec![],
        prior_games: 5,
        weeks_left: 4,
        roster_limit: 17,
        trade_deadline: None,
        horizon: vec![11, 12, 13, 14, 15, 16, 17, 18],
        weights: vec![1.0, 1.0, 1.0, 1.0, third, third, third, third],
        playoff_from: Some(15),
    });
    render(&season, out.path());

    let page = read(&out.path().join("how-it-works.html"));
    assert!(
        page.contains("7 of 8, or 81.2% once playoff weeks are weighted"),
        "{page:.0}"
    );
    assert!(!page.contains("81.3%"));
}

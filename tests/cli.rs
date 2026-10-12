//! The `ffpr` binary, run end to end against the fixture-backed API.

mod common;

use std::fs;
use std::process::Command;

use common::{fixture, season_2025_routes, serve_routes, LEAGUE_2025};

const PROXY_VARS: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

fn ffpr(dir: &std::path::Path, base: &common::MockApi) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ffpr"));
    cmd.current_dir(dir)
        .env("SLEEPER_API_BASE", &base.base)
        .env("SLEEPER_SCHEDULE_BASE", &base.schedule_base)
        .env_remove("SLEEPER_LEAGUE_ID");
    // Talk to the local stand-in directly, whatever proxy the machine uses.
    for var in PROXY_VARS {
        cmd.env_remove(var);
    }
    cmd
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn build_writes_the_site_from_the_api() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--league", LEAGUE_2025])
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Built site/ through week 5"
    );
    let site = dir.path().join("site");
    assert!(site.join("index.html").exists());
    assert!(site.join("weeks/week-5.html").exists());
    assert!(site.join(LEAGUE_2025).exists() || site.join("2025/index.html").exists());
    assert!(dir
        .path()
        .join("data/2025/raw/matchups_week5.json")
        .exists());
}

#[test]
fn blurb_prints_a_chat_ready_recap() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        "site_url = \"https://example.test/league/\"\n",
    )
    .unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["blurb", "--league", LEAGUE_2025])
        .output()
        .expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        text.starts_with("League of Fantasy Fanatics(LAFF) -- Week 5 recap"),
        "{text}"
    );
    assert!(text.contains("Top score: Taylor-Made Touchdowns (187.99)"));
    assert!(text.contains("Power rankings top 3:"));
    assert!(text.trim_end().ends_with("https://example.test/league/"));
}

#[test]
fn build_without_a_league_id_says_how_to_set_one() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api).arg("build").output().expect("runs");
    assert_eq!(output.status.code(), Some(2), "a usage error");
    assert!(stderr(&output).contains("No league id"));
}

#[test]
fn a_season_that_doesnt_match_the_league_is_rejected() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--league", LEAGUE_2025, "--season", "2024"])
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(2), "a usage error");
    assert!(stderr(&output).contains("doesn't match"));
}

#[test]
fn empty_overrides_fall_back_like_unset_ones() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        format!("league_id = \"{LEAGUE_2025}\"\n"),
    )
    .unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--league", "", "--season", ""])
        .output()
        .expect("runs");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(dir.path().join("site/index.html").exists());
}

#[test]
fn a_bare_integer_league_id_in_config_toml_works() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("config.toml"),
        format!("league_id = {LEAGUE_2025}\n"),
    )
    .unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--no-provisional"])
        .output()
        .expect("runs");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "Built site/ through week 5"
    );
}

#[test]
fn requests_go_through_the_proxy_in_the_environment() {
    // The API host can't be resolved; only the proxy (the local stand-in,
    // which answers absolute-form requests) can reach it.
    let api = serve_routes(season_2025_routes());
    let proxy = api.base.trim_end_matches("/v1").to_string();
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api)
        .env("SLEEPER_API_BASE", "http://sleeper.invalid/v1")
        .env(
            "SLEEPER_SCHEDULE_BASE",
            "http://sleeper.invalid/schedule/nfl/regular",
        )
        .env("HTTP_PROXY", &proxy)
        .args(["build", "--league", LEAGUE_2025])
        .output()
        .expect("runs");
    assert!(output.status.success(), "{}", stderr(&output));

    // NO_PROXY sends it direct again, and the unresolvable host fails.
    let output = ffpr(dir.path(), &api)
        .env("SLEEPER_API_BASE", "http://sleeper.invalid/v1")
        .env("HTTP_PROXY", &proxy)
        .env("NO_PROXY", "sleeper.invalid")
        .args(["build", "--league", LEAGUE_2025])
        .output()
        .expect("runs");
    assert!(!output.status.success());
}

#[test]
fn a_previous_season_whose_data_does_not_parse_fails_the_build_loudly() {
    // Python only skips a past season it can't fetch; data it can't use is an
    // error there, so it must not silently vanish from the site here.
    let mut routes = season_2025_routes();
    let previous = "1124855076452585472";
    let mut league = fixture("league.json");
    league["league_id"] = previous.into();
    league["season"] = "2024".into();
    league["previous_league_id"] = serde_json::Value::Null;
    routes.insert(format!("/v1/league/{previous}"), league);
    routes.insert(
        format!("/v1/league/{previous}/rosters"),
        serde_json::json!({"not": "a list"}),
    );
    let api = serve_routes(routes);
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--league", LEAGUE_2025])
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains(previous), "{}", stderr(&output));
}

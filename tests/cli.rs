//! The `ffpr` binary, run end to end against the fixture-backed API.

mod common;

use std::fs;
use std::process::Command;

use common::{season_2025_routes, serve_routes, LEAGUE_2025};

fn ffpr(dir: &std::path::Path, base: &common::MockApi) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ffpr"));
    cmd.current_dir(dir)
        .env("SLEEPER_API_BASE", &base.base)
        .env("SLEEPER_SCHEDULE_BASE", &base.schedule_base)
        .env_remove("SLEEPER_LEAGUE_ID");
    cmd
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
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("No league id"));
}

#[test]
fn a_season_that_doesnt_match_the_league_is_rejected() {
    let api = serve_routes(season_2025_routes());
    let dir = tempfile::tempdir().unwrap();
    let output = ffpr(dir.path(), &api)
        .args(["build", "--league", LEAGUE_2025, "--season", "2024"])
        .output()
        .expect("runs");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("doesn't match"));
}

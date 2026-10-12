//! Helpers shared by the integration tests: committed fixtures, and a
//! fixture-backed stand-in for the Sleeper API.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub const LEAGUE_2025: &str = "1252764079794159616";

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

pub fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(fixture_path(name)).expect("fixture exists");
    serde_json::from_str(&text).expect("fixture is JSON")
}

pub fn typed<T: DeserializeOwned>(name: &str) -> T {
    serde_json::from_value(fixture(name)).expect("fixture matches the typed view")
}

/// A running fixture-backed API: its base URL, and how many times each path was requested.
pub struct MockApi {
    pub base: String,
    pub schedule_base: String,
    pub hits: Arc<Mutex<BTreeMap<String, usize>>>,
}

/// Serve `routes` (path -> JSON body) on a random local port until the test ends.
/// Unknown paths get a 404, the way Sleeper answers a league it doesn't have.
pub fn serve_routes(routes: BTreeMap<String, Value>) -> MockApi {
    let server = tiny_http::Server::http("127.0.0.1:0").expect("bind a local port");
    let port = server.server_addr().to_ip().expect("ip address").port();
    let hits = Arc::new(Mutex::new(BTreeMap::new()));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            // Through a proxy the request line carries the whole URL
            // ("GET http://host/v1/..."); route on its path either way.
            let target = request.url();
            let target = match target.split_once("://") {
                Some((_, rest)) => rest.find('/').map_or("/", |i| &rest[i..]),
                None => target,
            };
            let path = target.split('?').next().unwrap_or("").to_string();
            *counter
                .lock()
                .expect("hit counter")
                .entry(path.clone())
                .or_insert(0) += 1;
            let response = match routes.get(&path) {
                Some(body) => tiny_http::Response::from_string(body.to_string()).boxed(),
                None => tiny_http::Response::from_string("not found")
                    .with_status_code(404)
                    .boxed(),
            };
            let _ = request.respond(response);
        }
    });
    let base = format!("http://127.0.0.1:{port}/v1");
    MockApi {
        schedule_base: format!("http://127.0.0.1:{port}/schedule/nfl/regular"),
        base,
        hits,
    }
}

/// Everything a 2025 (past season) build reads, from the committed fixtures.
/// Weeks 1-5 are the fixture week; later weeks are empty, as an unplayed
/// schedule would be to the build. The 2025 league has no draft and no
/// previous league here, which the build treats as "not available".
pub fn season_2025_routes() -> BTreeMap<String, Value> {
    let league = fixture("league.json");
    let matchups = fixture("matchups_week5.json");
    let transactions = fixture("transactions_week5.json");
    let mut routes = BTreeMap::new();
    routes.insert(
        "/v1/state/nfl".to_string(),
        json!({"week": 1, "season": "2026"}),
    );
    routes.insert(format!("/v1/league/{LEAGUE_2025}"), league);
    routes.insert(
        format!("/v1/league/{LEAGUE_2025}/rosters"),
        fixture("rosters.json"),
    );
    routes.insert(
        format!("/v1/league/{LEAGUE_2025}/users"),
        fixture("users.json"),
    );
    routes.insert(
        "/v1/players/nfl".to_string(),
        fixture("players_subset.json"),
    );
    for week in 1..=18 {
        let scored = if week <= 5 {
            matchups.clone()
        } else {
            json!([])
        };
        routes.insert(format!("/v1/league/{LEAGUE_2025}/matchups/{week}"), scored);
        let moves = if week == 5 {
            transactions.clone()
        } else {
            json!([])
        };
        routes.insert(
            format!("/v1/league/{LEAGUE_2025}/transactions/{week}"),
            moves,
        );
    }
    routes
}

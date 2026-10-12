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

/// Like [`serve_routes`], but over HTTPS with a certificate for 127.0.0.1
/// signed by the throwaway test CA in `tests/fixtures/tls/ca.pem`. Each
/// connection carries one request and is then closed.
pub fn serve_routes_tls(routes: BTreeMap<String, Value>) -> MockApi {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    use std::io::{Read, Write};

    let chain = CertificateDer::pem_file_iter(fixture_path("tls/server.pem"))
        .and_then(|certs| certs.collect::<Result<Vec<_>, _>>())
        .expect("test certificate");
    let key = PrivateKeyDer::from_pem_file(fixture_path("tls/server.key")).expect("test key");
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .expect("server TLS config"),
    );
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a local port");
    let port = listener.local_addr().expect("local address").port();
    let routes = Arc::new(routes);
    let hits = Arc::new(Mutex::new(BTreeMap::new()));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (config, routes, counter) = (
                Arc::clone(&config),
                Arc::clone(&routes),
                Arc::clone(&counter),
            );
            std::thread::spawn(move || {
                let Ok(conn) = rustls::ServerConnection::new(config) else {
                    return;
                };
                let mut tls = rustls::StreamOwned::new(conn, stream);
                // Read the request head; a client that rejects the
                // certificate ends the handshake here instead.
                let mut head = Vec::new();
                let mut buf = [0u8; 4096];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&head);
                let target = head.split_whitespace().nth(1).unwrap_or("/");
                let path = target.split('?').next().unwrap_or("").to_string();
                *counter
                    .lock()
                    .expect("hit counter")
                    .entry(path.clone())
                    .or_insert(0) += 1;
                let (status, body) = match routes.get(&path) {
                    Some(body) => ("200 OK", body.to_string()),
                    None => ("404 Not Found", "not found".to_string()),
                };
                let _ = write!(
                    tls,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                tls.conn.send_close_notify();
                let _ = tls.flush();
            });
        }
    });
    MockApi {
        base: format!("https://127.0.0.1:{port}/v1"),
        schedule_base: format!("https://127.0.0.1:{port}/schedule/nfl/regular"),
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

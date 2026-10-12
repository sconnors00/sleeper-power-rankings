//! Read-only Sleeper API client with an on-disk cache under `data/`.
//!
//! Completed-week responses are cached and reused; the in-progress week and
//! league/roster/user data are always refetched, falling back to a cached copy
//! when the network is unavailable (so offline builds still work). Responses
//! are cached as the raw JSON Sleeper sent.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use serde_json::Value;

pub const DEFAULT_BASE_URL: &str = "https://api.sleeper.app/v1";
/// Not part of the documented v1 API, but what Sleeper's own app reads the NFL
/// schedule from. Used only for bye weeks, so a failure costs nothing but those.
pub const DEFAULT_SCHEDULE_BASE: &str = "https://api.sleeper.com/schedule/nfl/regular";
const PLAYERS_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Connect, and each read or write, must make progress within this long. As
/// with httpx, it is not a cap on the whole download, which for the ~13 MB
/// player map can take longer on a slow link.
const IO_TIMEOUT: Duration = Duration::from_secs(15);
/// The largest response body read. /players/nfl is well over 10 MB; this
/// leaves plenty of room while still bounding a runaway response.
const MAX_RESPONSE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SleeperError(pub String);

impl std::fmt::Display for SleeperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SleeperError {}

pub type Result<T> = std::result::Result<T, SleeperError>;

/// True once any roster has scored in a week.
///
/// Sleeper serves a week's matchups as soon as the schedule exists, long before
/// kickoff, and every roster reads 0.0 until games are played. Such a response
/// is a placeholder, not a result, so it must never be cached as settled data.
pub fn week_has_scores(raw: &Value) -> bool {
    raw.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|m| m.get("points").and_then(Value::as_f64).unwrap_or(0.0) > 0.0)
    })
}

pub struct SleeperClient {
    data_dir: PathBuf,
    proxies: ProxySettings,
    /// One HTTP agent per proxy ("" for direct), so connections are reused.
    agents: Mutex<HashMap<String, ureq::Agent>>,
    base_url: String,
    schedule_base: String,
}

/// Proxy settings in the conventional environment variables, applied the way
/// httpx (and curl) apply them: `HTTPS_PROXY` for https URLs, `HTTP_PROXY` for
/// http ones, `ALL_PROXY` for either, and `NO_PROXY` hosts connect directly.
#[derive(Debug, Clone, Default)]
pub struct ProxySettings {
    http: Option<String>,
    https: Option<String>,
    all: Option<String>,
    no_proxy: Vec<String>,
}

impl ProxySettings {
    pub fn from_env() -> Self {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// Settings from any variable lookup (upper-case names win over lower-case).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let pick = |names: [&str; 2]| {
            names.iter().find_map(|name| {
                get(name)
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
            })
        };
        ProxySettings {
            http: pick(["HTTP_PROXY", "http_proxy"]),
            https: pick(["HTTPS_PROXY", "https_proxy"]),
            all: pick(["ALL_PROXY", "all_proxy"]),
            no_proxy: pick(["NO_PROXY", "no_proxy"])
                .map(|list| {
                    list.split(',')
                        .map(|entry| entry.trim().to_ascii_lowercase())
                        .filter(|entry| !entry.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The proxy a request to `url` goes through, or `None` to connect directly.
    pub fn proxy_for(&self, url: &str) -> Option<&str> {
        let (scheme, rest) = url.split_once("://")?;
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        let host_port = authority
            .rsplit('@')
            .next()
            .unwrap_or(authority)
            .to_ascii_lowercase();
        let host = match host_port.strip_prefix('[') {
            Some(v6) => v6.split(']').next().unwrap_or(v6).to_string(),
            None => host_port.split(':').next().unwrap_or("").to_string(),
        };
        if self.bypasses(&host, &host_port) {
            return None;
        }
        let specific = if scheme.eq_ignore_ascii_case("https") {
            self.https.as_deref()
        } else {
            self.http.as_deref()
        };
        specific.or(self.all.as_deref())
    }

    fn bypasses(&self, host: &str, host_port: &str) -> bool {
        self.no_proxy.iter().any(|entry| {
            if entry == "*" {
                return true;
            }
            let entry = entry.trim_start_matches("*.").trim_start_matches('.');
            let target = if entry.contains(':') && !entry.starts_with('[') {
                host_port
            } else {
                host
            };
            target == entry || target.ends_with(&format!(".{entry}"))
        })
    }
}

type Settled<'a> = Option<&'a dyn Fn(&Value) -> bool>;

impl SleeperClient {
    /// A client for the real API, using the proxy settings in the environment.
    /// `SLEEPER_API_BASE` and `SLEEPER_SCHEDULE_BASE` override the two base
    /// URLs, for pointing a build at a mock server.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let base_url =
            std::env::var("SLEEPER_API_BASE").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        let schedule_base =
            std::env::var("SLEEPER_SCHEDULE_BASE").unwrap_or_else(|_| DEFAULT_SCHEDULE_BASE.into());
        Self::with_base_urls(data_dir, base_url, schedule_base, ProxySettings::from_env())
    }

    /// A client that reads from the given base URLs (the API root, and the NFL
    /// schedule root that `/{season}` is appended to).
    pub fn with_base_urls(
        data_dir: impl Into<PathBuf>,
        base_url: impl Into<String>,
        schedule_base: impl Into<String>,
        proxies: ProxySettings,
    ) -> Self {
        SleeperClient {
            data_dir: data_dir.into(),
            proxies,
            agents: Mutex::new(HashMap::new()),
            base_url: base_url.into(),
            schedule_base: schedule_base.into(),
        }
    }

    fn agent_for(&self, url: &str) -> ureq::Agent {
        let proxy = self.proxies.proxy_for(url).unwrap_or("").to_string();
        let mut agents = self.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents
            .entry(proxy.clone())
            .or_insert_with(|| {
                let mut builder = ureq::AgentBuilder::new()
                    .timeout_connect(IO_TIMEOUT)
                    .timeout_read(IO_TIMEOUT)
                    .timeout_write(IO_TIMEOUT);
                // An unparseable proxy URL connects directly rather than failing.
                if let Some(p) = Some(&proxy)
                    .filter(|p| !p.is_empty())
                    .and_then(|p| ureq::Proxy::new(p).ok())
                {
                    builder = builder.proxy(p);
                }
                builder.build()
            })
            .clone()
    }

    fn raw_dir(&self, season: &str) -> Result<PathBuf> {
        let dir = self.data_dir.join(season).join("raw");
        create_dir(&dir)?;
        Ok(dir)
    }

    fn subdir(&self, name: &str) -> Result<PathBuf> {
        let dir = self.data_dir.join(name);
        create_dir(&dir)?;
        Ok(dir)
    }

    fn fetch_url(&self, url: &str) -> Result<Value> {
        let response = self
            .agent_for(url)
            .get(url)
            .call()
            .map_err(|e| SleeperError(format!("GET {url} failed: {e}")))?;
        // Read the body ourselves: ureq's into_string() refuses anything over
        // 10 MB, and /players/nfl is bigger than that.
        let mut body = Vec::new();
        response
            .into_reader()
            .take(MAX_RESPONSE_BYTES)
            .read_to_end(&mut body)
            .map_err(|e| SleeperError(format!("GET {url} unreadable: {e}")))?;
        serde_json::from_slice(&body).map_err(|e| SleeperError(format!("GET {url} bad JSON: {e}")))
    }

    fn fetch(&self, path: &str) -> Result<Value> {
        self.fetch_url(&format!("{}{path}", self.base_url))
    }

    /// Always try the network; fall back to a cached copy if it fails. A
    /// response `settled` rejects is returned but not cached.
    fn fetch_with_fallback_cache(
        &self,
        path: &str,
        cache_file: &Path,
        settled: Settled,
    ) -> Result<Value> {
        self.fetch_with_fallback_cache_url(&format!("{}{path}", self.base_url), cache_file, settled)
    }

    fn fetch_with_fallback_cache_url(
        &self,
        url: &str,
        cache_file: &Path,
        settled: Settled,
    ) -> Result<Value> {
        match self.fetch_url(url) {
            Ok(data) => {
                if settled.is_none_or(|ok| ok(&data)) {
                    write_json(cache_file, &data)?;
                }
                Ok(data)
            }
            Err(err) => {
                if cache_file.exists() {
                    read_json(cache_file)
                } else {
                    Err(SleeperError(format!("{err} and no cache available")))
                }
            }
        }
    }

    /// Use the cache if present and settled; otherwise fetch and cache.
    fn fetch_cache_first(&self, path: &str, cache_file: &Path, settled: Settled) -> Result<Value> {
        self.cache_first_url(&format!("{}{path}", self.base_url), cache_file, settled)
    }

    fn cache_first_url(&self, url: &str, cache_file: &Path, settled: Settled) -> Result<Value> {
        if cache_file.exists() {
            let cached = read_json(cache_file)?;
            if settled.is_none_or(|ok| ok(&cached)) {
                return Ok(cached);
            }
        }
        self.fetch_with_fallback_cache_url(url, cache_file, settled)
    }

    pub fn get_state(&self) -> Result<Value> {
        self.fetch("/state/nfl")
    }

    /// Cached by league id, not season: the season isn't known until this returns.
    pub fn get_league(&self, league_id: &str) -> Result<Value> {
        let cache = self.subdir("leagues")?.join(format!("{league_id}.json"));
        self.fetch_with_fallback_cache(&format!("/league/{league_id}"), &cache, None)
    }

    pub fn get_rosters(&self, league_id: &str, season: &str) -> Result<Value> {
        let cache = self.raw_dir(season)?.join("rosters.json");
        self.fetch_with_fallback_cache(&format!("/league/{league_id}/rosters"), &cache, None)
    }

    pub fn get_users(&self, league_id: &str, season: &str) -> Result<Value> {
        let cache = self.raw_dir(season)?.join("users.json");
        self.fetch_with_fallback_cache(&format!("/league/{league_id}/users"), &cache, None)
    }

    pub fn get_matchups(
        &self,
        league_id: &str,
        season: &str,
        week: i64,
        completed: bool,
    ) -> Result<Value> {
        let cache = self
            .raw_dir(season)?
            .join(format!("matchups_week{week}.json"));
        let path = format!("/league/{league_id}/matchups/{week}");
        if completed {
            self.fetch_cache_first(&path, &cache, Some(&week_has_scores))
        } else {
            self.fetch_with_fallback_cache(&path, &cache, Some(&week_has_scores))
        }
    }

    pub fn get_transactions(
        &self,
        league_id: &str,
        season: &str,
        week: i64,
        completed: bool,
    ) -> Result<Value> {
        let cache = self
            .raw_dir(season)?
            .join(format!("transactions_week{week}.json"));
        let path = format!("/league/{league_id}/transactions/{week}");
        if completed {
            self.fetch_cache_first(&path, &cache, None)
        } else {
            self.fetch_with_fallback_cache(&path, &cache, None)
        }
    }

    pub fn get_draft(&self, draft_id: &str) -> Result<Value> {
        let cache = self.subdir("drafts")?.join(format!("{draft_id}.json"));
        self.fetch_with_fallback_cache(&format!("/draft/{draft_id}"), &cache, None)
    }

    pub fn get_draft_picks(&self, draft_id: &str, completed: bool) -> Result<Value> {
        let cache = self
            .subdir("drafts")?
            .join(format!("{draft_id}_picks.json"));
        let path = format!("/draft/{draft_id}/picks");
        if completed {
            self.fetch_cache_first(&path, &cache, None)
        } else {
            self.fetch_with_fallback_cache(&path, &cache, None)
        }
    }

    /// Every regular-season NFL game: `[{"week": 1, "home": "KC", "away": "BAL", ...}]`.
    pub fn get_nfl_schedule(&self, season: &str) -> Result<Value> {
        let cache = self.raw_dir(season)?.join("nfl_schedule.json");
        let url = format!("{}/{season}", self.schedule_base);
        let non_empty = |data: &Value| data.as_array().is_some_and(|a| !a.is_empty());
        self.cache_first_url(&url, &cache, Some(&non_empty))
    }

    /// `/players/nfl` (about 5 MB), cached and refreshed at most once a day.
    pub fn get_players(&self) -> Result<Value> {
        let cache = self.data_dir.join("players_nfl.json");
        if let Ok(meta) = fs::metadata(&cache) {
            // A modification time in the future (clock skew, a cache restored
            // from another machine) counts as fresh, as it does in Python.
            let fresh = meta.modified().ok().is_some_and(|modified| {
                SystemTime::now()
                    .duration_since(modified)
                    .map_or(true, |age| age < PLAYERS_MAX_AGE)
            });
            if fresh {
                return read_json(&cache);
            }
        }
        match self.fetch("/players/nfl") {
            Ok(data) => {
                write_json(&cache, &data)?;
                Ok(data)
            }
            Err(err) if cache.exists() => read_json(&cache).map_err(|_| err),
            Err(err) => Err(SleeperError(format!("{err} and no cache available"))),
        }
    }
}

fn create_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)
        .map_err(|e| SleeperError(format!("cannot create {}: {e}", dir.display())))
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path)
        .map_err(|e| SleeperError(format!("cannot read {}: {e}", path.display())))?;
    serde_json::from_str(&text)
        .map_err(|e| SleeperError(format!("bad cache {}: {e}", path.display())))
}

fn write_json(path: &Path, data: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let text = serde_json::to_string(data).map_err(|e| SleeperError(e.to_string()))?;
    fs::write(path, text).map_err(|e| SleeperError(format!("cannot write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(vars: &[(&str, &str)]) -> ProxySettings {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ProxySettings::from_lookup(|name| vars.get(name).cloned())
    }

    #[test]
    fn the_proxy_follows_the_url_scheme() {
        let p = settings(&[
            ("HTTPS_PROXY", "http://tls-proxy:3128"),
            ("http_proxy", "http://plain:3128"),
        ]);
        assert_eq!(
            p.proxy_for("https://api.sleeper.app/v1/state/nfl"),
            Some("http://tls-proxy:3128")
        );
        assert_eq!(
            p.proxy_for("http://mirror.local/v1"),
            Some("http://plain:3128")
        );
    }

    #[test]
    fn all_proxy_is_the_fallback_and_nothing_set_means_direct() {
        let p = settings(&[("ALL_PROXY", "http://everything:8080")]);
        assert_eq!(
            p.proxy_for("https://api.sleeper.app/v1"),
            Some("http://everything:8080")
        );
        assert_eq!(settings(&[]).proxy_for("https://api.sleeper.app/v1"), None);
    }

    #[test]
    fn no_proxy_hosts_and_their_subdomains_connect_directly() {
        let p = settings(&[
            ("HTTPS_PROXY", "http://proxy:3128"),
            ("HTTP_PROXY", "http://proxy:3128"),
            (
                "NO_PROXY",
                "localhost, .internal ,127.0.0.1,mirror.test:8080",
            ),
        ]);
        assert_eq!(p.proxy_for("http://localhost:8765/v1"), None);
        assert_eq!(p.proxy_for("http://127.0.0.1:8765/v1"), None);
        assert_eq!(p.proxy_for("https://api.internal/v1"), None);
        assert_eq!(p.proxy_for("http://mirror.test:8080/v1"), None);
        assert_eq!(
            p.proxy_for("http://mirror.test:9090/v1"),
            Some("http://proxy:3128")
        );
        assert_eq!(
            p.proxy_for("https://notinternal.example/v1"),
            Some("http://proxy:3128")
        );
        assert_eq!(
            settings(&[("HTTP_PROXY", "http://p:1"), ("NO_PROXY", "*")]).proxy_for("http://x/v1"),
            None
        );
    }
}

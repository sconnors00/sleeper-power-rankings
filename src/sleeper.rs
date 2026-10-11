//! Read-only Sleeper API client with an on-disk cache under `data/`.
//!
//! Completed-week responses are cached and reused; the in-progress week and
//! league/roster/user data are always refetched, falling back to a cached copy
//! when the network is unavailable (so offline builds still work). Responses
//! are cached as the raw JSON Sleeper sent.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

pub const DEFAULT_BASE_URL: &str = "https://api.sleeper.app/v1";
/// Not part of the documented v1 API, but what Sleeper's own app reads the NFL
/// schedule from. Used only for bye weeks, so a failure costs nothing but those.
pub const DEFAULT_SCHEDULE_BASE: &str = "https://api.sleeper.com/schedule/nfl/regular";
const PLAYERS_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

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
    agent: ureq::Agent,
    base_url: String,
    schedule_base: String,
}

type Settled<'a> = Option<&'a dyn Fn(&Value) -> bool>;

impl SleeperClient {
    /// A client for the real API. `SLEEPER_API_BASE` and `SLEEPER_SCHEDULE_BASE`
    /// override the two base URLs, for pointing a build at a mock server.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        let base_url =
            std::env::var("SLEEPER_API_BASE").unwrap_or_else(|_| DEFAULT_BASE_URL.into());
        let schedule_base =
            std::env::var("SLEEPER_SCHEDULE_BASE").unwrap_or_else(|_| DEFAULT_SCHEDULE_BASE.into());
        Self::with_base_urls(data_dir, base_url, schedule_base)
    }

    /// A client that reads from the given base URLs (the API root, and the NFL
    /// schedule root that `/{season}` is appended to).
    pub fn with_base_urls(
        data_dir: impl Into<PathBuf>,
        base_url: impl Into<String>,
        schedule_base: impl Into<String>,
    ) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build();
        SleeperClient {
            data_dir: data_dir.into(),
            agent,
            base_url: base_url.into(),
            schedule_base: schedule_base.into(),
        }
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
            .agent
            .get(url)
            .call()
            .map_err(|e| SleeperError(format!("GET {url} failed: {e}")))?;
        let text = response
            .into_string()
            .map_err(|e| SleeperError(format!("GET {url} unreadable: {e}")))?;
        serde_json::from_str(&text).map_err(|e| SleeperError(format!("GET {url} bad JSON: {e}")))
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
            let fresh = meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|age| age < PLAYERS_MAX_AGE);
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

//! `config.toml`: the league id, the power-ranking weights and form window,
//! and the site URL used in `blurb` output.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};

use crate::compute::DEFAULT_FORM_WINDOW;
use crate::models::Weights;

pub const CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    /// A string, or a bare integer (TOML allows either for a numeric id).
    #[serde(deserialize_with = "string_or_integer")]
    pub league_id: Option<String>,
    pub site_url: Option<String>,
    pub weights: PartialWeights,
    pub form: FormConfig,
}

fn string_or_integer<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Number(i64),
    }
    Ok(Option::<Id>::deserialize(d)?.map(|id| match id {
        Id::Text(text) => text,
        Id::Number(n) => n.to_string(),
    }))
}

/// A mistake in how ffpr was invoked (a missing league id, a season that
/// doesn't match the league, `serve` with nothing built). The command line
/// exits with status 2 for these, as Click does for bad parameters.
#[derive(Debug)]
pub struct UsageError(pub String);

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// Weights as written in the file. A missing key keeps its default.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PartialWeights {
    pub win_pct: Option<f64>,
    pub allplay_pct: Option<f64>,
    pub pf_norm: Option<f64>,
    pub form_norm: Option<f64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct FormConfig {
    pub window: Option<i64>,
}

/// Everything the pipeline needs from configuration, with defaults applied.
#[derive(Debug, Clone)]
pub struct Settings {
    pub weights: Weights,
    pub form_window: i64,
    pub site_url: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
    }

    pub fn settings(&self) -> Settings {
        let defaults = Weights::default();
        Settings {
            weights: Weights {
                win_pct: self.weights.win_pct.unwrap_or(defaults.win_pct),
                allplay_pct: self.weights.allplay_pct.unwrap_or(defaults.allplay_pct),
                pf_norm: self.weights.pf_norm.unwrap_or(defaults.pf_norm),
                form_norm: self.weights.form_norm.unwrap_or(defaults.form_norm),
            },
            form_window: self.form.window.unwrap_or(DEFAULT_FORM_WINDOW),
            site_url: self.site_url.clone().unwrap_or_default(),
        }
    }
}

/// `--league` > `SLEEPER_LEAGUE_ID` > `config.toml`.
pub fn resolve_league_id(league_override: Option<&str>, config: &Config) -> Result<String> {
    let from_env = std::env::var("SLEEPER_LEAGUE_ID")
        .ok()
        .filter(|s| !s.is_empty());
    // An empty value counts as unset at every level, like Python's `or`.
    league_override
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or(from_env)
        .or_else(|| config.league_id.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| {
            UsageError(
                "No league id. Pass --league, set SLEEPER_LEAGUE_ID, or set league_id in config.toml."
                    .to_string(),
            )
            .into()
        })
}

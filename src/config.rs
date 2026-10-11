//! `config.toml`: the league id, the power-ranking weights and form window,
//! and the site URL used in `blurb` output.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::compute::DEFAULT_FORM_WINDOW;
use crate::models::Weights;

pub const CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub league_id: Option<String>,
    pub site_url: Option<String>,
    pub weights: PartialWeights,
    pub form: FormConfig,
}

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
    league_override
        .map(str::to_string)
        .or(from_env)
        .or_else(|| config.league_id.clone().filter(|s| !s.is_empty()))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No league id. Pass --league, set SLEEPER_LEAGUE_ID, or set league_id in config.toml."
            )
        })
}

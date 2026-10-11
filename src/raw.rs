//! Typed views of the Sleeper API responses the pipeline reads. Every field
//! that Sleeper sometimes sends as `null` (or omits) is tolerant, the same way
//! the Python code's `.get(key) or default` was.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A JSON array or object that may be `null`, read as its default when it is.
fn nullable<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// A JSON number (or numeric string, Sleeper sends auction amounts as strings)
/// as an integer, truncating like Python's `int()`. Missing or null is 0.
pub fn int_of(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        Some(Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .or_else(|_| s.trim().parse::<f64>().map(|f| f as i64))
            .unwrap_or(0),
        _ => 0,
    }
}

/// `str(x)` for the values Sleeper sends where a string or number may appear.
pub fn text_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct State {
    pub week: i64,
    pub season: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct League {
    #[serde(default)]
    pub league_id: Option<String>,
    pub name: String,
    pub season: String,
    pub settings: LeagueSettings,
    #[serde(default, deserialize_with = "nullable")]
    pub roster_positions: Vec<String>,
    #[serde(default)]
    pub previous_league_id: Option<String>,
    #[serde(default)]
    pub draft_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<LeagueMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LeagueSettings {
    #[serde(deserialize_with = "nullable")]
    pub playoff_week_start: i64,
    #[serde(deserialize_with = "nullable")]
    pub league_average_match: i64,
    #[serde(deserialize_with = "nullable")]
    pub playoff_teams: i64,
    pub playoff_round_type: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    pub divisions: i64,
    pub trade_deadline: Option<i64>,
    pub waiver_budget: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LeagueMetadata {
    pub latest_league_winner_roster_id: Option<Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RosterSettings {
    #[serde(deserialize_with = "nullable")]
    pub wins: i64,
    #[serde(deserialize_with = "nullable")]
    pub losses: i64,
    #[serde(deserialize_with = "nullable")]
    pub ties: i64,
    #[serde(deserialize_with = "nullable")]
    pub fpts: f64,
    #[serde(deserialize_with = "nullable")]
    pub fpts_decimal: f64,
    #[serde(deserialize_with = "nullable")]
    pub waiver_budget_used: f64,
    pub division: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawRoster {
    pub roster_id: i64,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub players: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub starters: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub reserve: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub taxi: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub settings: RosterSettings,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawUser {
    pub user_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub metadata: UserMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct UserMetadata {
    pub team_name: Option<String>,
    pub avatar: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawMatchup {
    pub roster_id: i64,
    #[serde(default)]
    pub matchup_id: Option<i64>,
    #[serde(default)]
    pub points: Option<f64>,
    #[serde(default)]
    pub custom_points: Option<f64>,
    #[serde(default, deserialize_with = "nullable")]
    pub starters: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub starters_points: Vec<Option<f64>>,
    #[serde(default, deserialize_with = "nullable")]
    pub players: Vec<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub players_points: HashMap<String, Option<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawTransaction {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub adds: IndexMap<String, i64>,
    #[serde(default, deserialize_with = "nullable")]
    pub drops: IndexMap<String, i64>,
    #[serde(default, deserialize_with = "nullable")]
    pub roster_ids: Vec<i64>,
    #[serde(default, deserialize_with = "nullable")]
    pub draft_picks: Vec<RawTradedPick>,
    #[serde(default, deserialize_with = "nullable")]
    pub settings: TransactionSettings,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub status_updated: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TransactionSettings {
    pub waiver_bid: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawTradedPick {
    #[serde(default)]
    pub season: Value,
    #[serde(default)]
    pub round: Option<i64>,
    #[serde(default)]
    pub owner_id: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawDraft {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub settings: DraftSettings,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DraftSettings {
    pub budget: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPick {
    pub player_id: String,
    pub pick_no: i64,
    pub round: i64,
    pub roster_id: i64,
    #[serde(default)]
    pub is_keeper: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub metadata: PickMetadata,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PickMetadata {
    pub amount: Option<Value>,
}

/// One entry of `/players/nfl`. Only the fields the pipeline reads.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RawPlayer {
    pub full_name: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub position: Option<String>,
    pub fantasy_positions: Option<Vec<Option<String>>>,
    pub team: Option<String>,
    pub search_rank: Option<Value>,
    pub injury_status: Option<String>,
}

impl RawPlayer {
    /// Sleeper's `search_rank`, as an integer rank when it is one.
    pub fn rank(&self) -> Option<i64> {
        match &self.search_rank {
            Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
            _ => None,
        }
    }
}

pub type PlayerMap = HashMap<String, RawPlayer>;

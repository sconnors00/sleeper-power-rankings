//! Pure functions: raw Sleeper data in, `models` out. No I/O, so everything
//! here can be tested against the committed fixtures.

pub mod acquisitions;
pub mod draft;
pub mod matchups;
pub mod playoffs;
pub mod preseason;
pub mod rivalries;
pub mod rosters;
pub mod season;
pub mod trades;

use std::collections::BTreeMap;

use crate::models::{PlayerScore, Team};
use crate::raw::{PlayerMap, RawRoster, RawUser};

pub const POSITIONS: [&str; 6] = ["QB", "RB", "WR", "TE", "K", "DEF"];
pub const DEFAULT_FORM_WINDOW: i64 = 3;
pub const PLAYOFF_SIMS: i64 = 10_000;
/// The last week of the NFL regular season; fantasy playoffs can't run past it.
pub const NFL_LAST_WEEK: i64 = 18;

/// 12 fixed, stable-order team colors. The first 8 are the validated
/// categorical palette (dataviz skill, references/palette.md); slots 9-12
/// extend it for a 12-team league.
pub const TEAM_COLORS_LIGHT: [&str; 12] = [
    "#2a78d6", // 1 blue
    "#eb6834", // 2 orange
    "#1baf7a", // 3 aqua
    "#eda100", // 4 yellow
    "#e87ba4", // 5 magenta
    "#008300", // 6 green
    "#4a3aa7", // 7 violet
    "#e34948", // 8 red
    "#8a5a2b", // 9 brown
    "#5ac8c8", // 10 teal
    "#c77dff", // 11 lavender
    "#6b7280", // 12 gray
];

/// Which positions each flex-style lineup slot accepts.
pub const FLEX_ELIGIBILITY: [(&str, &[&str]); 5] = [
    ("FLEX", &["RB", "WR", "TE"]),
    ("SUPER_FLEX", &["QB", "RB", "WR", "TE"]),
    ("REC_FLEX", &["WR", "TE"]),
    ("WRRB_FLEX", &["RB", "WR"]),
    ("IDP_FLEX", &["DL", "LB", "DB"]),
];

/// Roster slots that never start a game.
pub const NON_STARTING_SLOTS: [&str; 3] = ["BN", "IR", "TAXI"];

pub fn flex_eligibility(slot: &str) -> Option<&'static [&'static str]> {
    FLEX_ELIGIBILITY
        .iter()
        .find(|(name, _)| *name == slot)
        .map(|(_, positions)| *positions)
}

pub fn is_starting_slot(slot: &str) -> bool {
    !NON_STARTING_SLOTS.contains(&slot)
}

/// Team display name: the team name if set, else the display name.
pub fn resolve_team_name(user: Option<&RawUser>, roster_id: i64) -> String {
    let Some(user) = user else {
        return format!("Team {roster_id}");
    };
    if let Some(team_name) = user.metadata.team_name.as_deref().filter(|s| !s.is_empty()) {
        return team_name.to_string();
    }
    user.display_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("Team {roster_id}"))
}

pub fn resolve_avatar_url(user: Option<&RawUser>) -> Option<String> {
    let user = user?;
    if let Some(serde_json::Value::String(url)) = &user.metadata.avatar {
        if url.starts_with("http") && !url.is_empty() {
            return Some(url.clone());
        }
    }
    user.avatar
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|hash| format!("https://sleepercdn.com/avatars/thumbs/{hash}"))
}

pub fn build_teams(rosters: &[RawRoster], users: &[RawUser]) -> BTreeMap<i64, Team> {
    let users_by_id: std::collections::HashMap<&str, &RawUser> =
        users.iter().map(|u| (u.user_id.as_str(), u)).collect();
    let mut sorted: Vec<&RawRoster> = rosters.iter().collect();
    sorted.sort_by_key(|r| r.roster_id);
    let mut teams = BTreeMap::new();
    for (i, roster) in sorted.iter().enumerate() {
        let user = roster
            .owner_id
            .as_deref()
            .and_then(|oid| users_by_id.get(oid).copied());
        teams.insert(
            roster.roster_id,
            Team {
                roster_id: roster.roster_id,
                owner_id: roster.owner_id.clone(),
                name: resolve_team_name(user, roster.roster_id),
                avatar_url: resolve_avatar_url(user),
                color: TEAM_COLORS_LIGHT[i % TEAM_COLORS_LIGHT.len()].to_string(),
                owner_name: user.and_then(|u| u.display_name.clone()),
            },
        );
    }
    teams
}

/// A player's display name, position and team, with `points` attached.
pub fn player_score(player_id: &str, points: f64, players: &PlayerMap) -> PlayerScore {
    let info = players.get(player_id);
    let name = info
        .and_then(|p| p.full_name.clone().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| {
            let first = info.and_then(|p| p.first_name.as_deref()).unwrap_or("");
            let last = info.and_then(|p| p.last_name.as_deref()).unwrap_or("");
            let joined = format!("{first} {last}");
            let trimmed = joined.trim();
            if trimmed.is_empty() {
                player_id.to_string()
            } else {
                trimmed.to_string()
            }
        });
    let position = info
        .and_then(|p| p.position.clone().filter(|s| !s.is_empty()))
        .or_else(|| {
            info.and_then(|p| p.fantasy_positions.as_ref())
                .and_then(|list| list.first().cloned().flatten())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "?".to_string());
    PlayerScore {
        player_id: player_id.to_string(),
        name,
        position,
        nfl_team: info.and_then(|p| p.team.clone()),
        points,
    }
}

/// Python's built-in `sum()` over floats. CPython 3.12 uses Neumaier
/// compensated summation there, so this does too, to match its last digit.
pub fn sum_f64(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut total = 0.0;
    let mut compensation = 0.0;
    for x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            compensation += (total - t) + x;
        } else {
            compensation += (x - t) + total;
        }
        total = t;
    }
    if compensation != 0.0 && compensation.is_finite() {
        total + compensation
    } else {
        total
    }
}

/// Python's `f"{x:.1f}"`-style record: one decimal when a half win is involved.
pub fn format_record(wins: f64, losses: f64) -> String {
    if wins.fract() != 0.0 || losses.fract() != 0.0 {
        format!("{wins:.1}-{losses:.1}")
    } else {
        format!("{}-{}", wins as i64, losses as i64)
    }
}

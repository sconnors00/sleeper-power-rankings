//! The two machine-readable outputs: `data.js` for the charts and the trade
//! calculator, and `rosters.json` for scripts. Both match the Python build's
//! bytes, since the page asset-version hash covers `data.js`.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::env::STATIC_FILES;
use crate::compute::trades::{FAIR_TRADE_GAP, LINEUP_FIT_GAP, TRADE_VERDICT_BANDS};
use crate::compute::FLEX_ELIGIBILITY;
use crate::models::{RosterSpot, SeasonSummary, TradeValues};
use crate::pyjson;

/// Short digest of every file a page loads, used as a `?v=` cache buster.
pub fn asset_version(data_js: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(data_js.as_bytes());
    for (_, bytes) in STATIC_FILES {
        digest.update(bytes);
    }
    let hash = digest.finalize();
    hash.iter().map(|b| format!("{b:02x}")).collect::<String>()[..12].to_string()
}

fn key(rid: i64) -> String {
    rid.to_string()
}

/// Serialize everything the charts need into `window.FFPR = {...}`.
pub fn build_data_js(season: &SeasonSummary) -> String {
    let mut payload = Map::new();
    payload.insert("season".into(), json!(season.season));
    payload.insert("leagueName".into(), json!(season.league_name));
    payload.insert("throughWeek".into(), json!(season.through_week));
    payload.insert("provisionalWeek".into(), json!(season.provisional_week));
    payload.insert("playoffWeekStart".into(), json!(season.playoff_week_start));

    let teams: Map<String, Value> = season
        .teams
        .iter()
        .map(|(rid, t)| {
            (
                key(*rid),
                json!({
                    "name": t.name,
                    "color": t.color,
                    "colorDark": t.color_dark,
                    "avatar": t.avatar_url,
                }),
            )
        })
        .collect();
    payload.insert("teams".into(), Value::Object(teams));

    let week_numbers: Vec<i64> = season.weeks.iter().map(|w| w.week).collect();

    // Charts on every week page need that week's matchups, including the
    // provisional week, which is excluded from rankings but still gets a page.
    let mut chart_weeks: Vec<&crate::models::WeekSummary> = season.weeks.iter().collect();
    if let Some(prov) = &season.provisional_week_summary {
        chart_weeks.push(prov);
    }
    let mut week_matchups = Map::new();
    for wk in &chart_weeks {
        let rows: Vec<Value> = wk
            .matchups
            .iter()
            .map(|m| {
                let opp = wk
                    .matchups
                    .iter()
                    .find(|x| Some(x.roster_id) == m.opponent_roster_id);
                json!({
                    "rosterId": m.roster_id,
                    "points": m.team_points,
                    "opponentRosterId": m.opponent_roster_id,
                    "opponentPoints": opp.map(|o| o.team_points),
                    "margin": opp.map(|o| m.team_points - o.team_points),
                })
            })
            .collect();
        week_matchups.insert(wk.week.to_string(), Value::Array(rows));
    }
    payload.insert("weekMatchups".into(), Value::Object(week_matchups));

    let rank_series: Map<String, Value> = season
        .teams
        .keys()
        .map(|rid| {
            let ranks: Vec<Value> = season
                .weeks
                .iter()
                .map(|wk| {
                    wk.power_rankings
                        .iter()
                        .find(|row| row.roster_id == *rid)
                        .map_or(Value::Null, |row| json!(row.rank))
                })
                .collect();
            (key(*rid), Value::Array(ranks))
        })
        .collect();
    payload.insert(
        "rankTrajectory".into(),
        json!({"weeks": week_numbers, "series": Value::Object(rank_series)}),
    );

    let points_series: Map<String, Value> = season
        .teams
        .keys()
        .map(|rid| {
            let points: Vec<Value> = season
                .weeks
                .iter()
                .map(|wk| {
                    wk.matchups
                        .iter()
                        .find(|m| m.roster_id == *rid)
                        .map_or(Value::Null, |m| json!(m.team_points))
                })
                .collect();
            (key(*rid), Value::Array(points))
        })
        .collect();
    let league_avg: Vec<Value> = season
        .weeks
        .iter()
        .map(|wk| {
            if wk.matchups.is_empty() {
                json!(0)
            } else {
                json!(
                    crate::compute::sum_f64(wk.matchups.iter().map(|m| m.team_points))
                        / wk.matchups.len() as f64
                )
            }
        })
        .collect();
    payload.insert(
        "weeklyPointsByTeam".into(),
        json!({"weeks": week_numbers, "series": Value::Object(points_series), "leagueAvg": league_avg}),
    );

    let mut spread = Vec::new();
    for wk in &season.weeks {
        if wk.matchups.is_empty() {
            continue;
        }
        let mut scores: Vec<(f64, i64)> = wk
            .matchups
            .iter()
            .map(|m| (m.team_points, m.roster_id))
            .collect();
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let (low, low_rid) = scores[0];
        let (high, high_rid) = scores[scores.len() - 1];
        let mid = scores.len() / 2;
        let median = if scores.len() % 2 == 1 {
            scores[mid].0
        } else {
            (scores[mid - 1].0 + scores[mid].0) / 2.0
        };
        spread.push(json!({
            "week": wk.week,
            "low": low,
            "high": high,
            "median": median,
            "lowRosterId": low_rid,
            "highRosterId": high_rid,
        }));
    }
    payload.insert("scoringSpread".into(), Value::Array(spread));

    // Every team plays every regular-season week (twice, counting a median game).
    let games = season.weeks.len() as i64 * if season.league_average_match { 2 } else { 1 };
    let mut luck = Vec::new();
    if let Some(last) = season.weeks.last() {
        let mut rows: Vec<&crate::models::PowerRankRow> = last.power_rankings.iter().collect();
        rows.sort_by_key(|r| r.roster_id);
        for row in rows {
            luck.push(json!({
                "rosterId": row.roster_id,
                "allplayPct": row.allplay_pct,
                "winPct": row.win_pct,
                "record": row.record,
                "allplayRecord": row.allplay_record,
                "games": games,
            }));
        }
    }
    payload.insert("luck".into(), Value::Array(luck));

    let pf_vs_pa: Vec<Value> = season
        .season_board
        .pf_leaderboard
        .iter()
        .map(|row| json!({"rosterId": row.roster_id, "pf": row.pf, "pa": row.pa}))
        .collect();
    payload.insert("pfVsPa".into(), Value::Array(pf_vs_pa));

    if let Some(trade) = trade_payload(season) {
        payload.insert("trade".into(), trade);
    }
    format!(
        "window.FFPR = {};\n",
        pyjson::dumps(&Value::Object(payload), None)
    )
}

/// What the trade calculator needs: every rostered player's value and
/// projection, who holds whom, and the rules it mirrors.
fn trade_payload(season: &SeasonSummary) -> Option<Value> {
    let tv: &TradeValues = season.trade_values.as_ref()?;
    let mut players = Map::new();
    for pids in tv.rosters.iter().map(|(_, pids)| pids) {
        for pid in pids {
            let pv = &tv.players[pid];
            players.insert(
                pid.clone(),
                json!({
                    "name": pv.player.name,
                    "pos": pv.player.position,
                    "nfl": pv.player.nfl_team,
                    "proj": pv.projection,
                    "value": pv.value,
                    "injury": pv.injury,
                    "active": pv.active,
                    "avail": pv.availability,
                }),
            );
        }
    }
    let flex: Map<String, Value> = FLEX_ELIGIBILITY
        .iter()
        .map(|(slot, eligible)| {
            let mut positions: Vec<&str> = eligible.to_vec();
            positions.sort();
            (slot.to_string(), json!(positions))
        })
        .collect();
    let playoff_pct: Map<String, Value> = season
        .playoff_odds
        .iter()
        .map(|row| (key(row.roster_id), json!(row.playoff_pct)))
        .collect();
    let rosters: Map<String, Value> = tv
        .rosters
        .iter()
        .map(|(rid, pids)| (key(*rid), json!(pids)))
        .collect();
    let replacement: Map<String, Value> = tv
        .replacement
        .iter()
        .map(|(pos, level)| (pos.clone(), json!(level)))
        .collect();
    Some(json!({
        "rosterPositions": season.roster_positions,
        "flex": Value::Object(flex),
        "rosterLimit": tv.roster_limit,
        "weeksLeft": tv.weeks_left,
        "fairGap": FAIR_TRADE_GAP,
        "fitGap": LINEUP_FIT_GAP,
        "bands": TRADE_VERDICT_BANDS,
        "horizon": tv.horizon,
        "weights": tv.weights,
        "playoffFrom": tv.playoff_from,
        "playoffPct": Value::Object(playoff_pct),
        "replacement": Value::Object(replacement),
        "players": Value::Object(players),
        "rosters": Value::Object(rosters),
    }))
}

fn spot_json(spot: &RosterSpot) -> Value {
    json!({
        "slot": spot.slot,
        "player_id": spot.player_id,
        "name": spot.name,
        "position": spot.position,
        "nfl_team": spot.nfl_team,
        "injury": spot.injury,
    })
}

/// `rosters.json`: the same rosters and FAAB the page shows, for scripts.
/// Stable keys and ordering, no timestamps.
pub fn build_rosters_json(season: &SeasonSummary) -> String {
    let report = season
        .roster_report
        .as_ref()
        .expect("rosters.json is only written with a roster report");
    let teams: Vec<Value> = report
        .teams
        .iter()
        .map(|tr| {
            let team = &season.teams[&tr.roster_id];
            json!({
                "roster_id": tr.roster_id,
                "team_name": team.name,
                "owner": team.owner_name,
                "owner_id": team.owner_id,
                "record": tr.record,
                "faab": {
                    "budget": report.faab_budget,
                    "used": tr.faab_used,
                    "remaining": tr.faab_remaining,
                },
                "starters": tr.starters.iter().map(spot_json).collect::<Vec<_>>(),
                "bench": tr.bench.iter().map(spot_json).collect::<Vec<_>>(),
                "ir": tr.ir.iter().map(spot_json).collect::<Vec<_>>(),
                "taxi": tr.taxi.iter().map(spot_json).collect::<Vec<_>>(),
                "faab_claims": tr.claims.iter().map(|c| json!({
                    "week": c.week,
                    "player_id": c.player.player_id,
                    "player": c.player.name,
                    "position": c.player.position,
                    "bid": c.bid,
                    "dropped": c.dropped,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let payload = json!({
        "season": season.season,
        "league": season.league_name,
        "through_week": season.through_week,
        "roster_positions": season.roster_positions,
        "faab_budget": report.faab_budget,
        "teams": teams,
    });
    format!("{}\n", pyjson::dumps(&payload, Some(2)))
}

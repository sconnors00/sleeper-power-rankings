//! Rest-of-season trade values. A player's projection blends what he has
//! scored in this league with what his Sleeper ranking implies at his
//! position; his value is the surplus over replacement, scaled by the share
//! of the remaining weeks he's expected to play (byes and injuries count).

use std::collections::{BTreeMap, HashSet};

use indexmap::IndexMap;

use super::{flex_eligibility, is_starting_slot, sum_f64, POSITIONS};
use crate::models::{PlayerScore, PlayerValue, TeamStrengthRow, TradeValues};
use crate::raw::{PlayerMap, RawRoster};
use crate::util::round2;

pub const PRIOR_GAMES: i64 = 5;
/// A value gap under this many points per week always reads as fair.
pub const FAIR_TRADE_GAP: f64 = 1.0;
/// A lineup change smaller than this many points per week reads as no change.
pub const LINEUP_FIT_GAP: f64 = 0.5;
/// Value gap as a share of the bigger side: fair, slight edge, favors, lopsided.
pub const TRADE_VERDICT_BANDS: [f64; 3] = [0.10, 0.25, 0.50];

/// Short tags for injury statuses; anything else is shown as Sleeper sends it.
pub fn injury_tag(status: &str) -> &str {
    match status {
        "Questionable" => "Q",
        "Doubtful" => "D",
        other => other,
    }
}

/// How much of each coming week a player is expected to play, by Sleeper's
/// injury_status. Weeks past the list are full weeks.
pub fn injury_outlook(status: &str) -> &'static [f64] {
    match status {
        "Questionable" => &[0.75],
        "Doubtful" => &[0.25],
        "Out" | "Sus" => &[0.0],
        "IR" | "PUP" => &[0.0, 0.0, 0.0, 0.0],
        _ => &[],
    }
}

/// The weeks of an NFL season, as a set.
pub type WeekSet = std::collections::BTreeSet<i64>;

/// The injury outlook table, keyed by Sleeper's injury status (for the
/// how-it-works page).
pub fn injury_outlook_table() -> BTreeMap<&'static str, Vec<f64>> {
    ["Questionable", "Doubtful", "Out", "Sus", "IR", "PUP"]
        .into_iter()
        .map(|status| (status, injury_outlook(status).to_vec()))
        .collect()
}

/// NFL team -> the weeks it has no game, from Sleeper's NFL schedule.
pub fn nfl_byes(schedule: &serde_json::Value) -> BTreeMap<String, WeekSet> {
    let mut played: BTreeMap<String, WeekSet> = BTreeMap::new();
    let Some(games) = schedule.as_array() else {
        return BTreeMap::new();
    };
    for game in games {
        let Some(week) = game.get("week").and_then(serde_json::Value::as_i64) else {
            continue;
        };
        for side in ["home", "away"] {
            if let Some(team) = game
                .get(side)
                .and_then(serde_json::Value::as_str)
                .filter(|t| !t.is_empty())
            {
                played.entry(team.to_string()).or_default().insert(week);
            }
        }
    }
    if played.is_empty() {
        return BTreeMap::new();
    }
    let all_weeks: WeekSet = played.values().flatten().copied().collect();
    played
        .into_iter()
        .map(|(team, games)| (team, all_weeks.difference(&games).copied().collect()))
        .collect()
}

/// The share of each horizon week a player is expected to play: 0 on his bye,
/// discounted or 0 for the weeks his injury status covers, else 1.
pub fn availability(
    status: Option<&str>,
    nfl_team: Option<&str>,
    horizon: &[i64],
    byes: &BTreeMap<String, WeekSet>,
) -> Vec<f64> {
    let outlook = injury_outlook(status.unwrap_or(""));
    let off = nfl_team.and_then(|t| byes.get(t));
    horizon
        .iter()
        .enumerate()
        .map(|(i, week)| {
            if off.is_some_and(|set| set.contains(week)) {
                0.0
            } else {
                outlook.get(i).copied().unwrap_or(1.0)
            }
        })
        .collect()
}

/// Player id -> {week: points} for every week he was on a roster.
pub fn player_weekly_points(
    weeks_raw: &BTreeMap<i64, Vec<crate::raw::RawMatchup>>,
) -> BTreeMap<String, BTreeMap<i64, f64>> {
    let mut weekly: BTreeMap<String, BTreeMap<i64, f64>> = BTreeMap::new();
    for (week, raw) in weeks_raw {
        for m in raw {
            for (pid, pts) in &m.players_points {
                weekly
                    .entry(pid.clone())
                    .or_default()
                    .insert(*week, pts.unwrap_or(0.0));
            }
        }
    }
    weekly
}

/// A position's points-per-game prior, as a function of search rank.
pub type RankPrior = Box<dyn Fn(Option<i64>) -> f64>;

/// Points per game as a function of Sleeper's search_rank, fitted per position.
///
/// `samples` are (search_rank, points per game, games) for the position's
/// players who have scored. The fit is a least-squares line through points per
/// game against log(rank), weighted by games, and is only read inside the
/// ranks it was fitted on. Without a usable fit everyone gets the average.
pub fn rank_prior(samples: &[(Option<i64>, f64, i64)]) -> RankPrior {
    let games: i64 = samples.iter().map(|s| s.2).sum();
    let average = if games > 0 {
        (sum_f64(samples.iter().map(|(_, ppg, g)| ppg * *g as f64)) / games as f64).max(0.0)
    } else {
        0.0
    };
    let ranked: Vec<(f64, f64, f64)> = samples
        .iter()
        .filter_map(|(rank, ppg, g)| match rank {
            Some(r) if *r != 0 => Some(((*r as f64).ln(), *ppg, *g as f64)),
            _ => None,
        })
        .collect();
    let mut xs: Vec<f64> = ranked.iter().map(|(x, _, _)| *x).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs.dedup();
    if xs.len() < 3 {
        return Box::new(move |_| average);
    }
    let weight: f64 = ranked.iter().map(|(_, _, g)| g).sum();
    let mean_x = sum_f64(ranked.iter().map(|(x, _, g)| x * g)) / weight;
    let mean_y = sum_f64(ranked.iter().map(|(_, y, g)| y * g)) / weight;
    let sxx = sum_f64(
        ranked
            .iter()
            // Python writes g * (x - mean_x) ** 2: square first, then weight.
            .map(|(x, _, g)| g * ((x - mean_x) * (x - mean_x))),
    );
    let sxy = sum_f64(
        ranked
            .iter()
            .map(|(x, y, g)| g * (x - mean_x) * (y - mean_y)),
    );
    let slope = sxy / sxx;
    if slope >= 0.0 {
        return Box::new(move |_| average);
    }
    let lo = xs[0];
    let hi = xs[xs.len() - 1];
    Box::new(move |rank| {
        let x = match rank {
            Some(r) if r != 0 => (r as f64).ln().clamp(lo, hi),
            _ => hi,
        };
        (mean_y + slope * (x - mean_x)).max(0.0)
    })
}

/// Position -> the projection any team can count on to fill a starting spot.
///
/// Every team's starting slots are filled from the league-wide pool, best
/// players first: fixed slots, then flex slots from the narrowest eligibility
/// out. The best player left over at a position sets its replacement level.
pub fn replacement_levels(
    players: &[PlayerScore],
    roster_positions: &[String],
    num_teams: usize,
) -> BTreeMap<String, f64> {
    let mut counts: IndexMap<&str, usize> = IndexMap::new();
    for slot in roster_positions {
        if is_starting_slot(slot) {
            *counts.entry(slot.as_str()).or_insert(0) += 1;
        }
    }
    let fixed: Vec<&str> = counts
        .keys()
        .copied()
        .filter(|s| flex_eligibility(s).is_none())
        .collect();
    let mut flexes: Vec<&str> = counts
        .keys()
        .copied()
        .filter(|s| flex_eligibility(s).is_some())
        .collect();
    flexes.sort_by_key(|s| flex_eligibility(s).map_or(0, |e| e.len()));

    let mut remaining: Vec<PlayerScore> = players.to_vec();
    remaining.sort_by(|a, b| {
        b.points
            .partial_cmp(&a.points)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut starters: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for slot in fixed.iter().chain(flexes.iter()) {
        let eligible: Vec<&str> =
            flex_eligibility(slot).map_or_else(|| vec![*slot], |e| e.to_vec());
        let limit = counts[slot] * num_teams;
        let mut taken_idx: Vec<usize> = Vec::new();
        for (i, p) in remaining.iter().enumerate() {
            if taken_idx.len() >= limit {
                break;
            }
            if eligible.contains(&p.position.as_str()) {
                taken_idx.push(i);
            }
        }
        for &i in &taken_idx {
            starters
                .entry(remaining[i].position.clone())
                .or_default()
                .push(remaining[i].points);
        }
        let taken_set: HashSet<usize> = taken_idx.into_iter().collect();
        remaining = remaining
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !taken_set.contains(i))
            .map(|(_, p)| p)
            .collect();
    }

    let mut positions: Vec<String> = fixed.iter().map(|s| s.to_string()).collect();
    for flex in &flexes {
        for pos in flex_eligibility(flex).unwrap_or(&[]) {
            positions.push(pos.to_string());
        }
    }
    positions.sort();
    positions.dedup();

    let mut levels = BTreeMap::new();
    for pos in positions {
        let left = remaining
            .iter()
            .find(|p| p.position == pos)
            .map(|p| p.points)
            .or_else(|| {
                starters
                    .get(&pos)
                    .and_then(|v| crate::util::py_min(v.iter().copied()))
            });
        if let Some(level) = left {
            levels.insert(pos, level);
        }
    }
    levels
}

/// Each team's best projected lineup, ranked overall and position by position.
pub fn team_strength(
    players: &BTreeMap<String, PlayerValue>,
    rosters: &[(i64, Vec<String>)],
    roster_positions: &[String],
) -> Vec<TeamStrengthRow> {
    let mut totals: BTreeMap<i64, f64> = BTreeMap::new();
    let mut by_pos: BTreeMap<i64, BTreeMap<String, f64>> = BTreeMap::new();
    for (rid, pids) in rosters {
        let pool: Vec<PlayerScore> = pids
            .iter()
            .filter(|pid| players[*pid].active)
            .map(|pid| players[pid].player.clone())
            .collect();
        let lineup = super::matchups::best_lineup(&pool, roster_positions, &HashSet::new());
        let mut points: BTreeMap<String, f64> =
            POSITIONS.iter().map(|p| (p.to_string(), 0.0)).collect();
        for p in &lineup {
            if let Some(total) = points.get_mut(&p.position) {
                *total += p.points;
            }
        }
        by_pos.insert(
            *rid,
            points.into_iter().map(|(k, v)| (k, round2(v))).collect(),
        );
        totals.insert(*rid, round2(sum_f64(lineup.iter().map(|p| p.points))));
    }

    let share_ranks = |values: &dyn Fn(i64) -> f64| -> BTreeMap<i64, i64> {
        let ids: Vec<i64> = by_pos.keys().copied().collect();
        ids.iter()
            .map(|rid| {
                let mine = values(*rid);
                (
                    *rid,
                    1 + ids.iter().filter(|other| values(**other) > mine).count() as i64,
                )
            })
            .collect()
    };
    let total_ranks = share_ranks(&|rid| totals[&rid]);
    let pos_ranks: BTreeMap<String, BTreeMap<i64, i64>> = POSITIONS
        .iter()
        .map(|pos| (pos.to_string(), share_ranks(&|rid| by_pos[&rid][*pos])))
        .collect();

    let mut rows: Vec<TeamStrengthRow> = by_pos
        .iter()
        .map(|(rid, points)| TeamStrengthRow {
            roster_id: *rid,
            total: totals[rid],
            rank: total_ranks[rid],
            points: points.clone(),
            ranks: POSITIONS
                .iter()
                .map(|pos| (pos.to_string(), pos_ranks[*pos][rid]))
                .collect(),
        })
        .collect();
    rows.sort_by(|a, b| a.rank.cmp(&b.rank).then(a.roster_id.cmp(&b.roster_id)));
    rows
}

/// Rest-of-season trade values for every rostered player.
///
/// ```text
/// projection = (games * points per game + prior_games * prior) / (games + prior_games)
/// ```
///
/// `horizon` maps each week still to be played to its weight. Value is the
/// surplus over replacement times the weighted share of those weeks he's
/// expected to play.
#[allow(clippy::too_many_arguments)]
pub fn build_trade_values(
    weeks_raw: &BTreeMap<i64, Vec<crate::raw::RawMatchup>>,
    rosters: &[RawRoster],
    players: &PlayerMap,
    roster_positions: &[String],
    weeks_left: i64,
    trade_deadline: Option<i64>,
    prior_games: i64,
    horizon: &IndexMap<i64, f64>,
    playoff_from: Option<i64>,
    byes: &BTreeMap<String, WeekSet>,
) -> TradeValues {
    let weekly = player_weekly_points(weeks_raw);
    let mut holder: IndexMap<String, i64> = IndexMap::new();
    for r in rosters {
        for pid in &r.players {
            holder.insert(pid.clone(), r.roster_id);
        }
    }
    let sidelined: HashSet<&str> = rosters
        .iter()
        .flat_map(|r| r.reserve.iter().chain(r.taxi.iter()).map(String::as_str))
        .collect();

    // pid -> (player, games, ppg, search rank), in sorted player-id order.
    let mut all_pids: Vec<String> = weekly.keys().chain(holder.keys()).cloned().collect();
    all_pids.sort();
    all_pids.dedup();
    let mut production: IndexMap<String, (PlayerScore, i64, f64, Option<i64>)> = IndexMap::new();
    for pid in all_pids {
        let played: Vec<f64> = weekly
            .get(&pid)
            .map(|weeks| weeks.values().copied().filter(|p| *p != 0.0).collect())
            .unwrap_or_default();
        let ppg = if played.is_empty() {
            0.0
        } else {
            sum_f64(played.iter().copied()) / played.len() as f64
        };
        let rank = players.get(&pid).and_then(|p| p.rank());
        let player = super::player_score(&pid, 0.0, players);
        production.insert(pid.clone(), (player, played.len() as i64, ppg, rank));
    }

    // Position -> (rank, ppg, games) samples for every player who has scored.
    let mut samples: BTreeMap<String, Vec<(Option<i64>, f64, i64)>> = BTreeMap::new();
    for (player, games, ppg, rank) in production.values() {
        if *games > 0 {
            samples
                .entry(player.position.clone())
                .or_default()
                .push((*rank, *ppg, *games));
        }
    }
    let priors: BTreeMap<String, RankPrior> = samples
        .iter()
        .map(|(pos, s)| (pos.clone(), rank_prior(s)))
        .collect();

    let mut expected: BTreeMap<String, f64> = BTreeMap::new();
    for (pid, (player, games, ppg, rank)) in production.iter_mut() {
        let prior = priors.get(&player.position).map_or(0.0, |f| f(*rank));
        expected.insert(pid.clone(), prior);
        let weight = *games + prior_games;
        player.points = if weight > 0 {
            round2((*games as f64 * *ppg + prior_games as f64 * prior) / weight as f64)
        } else {
            0.0
        };
    }

    let pool: Vec<PlayerScore> = holder
        .keys()
        .filter(|pid| !sidelined.contains(pid.as_str()))
        .filter_map(|pid| production.get(pid).map(|(p, _, _, _)| p.clone()))
        .collect();
    let levels = replacement_levels(&pool, roster_positions, rosters.len());

    let weeks: Vec<i64> = horizon.keys().copied().collect();
    let weights: Vec<f64> = weeks.iter().map(|w| horizon[w]).collect();
    let weight_sum = sum_f64(weights.iter().copied());

    let mut values: BTreeMap<String, PlayerValue> = BTreeMap::new();
    for (pid, (player, games, ppg, _rank)) in &production {
        let level = levels.get(&player.position).copied();
        let status = players.get(pid).and_then(|p| p.injury_status.clone());
        let surplus = match level {
            Some(l) => (player.points - l).max(0.0),
            None => 0.0,
        };
        let avail = availability(status.as_deref(), player.nfl_team.as_deref(), &weeks, byes);
        let played = sum_f64(weights.iter().zip(&avail).map(|(w, a)| w * a));
        let share = if weight_sum != 0.0 {
            played / weight_sum
        } else {
            1.0
        };
        let weeks_available = sum_f64(avail.iter().copied());
        values.insert(
            pid.clone(),
            PlayerValue {
                player: player.clone(),
                roster_id: holder.get(pid).copied(),
                games: *games,
                ppg: round2(*ppg),
                prior: round2(expected[pid]),
                value: round2(surplus * share),
                injury: status
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .map(|s| injury_tag(s).to_string()),
                active: !sidelined.contains(pid.as_str()),
                surplus: round2(surplus),
                availability: avail,
                projection: player.points,
                weeks_available,
            },
        );
    }

    // Each roster's players, most valuable first.
    let mut by_roster: IndexMap<i64, Vec<String>> =
        rosters.iter().map(|r| (r.roster_id, Vec::new())).collect();
    for (pid, rid) in &holder {
        by_roster.entry(*rid).or_default().push(pid.clone());
    }
    for pids in by_roster.values_mut() {
        pids.sort_by(|a, b| {
            let (va, vb) = (&values[a], &values[b]);
            vb.value
                .partial_cmp(&va.value)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    vb.projection
                        .partial_cmp(&va.projection)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.cmp(b))
        });
    }
    let rosters_out: Vec<(i64, Vec<String>)> = by_roster.into_iter().collect();

    let strength = team_strength(&values, &rosters_out, roster_positions);
    let roster_limit = roster_positions
        .iter()
        .filter(|s| s.as_str() != "IR" && s.as_str() != "TAXI")
        .count() as i64;

    TradeValues {
        players: values,
        replacement: levels,
        rosters: rosters_out,
        strength,
        prior_games,
        weeks_left,
        roster_limit,
        trade_deadline,
        horizon: weeks,
        weights,
        playoff_from,
    }
}

/// How lopsided a trade is, from the value each side receives.
///
/// Returns (verdict, index of the side ahead). "even" when nobody gets a
/// player above replacement; "fair" under the first band or the fair-gap
/// minimum; then "slight edge", "favors" and "lopsided" as the gap grows.
pub fn trade_verdict(gets: &[f64]) -> (&'static str, Option<usize>) {
    let mut order: Vec<usize> = (0..gets.len()).collect();
    order.sort_by(|a, b| {
        gets[*b]
            .partial_cmp(&gets[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if order.len() < 2 || gets[order[0]] < 0.005 {
        return ("even", None);
    }
    let top = gets[order[0]];
    let second = gets[order[1]];
    let gap = top - second;
    let share = gap / top;
    let [fair, slight, favors] = TRADE_VERDICT_BANDS;
    if gap < FAIR_TRADE_GAP || share < fair {
        return ("fair", None);
    }
    if share < slight {
        return ("slight edge", Some(order[0]));
    }
    (
        if share < favors { "favors" } else { "lopsided" },
        Some(order[0]),
    )
}

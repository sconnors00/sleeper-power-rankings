//! Playoff odds: Monte Carlo over the rest of the regular season, plus the
//! playoff bracket rules (byes, seeding, which weeks the playoffs use).

use std::collections::BTreeMap;

use indexmap::IndexMap;

use super::matchups::compute_weekly_head_to_head;
use super::{format_record, NFL_LAST_WEEK};
use crate::models::{PlayoffOddsRow, WeekSummary};
use crate::pyrand::PyRandom;
use crate::raw::RawMatchup;
use crate::util::{fmean, median, pow_half, pvariance};

/// (roster, roster) pairings for a scheduled week; unpaired entries skipped.
pub fn schedule_pairs(raw: &[RawMatchup]) -> Vec<(i64, i64)> {
    let mut by_matchup: IndexMap<i64, Vec<i64>> = IndexMap::new();
    for m in raw {
        if let Some(mid) = m.matchup_id {
            by_matchup.entry(mid).or_default().push(m.roster_id);
        }
    }
    by_matchup
        .values()
        .filter(|ids| ids.len() == 2)
        .map(|ids| (ids[0], ids[1]))
        .collect()
}

/// First-round byes in a single-elimination bracket: the gap to a power of two.
pub fn playoff_byes(playoff_teams: i64) -> i64 {
    let mut size = 1;
    while size < playoff_teams {
        size *= 2;
    }
    size - playoff_teams
}

/// The weeks the fantasy playoffs are played, from Sleeper's league settings.
/// `playoff_round_type` 0 plays a round a week, 1 makes the championship a
/// two-week round, 2 makes every round two weeks.
pub fn playoff_weeks(teams: i64, start: i64, round_type: Option<i64>) -> Vec<i64> {
    if teams < 2 || start == 0 {
        return Vec::new();
    }
    let rounds = (teams + playoff_byes(teams)).ilog2() as i64;
    let weeks = match round_type {
        Some(1) => rounds + 1,
        Some(2) => 2 * rounds,
        _ => rounds,
    };
    (start..(start + weeks).min(NFL_LAST_WEEK + 1)).collect()
}

/// Playoff roster ids in seed order. Standings go by wins, then points for.
/// With divisions on, every division winner is seeded ahead of every wild card.
pub fn seed_playoffs(
    wins: &BTreeMap<i64, f64>,
    points_for: &BTreeMap<i64, f64>,
    playoff_teams: i64,
    divisions: Option<&BTreeMap<i64, Option<i64>>>,
) -> Vec<i64> {
    let limit = playoff_teams.max(0) as usize;
    let mut order: Vec<i64> = wins.keys().copied().collect();
    order.sort_by(|a, b| {
        wins[b]
            .partial_cmp(&wins[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                points_for[b]
                    .partial_cmp(&points_for[a])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then(a.cmp(b))
    });
    let divisions = match divisions {
        Some(d) if !d.is_empty() => d,
        _ => return order.into_iter().take(limit).collect(),
    };
    let mut winners: Vec<i64> = Vec::new();
    let mut won: Vec<i64> = Vec::new();
    for rid in &order {
        if let Some(Some(division)) = divisions.get(rid) {
            if !won.contains(division) {
                won.push(*division);
                winners.push(*rid);
            }
        }
    }
    let mut seeds = winners.clone();
    seeds.extend(order.iter().filter(|r| !winners.contains(r)));
    seeds.into_iter().take(limit).collect()
}

/// `"{wins:.1f}-{losses:.1f}"`-style record, with whole numbers when exact.
fn odds_record(wins: f64, losses: f64) -> String {
    format_record(wins, losses)
}

/// Monte Carlo the rest of the regular season from each team's scoring.
///
/// A team's scores are its true strength plus weekly noise. The noise is
/// measured from how much teams' own scores swing week to week; whatever
/// spread between team averages is left over is real strength.
#[allow(clippy::too_many_arguments)]
pub fn simulate_playoff_odds(
    weeks: &[WeekSummary],
    remaining: &BTreeMap<i64, Vec<(i64, i64)>>,
    league_average_match: bool,
    playoff_teams: i64,
    divisions: Option<&BTreeMap<i64, Option<i64>>>,
    sims: usize,
    seed: u64,
) -> Vec<PlayoffOddsRow> {
    let mut history: IndexMap<i64, Vec<f64>> = IndexMap::new();
    let mut wins: BTreeMap<i64, f64> = BTreeMap::new();
    let mut games: BTreeMap<i64, i64> = BTreeMap::new();
    let mut points_for: BTreeMap<i64, f64> = BTreeMap::new();
    for wk in weeks {
        for (rid, (w, g)) in compute_weekly_head_to_head(&wk.matchups, league_average_match) {
            *wins.entry(rid).or_insert(0.0) += w;
            *games.entry(rid).or_insert(0) += g;
        }
        for m in &wk.matchups {
            history.entry(m.roster_id).or_default().push(m.team_points);
            *points_for.entry(m.roster_id).or_insert(0.0) += m.team_points;
        }
    }
    if history.is_empty() || remaining.is_empty() {
        return Vec::new();
    }

    let all_scores: Vec<f64> = history.values().flatten().copied().collect();
    let league_mean = fmean(&all_scores);
    let team_means: BTreeMap<i64, f64> = history
        .iter()
        .map(|(rid, scores)| (*rid, fmean(scores)))
        .collect();
    let dof = all_scores.len() as i64 - history.len() as i64;
    let noise_var = if dof > 0 {
        let means = &team_means;
        let ss = history.iter().flat_map(|(rid, scores)| {
            scores
                .iter()
                .map(move |s| (s - means[rid]) * (s - means[rid]))
        });
        let ss = crate::compute::sum_f64(ss);
        ss / dof as f64
    } else {
        pvariance(&all_scores) // one week in: nothing yet separates luck from strength
    };
    let noise_var = noise_var.max(1.0);
    let per_team = all_scores.len() as f64 / history.len() as f64;
    let means: Vec<f64> = history.keys().map(|rid| team_means[rid]).collect();
    let talent_var = (pvariance(&means) - noise_var / per_team).max(0.0);

    // rid -> (estimate, its std error)
    let mut strength: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
    for (rid, scores) in &history {
        let sampling_var = noise_var / scores.len() as f64;
        let weight = if talent_var > 0.0 {
            talent_var / (talent_var + sampling_var)
        } else {
            0.0
        };
        let estimate = league_mean + weight * (team_means[rid] - league_mean);
        strength.insert(*rid, (estimate, pow_half(weight * sampling_var)));
    }
    let noise_sd = pow_half(noise_var);

    let rosters: Vec<i64> = {
        let mut keys: Vec<i64> = history.keys().copied().collect();
        keys.sort();
        keys
    };
    for rid in &rosters {
        wins.entry(*rid).or_insert(0.0);
        games.entry(*rid).or_insert(0);
    }
    let byes = playoff_byes(playoff_teams);
    let mut made: BTreeMap<i64, usize> = rosters.iter().map(|r| (*r, 0)).collect();
    let mut bye: BTreeMap<i64, usize> = rosters.iter().map(|r| (*r, 0)).collect();
    let mut top: BTreeMap<i64, usize> = rosters.iter().map(|r| (*r, 0)).collect();
    let mut total_wins: BTreeMap<i64, f64> = rosters.iter().map(|r| (*r, 0.0)).collect();
    let mut rng = PyRandom::new(seed);

    for _ in 0..sims {
        let mut w = wins.clone();
        let mut pf = points_for.clone();
        let true_strength: BTreeMap<i64, f64> = rosters
            .iter()
            .map(|rid| {
                let (mu, sigma) = strength[rid];
                (*rid, rng.gauss(mu, sigma))
            })
            .collect();
        for pairs in remaining.values() {
            let scores: Vec<(i64, f64)> = rosters
                .iter()
                .map(|rid| (*rid, rng.gauss(true_strength[rid], noise_sd)))
                .collect();
            let score_of: BTreeMap<i64, f64> = scores.iter().copied().collect();
            for (a, b) in pairs {
                let (Some(&sa), Some(&sb)) = (score_of.get(a), score_of.get(b)) else {
                    continue;
                };
                if sa > sb {
                    *w.entry(*a).or_insert(0.0) += 1.0;
                } else if sb > sa {
                    *w.entry(*b).or_insert(0.0) += 1.0;
                } else {
                    *w.entry(*a).or_insert(0.0) += 0.5;
                    *w.entry(*b).or_insert(0.0) += 0.5;
                }
            }
            if league_average_match {
                let values: Vec<f64> = scores.iter().map(|(_, s)| *s).collect();
                let med = median(&values);
                for (rid, s) in &scores {
                    let bonus = if *s > med {
                        1.0
                    } else if *s == med {
                        0.5
                    } else {
                        0.0
                    };
                    *w.entry(*rid).or_insert(0.0) += bonus;
                }
            }
            for (rid, s) in &scores {
                *pf.entry(*rid).or_insert(0.0) += s;
            }
        }

        let seeds = seed_playoffs(&w, &pf, playoff_teams, divisions);
        for (i, rid) in seeds.iter().enumerate() {
            *made.entry(*rid).or_insert(0) += 1;
            if (i as i64) < byes {
                *bye.entry(*rid).or_insert(0) += 1;
            }
            if i == 0 {
                *top.entry(*rid).or_insert(0) += 1;
            }
        }
        for rid in &rosters {
            *total_wins.entry(*rid).or_insert(0.0) += w.get(rid).copied().unwrap_or(0.0);
        }
    }

    let sims_f = sims as f64;
    let mut rows: Vec<PlayoffOddsRow> = rosters
        .iter()
        .map(|rid| {
            let w = wins[rid];
            let losses = games[rid] as f64 - w;
            PlayoffOddsRow {
                roster_id: *rid,
                record: odds_record(w, losses),
                projected_wins: total_wins[rid] / sims_f,
                playoff_pct: made[rid] as f64 / sims_f,
                bye_pct: bye[rid] as f64 / sims_f,
                top_seed_pct: top[rid] as f64 / sims_f,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.playoff_pct
            .partial_cmp(&a.playoff_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.projected_wins
                    .partial_cmp(&a.projected_wins)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then(a.roster_id.cmp(&b.roster_id))
    });
    rows
}

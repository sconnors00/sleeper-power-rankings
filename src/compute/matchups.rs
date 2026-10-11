//! One week at a time: pairings, awards, all-play, head-to-head, the power
//! rankings, position ranks, lineup efficiency, and the week summaries that
//! carry the cumulative state from week to week.

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;

use super::{format_record, sum_f64, POSITIONS};
use crate::models::{
    LineupRow, Matchup, PlayerMove, PlayerScore, PositionRankRow, PowerRankRow, TeamRosterMoves,
    WeekAwards, WeekSummary, Weights,
};
use crate::raw::{PlayerMap, RawMatchup, RawRoster, RawTransaction};
use crate::util::{median, round2};

pub type RosterMap = HashMap<i64, RawRoster>;

pub fn parse_week_matchups(
    raw: &[RawMatchup],
    week: i64,
    rosters_by_id: &RosterMap,
    players: &PlayerMap,
) -> Vec<Matchup> {
    let mut by_matchup: IndexMap<Option<i64>, Vec<&RawMatchup>> = IndexMap::new();
    for m in raw {
        by_matchup.entry(m.matchup_id).or_default().push(m);
    }

    let mut matchups = Vec::new();
    for pair in by_matchup.values() {
        let roster_ids: Vec<i64> = pair.iter().map(|m| m.roster_id).collect();
        for m in pair {
            let opponent = roster_ids.iter().copied().find(|rid| *rid != m.roster_id);
            let roster = rosters_by_id.get(&m.roster_id);
            let reserve: HashSet<&str> = roster
                .map(|r| r.reserve.iter().map(String::as_str).collect())
                .unwrap_or_default();
            let taxi: HashSet<&str> = roster
                .map(|r| r.taxi.iter().map(String::as_str).collect())
                .unwrap_or_default();

            let starter_ids = &m.starters;
            let starters_points = &m.starters_points;
            let starters: Vec<PlayerScore> = starter_ids
                .iter()
                .enumerate()
                .filter(|(_, pid)| pid.as_str() != "0")
                .map(|(i, pid)| {
                    let points = starters_points.get(i).copied().flatten().unwrap_or(0.0);
                    player_points(pid, points, players)
                })
                .collect();

            let starter_set: HashSet<&str> = starter_ids
                .iter()
                .map(String::as_str)
                .filter(|pid| *pid != "0")
                .collect();
            let bench: Vec<PlayerScore> = m
                .players
                .iter()
                .filter(|pid| {
                    let pid = pid.as_str();
                    !starter_set.contains(pid) && !reserve.contains(pid) && !taxi.contains(pid)
                })
                .map(|pid| {
                    let points = m.players_points.get(pid).copied().flatten().unwrap_or(0.0);
                    player_points(pid, points, players)
                })
                .collect();

            let team_points = m.custom_points.or(m.points).unwrap_or(0.0);
            matchups.push(Matchup {
                week,
                matchup_id: m.matchup_id.unwrap_or(-1),
                roster_id: m.roster_id,
                opponent_roster_id: opponent,
                team_points,
                starters,
                bench,
            });
        }
    }
    matchups
}

fn player_points(pid: &str, points: f64, players: &PlayerMap) -> PlayerScore {
    super::player_score(pid, points, players)
}

/// Roster ids at the extreme value, every tie included, and that value. An
/// empty list gives `([], 0.0)`.
fn tied_extreme(items: &[(i64, f64)], want_max: bool) -> (Vec<i64>, f64) {
    if items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let best = items.iter().map(|(_, v)| *v).fold(items[0].1, |acc, v| {
        if want_max {
            acc.max(v)
        } else {
            acc.min(v)
        }
    });
    (
        items
            .iter()
            .filter(|(_, v)| *v == best)
            .map(|(id, _)| *id)
            .collect(),
        best,
    )
}

fn by_matchup_id(matchups: &[Matchup]) -> IndexMap<i64, Vec<&Matchup>> {
    let mut groups: IndexMap<i64, Vec<&Matchup>> = IndexMap::new();
    for m in matchups {
        groups.entry(m.matchup_id).or_default().push(m);
    }
    groups
}

pub fn compute_week_awards(matchups: &[Matchup]) -> WeekAwards {
    let team_scores: Vec<(i64, f64)> = matchups
        .iter()
        .map(|m| (m.roster_id, m.team_points))
        .collect();
    let (highest_ids, highest_score) = tied_extreme(&team_scores, true);
    let (lowest_ids, lowest_score) = tied_extreme(&team_scores, false);

    let mut bench_points_by_roster = BTreeMap::new();
    let mut all_bench: Vec<(i64, PlayerScore)> = Vec::new();
    for m in matchups {
        bench_points_by_roster.insert(m.roster_id, sum_f64(m.bench.iter().map(|p| p.points)));
        all_bench.extend(m.bench.iter().map(|p| (m.roster_id, p.clone())));
    }
    let (best_bench_points, best_bench) = best_by_points(&all_bench, true);

    let mut all_starters: Vec<(i64, PlayerScore)> = Vec::new();
    for m in matchups {
        all_starters.extend(m.starters.iter().map(|p| (m.roster_id, p.clone())));
    }
    let (best_starter_points, best_starter) = best_by_points(&all_starters, true);
    let (worst_starter_points, worst_starter) = best_by_points(&all_starters, false);
    let worst_starter_zero = worst_starter_points == 0.0;

    let mut margins: Vec<(i64, f64)> = Vec::new();
    for (mid, pair) in by_matchup_id(matchups) {
        if pair.len() == 2 {
            margins.push((mid, (pair[0].team_points - pair[1].team_points).abs()));
        }
    }
    let (closest_ids, closest_margin) = tied_extreme(&margins, false);
    let (blowout_ids, blowout_margin) = tied_extreme(&margins, true);

    WeekAwards {
        highest_score_roster_ids: highest_ids,
        highest_score,
        lowest_score_roster_ids: lowest_ids,
        lowest_score,
        best_bench,
        best_bench_points,
        bench_points_by_roster,
        best_starter,
        best_starter_points,
        worst_starter,
        worst_starter_points,
        worst_starter_zero,
        closest_matchup_ids: closest_ids,
        closest_margin,
        biggest_blowout_matchup_ids: blowout_ids,
        biggest_blowout_margin: blowout_margin,
    }
}

/// The highest (or lowest) points among `(roster, player)` pairs, with every
/// player who tied for it. Empty input gives `(0.0, [])`.
pub fn best_by_points(
    items: &[(i64, PlayerScore)],
    want_max: bool,
) -> (f64, Vec<(i64, PlayerScore)>) {
    if items.is_empty() {
        return (0.0, Vec::new());
    }
    let mut best = items[0].1.points;
    for (_, p) in items {
        best = if want_max {
            best.max(p.points)
        } else {
            best.min(p.points)
        };
    }
    (
        best,
        items
            .iter()
            .filter(|(_, p)| p.points == best)
            .cloned()
            .collect(),
    )
}

/// Roster id -> the number of other teams it outscored this week.
pub fn compute_allplay_week(matchups: &[Matchup]) -> BTreeMap<i64, i64> {
    let scores: Vec<(i64, f64)> = matchups
        .iter()
        .map(|m| (m.roster_id, m.team_points))
        .collect();
    scores
        .iter()
        .map(|(rid, pts)| {
            let count = scores
                .iter()
                .filter(|(other, other_pts)| other != rid && pts > other_pts)
                .count() as i64;
            (*rid, count)
        })
        .collect()
}

/// Roster id -> all-play win-equivalents this week, ties worth 0.5.
pub fn compute_allplay_week_equiv(matchups: &[Matchup]) -> BTreeMap<i64, f64> {
    let scores: Vec<(i64, f64)> = matchups
        .iter()
        .map(|m| (m.roster_id, m.team_points))
        .collect();
    scores
        .iter()
        .map(|(rid, pts)| {
            let mut equiv = 0.0;
            for (other, other_pts) in &scores {
                if other == rid {
                    continue;
                }
                if pts > other_pts {
                    equiv += 1.0;
                } else if pts == other_pts {
                    equiv += 0.5;
                }
            }
            (*rid, equiv)
        })
        .collect()
}

/// Roster id -> (win-equivalent this week, games this week). Head-to-head from
/// the pairing, plus (with `league_average_match`) a game against the week's
/// median score, mirroring how Sleeper computes the official record.
pub fn compute_weekly_head_to_head(
    matchups: &[Matchup],
    league_average_match: bool,
) -> BTreeMap<i64, (f64, i64)> {
    let mut result: BTreeMap<i64, (f64, i64)> = BTreeMap::new();
    for pair in by_matchup_id(matchups).values() {
        if pair.len() != 2 {
            continue;
        }
        let (a, b) = (pair[0], pair[1]);
        if a.team_points > b.team_points {
            result.insert(a.roster_id, (1.0, 1));
            result.insert(b.roster_id, (0.0, 1));
        } else if a.team_points < b.team_points {
            result.insert(a.roster_id, (0.0, 1));
            result.insert(b.roster_id, (1.0, 1));
        } else {
            result.insert(a.roster_id, (0.5, 1));
            result.insert(b.roster_id, (0.5, 1));
        }
    }

    if league_average_match {
        let points: Vec<f64> = matchups.iter().map(|m| m.team_points).collect();
        let med = median(&points);
        for m in matchups {
            let (win_equiv, games) = result.get(&m.roster_id).copied().unwrap_or((0.0, 0));
            let bonus = if m.team_points > med {
                1.0
            } else if m.team_points < med {
                0.0
            } else {
                0.5
            };
            result.insert(m.roster_id, (win_equiv + bonus, games + 1));
        }
    }
    result
}

/// Cumulative state going into one week's power rankings.
pub struct Cumulative<'a> {
    pub win: &'a BTreeMap<i64, f64>,
    pub games: &'a BTreeMap<i64, i64>,
    pub allplay_win: &'a BTreeMap<i64, f64>,
    pub allplay_games: &'a BTreeMap<i64, i64>,
    pub pf: &'a BTreeMap<i64, f64>,
    pub last_n_scores: &'a BTreeMap<i64, Vec<f64>>,
}

pub fn compute_power_rankings(
    roster_ids: &[i64],
    cum: &Cumulative,
    weights: &Weights,
    prev_ranks: Option<&BTreeMap<i64, i64>>,
) -> Vec<PowerRankRow> {
    let get_f = |map: &BTreeMap<i64, f64>, rid: i64| map.get(&rid).copied().unwrap_or(0.0);
    let get_i = |map: &BTreeMap<i64, i64>, rid: i64| map.get(&rid).copied().unwrap_or(0);

    let win_pct: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let games = get_i(cum.games, rid);
            let pct = if games != 0 {
                get_f(cum.win, rid) / games as f64
            } else {
                0.0
            };
            (rid, pct)
        })
        .collect();
    let allplay_pct: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let games = get_i(cum.allplay_games, rid);
            let pct = if games != 0 {
                get_f(cum.allplay_win, rid) / games as f64
            } else {
                0.0
            };
            (rid, pct)
        })
        .collect();

    let pf_values: Vec<f64> = roster_ids.iter().map(|&rid| get_f(cum.pf, rid)).collect();
    let pf_min = pf_values.iter().copied().fold(f64::INFINITY, f64::min);
    let pf_max = pf_values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let pf_norm: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let v = get_f(cum.pf, rid);
            (
                rid,
                if pf_max > pf_min {
                    (v - pf_min) / (pf_max - pf_min)
                } else {
                    0.5
                },
            )
        })
        .collect();

    let form_avg: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let last = cum
                .last_n_scores
                .get(&rid)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let avg = if last.is_empty() {
                0.0
            } else {
                sum_f64(last.iter().copied()) / last.len() as f64
            };
            (rid, avg)
        })
        .collect();
    let form_min = form_avg.values().copied().fold(f64::INFINITY, f64::min);
    let form_max = form_avg.values().copied().fold(f64::NEG_INFINITY, f64::max);
    let form_norm: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let v = form_avg[&rid];
            (
                rid,
                if form_max > form_min {
                    (v - form_min) / (form_max - form_min)
                } else {
                    0.5
                },
            )
        })
        .collect();

    let scores: BTreeMap<i64, f64> = roster_ids
        .iter()
        .map(|&rid| {
            let score = weights.win_pct * win_pct[&rid]
                + weights.allplay_pct * allplay_pct[&rid]
                + weights.pf_norm * pf_norm[&rid]
                + weights.form_norm * form_norm[&rid];
            (rid, score)
        })
        .collect();

    let mut ranked: Vec<i64> = roster_ids.to_vec();
    ranked.sort_by(|a, b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                get_f(cum.pf, *b)
                    .partial_cmp(&get_f(cum.pf, *a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    ranked
        .iter()
        .enumerate()
        .map(|(i, &rid)| {
            let rank = i as i64 + 1;
            let movement = prev_ranks
                .and_then(|prev| prev.get(&rid))
                .map(|prev| prev - rank);
            let wins = get_f(cum.win, rid);
            let losses_ties = get_i(cum.games, rid) as f64 - wins;
            let ap_wins = get_f(cum.allplay_win, rid);
            let ap_total = get_i(cum.allplay_games, rid) as f64;
            PowerRankRow {
                roster_id: rid,
                rank,
                score: scores[&rid],
                win_pct: win_pct[&rid],
                allplay_pct: allplay_pct[&rid],
                pf_norm: pf_norm[&rid],
                form_norm: form_norm[&rid],
                luck: win_pct[&rid] - allplay_pct[&rid],
                record: format_record(wins, losses_ties),
                allplay_record: format!("{:.1}-{:.1}", ap_wins, ap_total - ap_wins),
                movement,
            }
        })
        .collect()
}

/// Completed adds and drops, grouped by roster. Failed transactions (an
/// outbid claim, a vetoed trade) never happened, so they're excluded.
pub fn compute_roster_moves(raw: &[RawTransaction], players: &PlayerMap) -> Vec<TeamRosterMoves> {
    let mut by_roster: BTreeMap<i64, TeamRosterMoves> = BTreeMap::new();
    let bucket = |map: &mut BTreeMap<i64, TeamRosterMoves>, rid: i64| {
        map.entry(rid).or_insert_with(|| TeamRosterMoves {
            roster_id: rid,
            added: Vec::new(),
            dropped: Vec::new(),
        });
    };
    for tx in raw
        .iter()
        .filter(|t| t.status.as_deref() == Some("complete"))
    {
        let move_type = tx.kind.clone().unwrap_or_else(|| "waiver".to_string());
        let faab = if move_type == "waiver" {
            tx.settings.waiver_bid
        } else {
            None
        };
        for (pid, rid) in &tx.adds {
            bucket(&mut by_roster, *rid);
            by_roster
                .get_mut(rid)
                .expect("bucket")
                .added
                .push(PlayerMove {
                    player: super::player_score(pid, 0.0, players),
                    move_type: move_type.clone(),
                    faab,
                });
        }
        for (pid, rid) in &tx.drops {
            bucket(&mut by_roster, *rid);
            by_roster
                .get_mut(rid)
                .expect("bucket")
                .dropped
                .push(PlayerMove {
                    player: super::player_score(pid, 0.0, players),
                    move_type: move_type.clone(),
                    faab: None,
                });
        }
    }
    by_roster.into_values().collect()
}

/// Each team's starter points at each position, ranked against the league.
/// Starters count toward their own position whatever slot they filled.
pub fn compute_position_ranks(matchups: &[Matchup]) -> Vec<PositionRankRow> {
    let mut totals: BTreeMap<i64, BTreeMap<String, f64>> = BTreeMap::new();
    for m in matchups {
        let mut by_pos: BTreeMap<String, f64> =
            POSITIONS.iter().map(|p| (p.to_string(), 0.0)).collect();
        for p in &m.starters {
            if let Some(total) = by_pos.get_mut(&p.position) {
                *total += p.points;
            }
        }
        // Round before ranking so float summation order can't split a real tie.
        let rounded = by_pos
            .into_iter()
            .map(|(pos, pts)| (pos, round2(pts)))
            .collect();
        totals.insert(m.roster_id, rounded);
    }

    totals
        .iter()
        .map(|(rid, mine)| {
            let ranks = POSITIONS
                .iter()
                .map(|pos| {
                    let rank = 1 + totals
                        .values()
                        .filter(|other| other[*pos] > mine[*pos])
                        .count() as i64;
                    (pos.to_string(), rank)
                })
                .collect();
            PositionRankRow {
                roster_id: *rid,
                points: mine.clone(),
                ranks,
            }
        })
        .collect()
}

/// The highest-scoring legal lineup these players could have formed.
///
/// Fixed slots take the best player left at their position. Flex slots are
/// then searched exhaustively, which stays exact when flex eligibilities
/// overlap. Every slot with an eligible player gets filled, and on equal
/// points the players the manager actually started are kept.
pub fn best_lineup(
    players: &[PlayerScore],
    roster_positions: &[String],
    starter_ids: &HashSet<&str>,
) -> Vec<PlayerScore> {
    let slots: Vec<&str> = roster_positions
        .iter()
        .map(String::as_str)
        .filter(|s| super::is_starting_slot(s))
        .collect();
    let flexes: Vec<&[&str]> = slots
        .iter()
        .filter_map(|s| super::flex_eligibility(s))
        .collect();
    let fixed: Vec<&str> = slots
        .iter()
        .copied()
        .filter(|s| super::flex_eligibility(s).is_none())
        .collect();

    let mut remaining: Vec<PlayerScore> = players.to_vec();
    // Highest points first; a stable sort keeps equal keys in input order.
    remaining.sort_by(|a, b| {
        b.points
            .partial_cmp(&a.points)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                starter_ids
                    .contains(b.player_id.as_str())
                    .cmp(&starter_ids.contains(a.player_id.as_str()))
            })
    });

    let mut lineup: Vec<PlayerScore> = Vec::new();
    for slot in fixed {
        if let Some(idx) = remaining.iter().position(|p| p.position == slot) {
            lineup.push(remaining.remove(idx));
        }
    }

    // Only the top len(flexes) players at a position can ever reach a flex slot.
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    let mut candidates: Vec<PlayerScore> = Vec::new();
    for p in &remaining {
        let eligible = flexes.iter().any(|e| e.contains(&p.position.as_str()));
        let count = taken.entry(p.position.clone()).or_insert(0);
        if eligible && *count < flexes.len() {
            *count += 1;
            candidates.push(p.clone());
        }
    }

    let mut search = FlexSearch {
        candidates: &candidates,
        flexes: &flexes,
        starter_ids,
        best_key: (f64::NEG_INFINITY, 0),
        best: Vec::new(),
    };
    search.run(0, &mut Vec::new());
    lineup.extend(search.best);
    lineup
}

struct FlexSearch<'a, 'b> {
    candidates: &'a [PlayerScore],
    flexes: &'a [&'a [&'a str]],
    starter_ids: &'a HashSet<&'b str>,
    best_key: (f64, usize),
    best: Vec<PlayerScore>,
}

impl FlexSearch<'_, '_> {
    fn run(&mut self, i: usize, picked: &mut Vec<usize>) {
        if i == self.flexes.len() {
            let picks: Vec<&PlayerScore> = picked.iter().map(|&j| &self.candidates[j]).collect();
            let total = round2(sum_f64(picks.iter().map(|p| p.points)));
            let starters = picks
                .iter()
                .filter(|p| self.starter_ids.contains(p.player_id.as_str()))
                .count();
            let key = (total, starters);
            if key.0 > self.best_key.0 || (key.0 == self.best_key.0 && key.1 > self.best_key.1) {
                self.best_key = key;
                self.best = picks.into_iter().cloned().collect();
            }
            return;
        }
        let eligible: Vec<usize> = (0..self.candidates.len())
            .filter(|j| {
                self.flexes[i].contains(&self.candidates[*j].position.as_str())
                    && !picked.contains(j)
            })
            .collect();
        if eligible.is_empty() {
            self.run(i + 1, picked);
        }
        for j in eligible {
            picked.push(j);
            self.run(i + 1, picked);
            picked.pop();
        }
    }
}

/// Match each player the best lineup adds with the starter he displaces.
/// Same-position swaps are matched first; leftovers pair across positions.
pub fn pair_swaps(
    started: &[PlayerScore],
    sat: &[PlayerScore],
) -> Vec<(PlayerScore, Option<PlayerScore>)> {
    let mut ins: Vec<PlayerScore> = started.to_vec();
    ins.sort_by(|a, b| {
        b.points
            .partial_cmp(&a.points)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut outs: Vec<PlayerScore> = sat.to_vec();
    outs.sort_by(|a, b| {
        a.points
            .partial_cmp(&b.points)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut pairs: Vec<(PlayerScore, Option<PlayerScore>)> = Vec::new();
    let mut paired_in = vec![false; ins.len()];
    for (i, p) in ins.iter().enumerate() {
        if let Some(idx) = outs.iter().position(|o| o.position == p.position) {
            pairs.push((p.clone(), Some(outs.remove(idx))));
            paired_in[i] = true;
        }
    }
    let leftovers: Vec<PlayerScore> = ins
        .iter()
        .enumerate()
        .filter(|(i, _)| !paired_in[*i])
        .map(|(_, p)| p.clone())
        .collect();
    for p in leftovers {
        let out = if outs.is_empty() {
            None
        } else {
            Some(outs.remove(0))
        };
        pairs.push((p, out));
    }

    let gain = |pr: &(PlayerScore, Option<PlayerScore>)| {
        pr.0.points - pr.1.as_ref().map_or(0.0, |o| o.points)
    };
    pairs.sort_by(|a, b| {
        gain(b)
            .partial_cmp(&gain(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pairs
}

/// Each team's lineup against the best one its own roster allowed.
pub fn compute_lineup_efficiency(
    matchups: &[Matchup],
    roster_positions: &[String],
) -> Vec<LineupRow> {
    let mut sorted: Vec<&Matchup> = matchups.iter().collect();
    sorted.sort_by_key(|m| m.roster_id);
    sorted
        .into_iter()
        .map(|m| {
            let starter_ids: HashSet<&str> =
                m.starters.iter().map(|p| p.player_id.as_str()).collect();
            let mut pool = m.starters.clone();
            pool.extend(m.bench.iter().cloned());
            let optimal = best_lineup(&pool, roster_positions, &starter_ids);
            let optimal_ids: HashSet<&str> = optimal.iter().map(|p| p.player_id.as_str()).collect();
            let started: Vec<PlayerScore> = optimal
                .iter()
                .filter(|p| !starter_ids.contains(p.player_id.as_str()))
                .cloned()
                .collect();
            let sat: Vec<PlayerScore> = m
                .starters
                .iter()
                .filter(|p| !optimal_ids.contains(p.player_id.as_str()))
                .cloned()
                .collect();
            let actual = round2(sum_f64(m.starters.iter().map(|p| p.points)));
            let best_total = round2(sum_f64(optimal.iter().map(|p| p.points)));

            let mut should_have_started = started.clone();
            should_have_started.sort_by(|a, b| {
                b.points
                    .partial_cmp(&a.points)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut should_have_sat = sat.clone();
            should_have_sat.sort_by(|a, b| {
                a.points
                    .partial_cmp(&b.points)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            LineupRow {
                roster_id: m.roster_id,
                actual,
                optimal: best_total,
                should_have_started,
                should_have_sat,
                swaps: pair_swaps(&started, &sat),
                left_on_bench: best_total - actual,
                efficiency: if best_total > 0.0 {
                    actual / best_total
                } else {
                    1.0
                },
            }
        })
        .collect()
}

/// Overwrite each row's record with the official record from `rosters.settings`.
pub fn apply_official_records(power_rankings: &mut [PowerRankRow], rosters_by_id: &RosterMap) {
    for row in power_rankings {
        if let Some(roster) = rosters_by_id.get(&row.roster_id) {
            row.record = official_record_string(roster);
        }
    }
}

/// The record as Sleeper reports it (`rosters.settings`), for display only.
pub fn official_record_string(roster: &RawRoster) -> String {
    let s = &roster.settings;
    if s.ties != 0 {
        format!("{}-{}-{}", s.wins, s.losses, s.ties)
    } else {
        format!("{}-{}", s.wins, s.losses)
    }
}

/// Awards and scores for the in-progress week, with no power rankings.
pub fn build_provisional_week_summary(
    week: i64,
    raw_matchups: &[RawMatchup],
    raw_transactions: &[RawTransaction],
    rosters_by_id: &RosterMap,
    players: &PlayerMap,
    roster_positions: &[String],
) -> WeekSummary {
    let matchups = parse_week_matchups(raw_matchups, week, rosters_by_id, players);
    let awards = compute_week_awards(&matchups);
    WeekSummary {
        week,
        allplay_week_wins: compute_allplay_week(&matchups),
        roster_moves: compute_roster_moves(raw_transactions, players),
        position_ranks: compute_position_ranks(&matchups),
        lineups: if roster_positions.is_empty() {
            Vec::new()
        } else {
            compute_lineup_efficiency(&matchups, roster_positions)
        },
        matchups,
        awards,
        power_rankings: Vec::new(),
    }
}

/// Every week, in order, with the cumulative state carried forward.
#[allow(clippy::too_many_arguments)]
pub fn build_week_summaries(
    roster_ids: &[i64],
    weeks_raw_matchups: &BTreeMap<i64, Vec<RawMatchup>>,
    weeks_raw_transactions: &BTreeMap<i64, Vec<RawTransaction>>,
    rosters_by_id: &RosterMap,
    players: &PlayerMap,
    league_average_match: bool,
    weights: &Weights,
    form_window: i64,
    roster_positions: Option<&[String]>,
) -> Vec<WeekSummary> {
    let mut cumulative_win: BTreeMap<i64, f64> = roster_ids.iter().map(|r| (*r, 0.0)).collect();
    let mut cumulative_games: BTreeMap<i64, i64> = roster_ids.iter().map(|r| (*r, 0)).collect();
    let mut cumulative_allplay_win: BTreeMap<i64, f64> =
        roster_ids.iter().map(|r| (*r, 0.0)).collect();
    let mut cumulative_allplay_games: BTreeMap<i64, i64> =
        roster_ids.iter().map(|r| (*r, 0)).collect();
    let mut cumulative_pf: BTreeMap<i64, f64> = roster_ids.iter().map(|r| (*r, 0.0)).collect();
    let mut score_history: BTreeMap<i64, Vec<f64>> =
        roster_ids.iter().map(|r| (*r, Vec::new())).collect();

    let mut prev_ranks: Option<BTreeMap<i64, i64>> = None;
    let mut summaries = Vec::new();

    for (&week, raw) in weeks_raw_matchups {
        let matchups = parse_week_matchups(raw, week, rosters_by_id, players);
        let awards = compute_week_awards(&matchups);
        let allplay_week = compute_allplay_week(&matchups);
        let allplay_equiv = compute_allplay_week_equiv(&matchups);
        let h2h = compute_weekly_head_to_head(&matchups, league_average_match);

        for m in &matchups {
            let rid = m.roster_id;
            *cumulative_pf.entry(rid).or_insert(0.0) += m.team_points;
            score_history.entry(rid).or_default().push(m.team_points);

            let (win_equiv, games) = h2h.get(&rid).copied().unwrap_or((0.0, 0));
            *cumulative_win.entry(rid).or_insert(0.0) += win_equiv;
            *cumulative_games.entry(rid).or_insert(0) += games;

            *cumulative_allplay_win.entry(rid).or_insert(0.0) +=
                allplay_equiv.get(&rid).copied().unwrap_or(0.0);
            *cumulative_allplay_games.entry(rid).or_insert(0) += matchups.len() as i64 - 1;
        }

        let last_n_scores: BTreeMap<i64, Vec<f64>> = roster_ids
            .iter()
            .map(|rid| {
                (
                    *rid,
                    last_n(
                        score_history.get(rid).map_or(&[], Vec::as_slice),
                        form_window,
                    ),
                )
            })
            .collect();
        let cum = Cumulative {
            win: &cumulative_win,
            games: &cumulative_games,
            allplay_win: &cumulative_allplay_win,
            allplay_games: &cumulative_allplay_games,
            pf: &cumulative_pf,
            last_n_scores: &last_n_scores,
        };
        let power_rankings = compute_power_rankings(roster_ids, &cum, weights, prev_ranks.as_ref());
        prev_ranks = Some(
            power_rankings
                .iter()
                .map(|row| (row.roster_id, row.rank))
                .collect(),
        );

        let roster_moves = compute_roster_moves(
            weeks_raw_transactions.get(&week).map_or(&[], Vec::as_slice),
            players,
        );
        summaries.push(WeekSummary {
            week,
            position_ranks: compute_position_ranks(&matchups),
            lineups: match roster_positions {
                Some(slots) if !slots.is_empty() => compute_lineup_efficiency(&matchups, slots),
                _ => Vec::new(),
            },
            matchups,
            awards,
            allplay_week_wins: allplay_week,
            power_rankings,
            roster_moves,
        });
    }
    summaries
}

/// The last `window` scores, as Python's `scores[-window:]` slices them. A
/// window of 0 keeps the whole history, which is what Python does too.
fn last_n(scores: &[f64], window: i64) -> Vec<f64> {
    if window == 0 {
        return scores.to_vec();
    }
    let start = scores.len().saturating_sub(window.unsigned_abs() as usize);
    scores[start..].to_vec()
}

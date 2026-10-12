//! The season scoring board: the weekly log, crowns, the points-for
//! leaderboard, the season's records, and lineup efficiency over the season.

use std::collections::BTreeMap;

use indexmap::IndexMap;

use crate::models::{
    GameRecord, Matchup, PfLeaderboardRow, PlayerScore, SeasonBoard, SeasonBoardEntry,
    SeasonLineupRow, SeasonRecords, Team, WeekSummary,
};

type Holders = (Vec<i64>, i64, f64);
type PlayerHolders = (Vec<(i64, PlayerScore)>, i64);

pub fn build_season_board(weeks: &[WeekSummary], teams: &BTreeMap<i64, Team>) -> SeasonBoard {
    let mut log = Vec::new();
    let mut crown_counts: BTreeMap<i64, i64> = teams.keys().map(|rid| (*rid, 0)).collect();
    let mut pf_totals: BTreeMap<i64, f64> = teams.keys().map(|rid| (*rid, 0.0)).collect();
    let mut pf_against_totals: BTreeMap<i64, f64> = teams.keys().map(|rid| (*rid, 0.0)).collect();
    let mut bench_totals: BTreeMap<i64, f64> = teams.keys().map(|rid| (*rid, 0.0)).collect();
    let mut scores_by_roster: BTreeMap<i64, BTreeMap<i64, f64>> =
        teams.keys().map(|rid| (*rid, BTreeMap::new())).collect();

    let mut highest_team: Holders = (Vec::new(), 0, f64::NEG_INFINITY);
    let mut lowest_team: Holders = (Vec::new(), 0, f64::INFINITY);
    let mut highest_starter: PlayerHolders = (Vec::new(), 0);
    let mut highest_starter_pts = f64::NEG_INFINITY;
    let mut lowest_starter: PlayerHolders = (Vec::new(), 0);
    let mut lowest_starter_pts = f64::INFINITY;
    let mut highest_bench: PlayerHolders = (Vec::new(), 0);
    let mut highest_bench_pts = f64::NEG_INFINITY;
    let mut closest_games: Vec<GameRecord> = Vec::new();
    let mut closest_margin = f64::INFINITY;
    let mut biggest_blowouts: Vec<GameRecord> = Vec::new();
    let mut biggest_blowout_margin = f64::NEG_INFINITY;

    for wk in weeks {
        let mut by_matchup: IndexMap<i64, Vec<&Matchup>> = IndexMap::new();
        for m in &wk.matchups {
            by_matchup.entry(m.matchup_id).or_default().push(m);
        }

        let awards = &wk.awards;
        if !awards.closest_matchup_ids.is_empty() {
            if awards.closest_margin < closest_margin {
                closest_margin = awards.closest_margin;
                closest_games = Vec::new();
            }
            if awards.closest_margin == closest_margin {
                closest_games.extend(
                    awards
                        .closest_matchup_ids
                        .iter()
                        .filter_map(|mid| game_record(&by_matchup, wk.week, *mid, closest_margin)),
                );
            }
        }
        if !awards.biggest_blowout_matchup_ids.is_empty() {
            if awards.biggest_blowout_margin > biggest_blowout_margin {
                biggest_blowout_margin = awards.biggest_blowout_margin;
                biggest_blowouts = Vec::new();
            }
            if awards.biggest_blowout_margin == biggest_blowout_margin {
                biggest_blowouts.extend(awards.biggest_blowout_matchup_ids.iter().filter_map(
                    |mid| game_record(&by_matchup, wk.week, *mid, biggest_blowout_margin),
                ));
            }
        }

        let top_ids = awards.highest_score_roster_ids.clone();
        log.push(SeasonBoardEntry {
            week: wk.week,
            top_scorer_roster_ids: top_ids.clone(),
            top_score: awards.highest_score,
        });
        for rid in &top_ids {
            *crown_counts.entry(*rid).or_insert(0) += 1;
        }

        for (rid, bench_pts) in &awards.bench_points_by_roster {
            *bench_totals.entry(*rid).or_insert(0.0) += bench_pts;
        }

        for m in &wk.matchups {
            *pf_totals.entry(m.roster_id).or_insert(0.0) += m.team_points;
            scores_by_roster
                .entry(m.roster_id)
                .or_default()
                .insert(wk.week, m.team_points);
            if let Some(opp_id) = m.opponent_roster_id {
                if let Some(opp) = wk.matchups.iter().find(|x| x.roster_id == opp_id) {
                    *pf_against_totals.entry(m.roster_id).or_insert(0.0) += opp.team_points;
                }
            }

            let pts = m.team_points;
            if pts > highest_team.2 {
                highest_team = (vec![m.roster_id], wk.week, pts);
            } else if pts == highest_team.2 {
                highest_team = (with_pushed(&highest_team.0, m.roster_id), wk.week, pts);
            }
            if pts < lowest_team.2 {
                lowest_team = (vec![m.roster_id], wk.week, pts);
            } else if pts == lowest_team.2 {
                lowest_team = (with_pushed(&lowest_team.0, m.roster_id), wk.week, pts);
            }

            for p in &m.starters {
                if p.points > highest_starter_pts {
                    highest_starter_pts = p.points;
                    highest_starter = (vec![(m.roster_id, p.clone())], wk.week);
                } else if p.points == highest_starter_pts {
                    highest_starter = (
                        with_pushed_player(&highest_starter.0, m.roster_id, p),
                        wk.week,
                    );
                }
                if p.points < lowest_starter_pts {
                    lowest_starter_pts = p.points;
                    lowest_starter = (vec![(m.roster_id, p.clone())], wk.week);
                } else if p.points == lowest_starter_pts {
                    lowest_starter = (
                        with_pushed_player(&lowest_starter.0, m.roster_id, p),
                        wk.week,
                    );
                }
            }
            for p in &m.bench {
                if p.points > highest_bench_pts {
                    highest_bench_pts = p.points;
                    highest_bench = (vec![(m.roster_id, p.clone())], wk.week);
                } else if p.points == highest_bench_pts {
                    highest_bench = (
                        with_pushed_player(&highest_bench.0, m.roster_id, p),
                        wk.week,
                    );
                }
            }
        }
    }

    let mut pf_leaderboard: Vec<PfLeaderboardRow> = Vec::new();
    for rid in teams.keys() {
        let Some(weekly) = scores_by_roster.get(rid).filter(|w| !w.is_empty()) else {
            continue;
        };
        let (best_week, best_score) = extreme_week(weekly, true);
        let (worst_week, worst_score) = extreme_week(weekly, false);
        let pf = pf_totals.get(rid).copied().unwrap_or(0.0);
        pf_leaderboard.push(PfLeaderboardRow {
            roster_id: *rid,
            pf,
            pa: pf_against_totals.get(rid).copied().unwrap_or(0.0),
            avg: pf / weekly.len() as f64,
            best_week,
            best_week_score: best_score,
            worst_week,
            worst_week_score: worst_score,
            bench_points: bench_totals.get(rid).copied().unwrap_or(0.0),
        });
    }
    pf_leaderboard.sort_by(|a, b| b.pf.partial_cmp(&a.pf).unwrap_or(std::cmp::Ordering::Equal));

    let records = SeasonRecords {
        highest_team_score: highest_team,
        lowest_team_score: if weeks.is_empty() {
            (Vec::new(), 0, 0.0)
        } else {
            lowest_team
        },
        highest_starter,
        lowest_starter,
        highest_bench_player: highest_bench,
        closest_games,
        biggest_blowouts,
    };

    SeasonBoard {
        log,
        crown_counts,
        pf_leaderboard,
        records,
        lineup_leaderboard: build_lineup_leaderboard(weeks),
    }
}

fn with_pushed(ids: &[i64], rid: i64) -> Vec<i64> {
    let mut out = ids.to_vec();
    out.push(rid);
    out
}

fn with_pushed_player(
    holders: &[(i64, PlayerScore)],
    rid: i64,
    p: &PlayerScore,
) -> Vec<(i64, PlayerScore)> {
    let mut out = holders.to_vec();
    out.push((rid, p.clone()));
    out
}

/// The week and score of the best (or worst) entry; the first week wins a tie.
fn extreme_week(weekly: &BTreeMap<i64, f64>, want_max: bool) -> (i64, f64) {
    let mut best: Option<(i64, f64)> = None;
    for (&week, &score) in weekly {
        let better = match best {
            None => true,
            Some((_, current)) => {
                if want_max {
                    score > current
                } else {
                    score < current
                }
            }
        };
        if better {
            best = Some((week, score));
        }
    }
    best.unwrap_or((0, 0.0))
}

fn game_record(
    by_matchup: &IndexMap<i64, Vec<&Matchup>>,
    week: i64,
    matchup_id: i64,
    margin: f64,
) -> Option<GameRecord> {
    let pair = by_matchup.get(&matchup_id)?;
    if pair.len() != 2 {
        return None;
    }
    let (a, b) = (pair[0], pair[1]);
    Some(GameRecord {
        week,
        roster_a: a.roster_id,
        points_a: a.team_points,
        roster_b: b.roster_id,
        points_b: b.team_points,
        margin,
    })
}

pub fn build_lineup_leaderboard(weeks: &[WeekSummary]) -> Vec<SeasonLineupRow> {
    let mut by_roster: IndexMap<i64, SeasonLineupRow> = IndexMap::new();
    for wk in weeks {
        for row in &wk.lineups {
            let agg = by_roster.entry(row.roster_id).or_insert(SeasonLineupRow {
                roster_id: row.roster_id,
                actual: 0.0,
                optimal: 0.0,
                weeks: 0,
                perfect_weeks: 0,
                left_on_bench: 0.0,
                efficiency: 1.0,
            });
            agg.actual = crate::util::round2(agg.actual + row.actual);
            agg.optimal = crate::util::round2(agg.optimal + row.optimal);
            agg.weeks += 1;
            if row.left_on_bench <= 0.005 {
                agg.perfect_weeks += 1;
            }
        }
    }
    let mut rows: Vec<SeasonLineupRow> = by_roster
        .into_values()
        .map(|mut row| {
            row.left_on_bench = row.optimal - row.actual;
            row.efficiency = if row.optimal > 0.0 {
                row.actual / row.optimal
            } else {
                1.0
            };
            row
        })
        .collect();
    rows.sort_by(|a, b| {
        b.efficiency
            .partial_cmp(&a.efficiency)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.roster_id.cmp(&b.roster_id))
    });
    rows
}

//! The template context for each page. The shapes mirror what the Python
//! build passed to Jinja2, so the templates read the same names.

use std::collections::BTreeMap;

use minijinja::Value;
use serde::Serialize;

use super::payloads::asset_version;
use super::SiteOptions;
use crate::compute::rivalries::rivalry_highlights;
use crate::compute::trades::{
    injury_outlook_table, trade_verdict, FAIR_TRADE_GAP, LINEUP_FIT_GAP, PRIOR_GAMES,
    TRADE_VERDICT_BANDS,
};
use crate::compute::POSITIONS;
use crate::models::{
    HeadToHead, LineupRow, Manager, PlayerScore, PlayerValue, PositionRankRow, PowerRankRow,
    PreseasonRow, RosterReport, SeasonSummary, Team, TeamRoster, TeamRosterMoves, TeamStrengthRow,
    TradeSide, WeekSummary,
};
use crate::pipeline::is_complete;

/// A page context: named values, merged into one Jinja2 context at render.
pub type Ctx = BTreeMap<&'static str, Value>;

fn ser<T: Serialize>(value: &T) -> Value {
    Value::from_serialize(value)
}

pub fn merge(common: &Ctx, extra: Ctx) -> Value {
    let mut all = common.clone();
    all.extend(extra);
    Value::from_iter(all)
}

fn week_label_weeks(season: &SeasonSummary) -> Vec<i64> {
    let mut weeks: Vec<i64> = season.weeks.iter().map(|w| w.week).collect();
    if let Some(prov) = &season.provisional_week_summary {
        weeks.push(prov.week);
    }
    if !season.preseason.is_empty() {
        weeks.insert(0, 0);
    }
    if is_complete(season) {
        weeks.push(season.playoff_week_start);
    }
    weeks
}

pub fn common(season: &SeasonSummary, data_js: &str, opts: &SiteOptions) -> Ctx {
    let all_seasons = if opts.all_seasons.is_empty() {
        vec![season.season.clone()]
    } else {
        opts.all_seasons.to_vec()
    };
    BTreeMap::from([
        ("asset_version", Value::from(asset_version(data_js))),
        ("site_url", Value::from(opts.site_url)),
        ("chart_js_url", Value::from(super::CHART_JS_URL)),
        ("chart_js_sri", Value::from(super::CHART_JS_SRI)),
        ("league_name", Value::from(season.league_name.as_str())),
        ("season_year", Value::from(season.season.as_str())),
        ("has_draft", Value::from(season.draft.is_some())),
        ("has_season", Value::from(!season.weeks.is_empty())),
        ("has_rivalries", Value::from(!opts.rivalries.is_empty())),
        ("has_trades", Value::from(has_trades_page(season))),
        ("has_rosters", Value::from(season.roster_report.is_some())),
        ("all_seasons", ser(&all_seasons)),
        ("site_root_prefix", Value::from(opts.site_root_prefix)),
        ("recap_week", Value::from(season.playoff_week_start)),
        ("all_weeks", ser(&week_label_weeks(season))),
    ])
}

pub fn has_trades_page(season: &SeasonSummary) -> bool {
    season.trade_values.is_some()
        || season
            .acquisitions
            .as_ref()
            .is_some_and(|a| !a.trades.is_empty())
}

#[derive(Serialize)]
struct RankingRow {
    row: PowerRankRow,
    team: Team,
    week_points: Option<f64>,
}

#[derive(Serialize)]
struct ResultRow {
    winner_team: Team,
    winner_points: f64,
    loser_team: Team,
    loser_points: f64,
    margin: f64,
}

#[derive(Serialize)]
struct PairSide {
    team: Team,
    points: f64,
}

#[derive(Serialize)]
struct MovesRow {
    team: Team,
    moves: TeamRosterMoves,
}

#[derive(Serialize)]
struct LineupEntry {
    team: Team,
    row: LineupRow,
}

#[derive(Serialize)]
struct PositionEntry {
    team: Team,
    row: PositionRankRow,
}

#[derive(Serialize)]
pub struct PreseasonEntry {
    row: PreseasonRow,
    team: Team,
}

fn team_of(season: &SeasonSummary, rid: i64) -> Team {
    season.teams[&rid].clone()
}

fn matchup_pair(wk: &WeekSummary, matchup_id: i64, season: &SeasonSummary) -> Vec<PairSide> {
    wk.matchups
        .iter()
        .filter(|m| m.matchup_id == matchup_id)
        .map(|m| PairSide {
            team: team_of(season, m.roster_id),
            points: m.team_points,
        })
        .collect()
}

/// One week's page: rankings, awards, results, positions, lineups and moves.
pub fn week(season: &SeasonSummary, wk: &WeekSummary, asset_prefix: &str, is_index: bool) -> Ctx {
    let matchups_by_roster: BTreeMap<i64, &crate::models::Matchup> =
        wk.matchups.iter().map(|m| (m.roster_id, m)).collect();
    let rankings_frozen = wk.power_rankings.is_empty() && !season.weeks.is_empty();
    let ranking_source = if rankings_frozen {
        &season
            .weeks
            .last()
            .expect("weeks is non-empty")
            .power_rankings
    } else {
        &wk.power_rankings
    };
    let ranking_rows: Vec<RankingRow> = ranking_source
        .iter()
        .map(|row| RankingRow {
            row: row.clone(),
            team: team_of(season, row.roster_id),
            week_points: matchups_by_roster
                .get(&row.roster_id)
                .map(|m| m.team_points),
        })
        .collect();

    let mut results: Vec<ResultRow> = Vec::new();
    let mut seen: Vec<i64> = Vec::new();
    for m in &wk.matchups {
        if seen.contains(&m.matchup_id) {
            continue;
        }
        let Some(opp_id) = m.opponent_roster_id else {
            continue;
        };
        seen.push(m.matchup_id);
        let Some(opp) = matchups_by_roster.get(&opp_id) else {
            continue;
        };
        let (winner, loser) = if m.team_points >= opp.team_points {
            (m, *opp)
        } else {
            (*opp, m)
        };
        results.push(ResultRow {
            winner_team: team_of(season, winner.roster_id),
            winner_points: winner.team_points,
            loser_team: team_of(season, loser.roster_id),
            loser_points: loser.team_points,
            margin: winner.team_points - loser.team_points,
        });
    }
    results.sort_by(|a, b| {
        a.margin
            .partial_cmp(&b.margin)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let awards = &wk.awards;
    let closest: Vec<Vec<PairSide>> = awards
        .closest_matchup_ids
        .iter()
        .map(|mid| matchup_pair(wk, *mid, season))
        .collect();
    let blowouts: Vec<Vec<PairSide>> = awards
        .biggest_blowout_matchup_ids
        .iter()
        .map(|mid| matchup_pair(wk, *mid, season))
        .collect();
    let roster_moves_rows: Vec<MovesRow> = wk
        .roster_moves
        .iter()
        .map(|m| MovesRow {
            team: team_of(season, m.roster_id),
            moves: m.clone(),
        })
        .collect();

    // A lineup can only be judged once its games are played.
    let mut lineup_rows: Vec<LineupEntry> = Vec::new();
    let mut costliest: Option<LineupEntry> = None;
    if !wk.lineups.is_empty() && Some(wk.week) != season.provisional_week {
        lineup_rows = wk
            .lineups
            .iter()
            .map(|row| LineupEntry {
                team: team_of(season, row.roster_id),
                row: row.clone(),
            })
            .collect();
        lineup_rows.sort_by(|a, b| {
            b.row
                .left_on_bench
                .partial_cmp(&a.row.left_on_bench)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.team.name.cmp(&b.team.name))
        });
        if lineup_rows[0].row.left_on_bench > 0.005 {
            costliest = Some(LineupEntry {
                team: lineup_rows[0].team.clone(),
                row: lineup_rows[0].row.clone(),
            });
        }
    }

    // Before kickoff every total is 0.0 and every team "ties for 1st" -- noise.
    let mut position_rows: Vec<PositionEntry> = Vec::new();
    let mut position_extremes: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    if wk
        .position_ranks
        .iter()
        .any(|row| row.points.values().any(|pts| *pts > 0.0))
    {
        position_rows = wk
            .position_ranks
            .iter()
            .map(|row| PositionEntry {
                team: team_of(season, row.roster_id),
                row: row.clone(),
            })
            .collect();
        // Highest score first; the sort is stable, so ties keep roster order.
        let score = |e: &PositionEntry| -matchups_by_roster[&e.row.roster_id].team_points;
        position_rows.sort_by(|a, b| {
            score(a)
                .partial_cmp(&score(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for pos in POSITIONS {
            let ranks: Vec<i64> = wk.position_ranks.iter().map(|row| row.ranks[pos]).collect();
            let lo = ranks.iter().copied().min().unwrap_or(0);
            let hi = ranks.iter().copied().max().unwrap_or(0);
            position_extremes.insert(pos.to_string(), (lo, hi));
        }
    }

    let teams_for =
        |ids: &[i64]| -> Vec<Team> { ids.iter().map(|rid| team_of(season, *rid)).collect() };
    let players_for = |pairs: &[(i64, PlayerScore)]| -> Vec<(Team, PlayerScore)> {
        pairs
            .iter()
            .map(|(rid, p)| (team_of(season, *rid), p.clone()))
            .collect()
    };

    BTreeMap::from([
        ("season", ser(season)),
        ("week", ser(wk)),
        ("teams", ser(&season.teams)),
        ("ranking_rows", ser(&ranking_rows)),
        ("results", ser(&results)),
        ("closest_matchups", ser(&closest)),
        ("blowout_matchups", ser(&blowouts)),
        ("roster_moves_rows", ser(&roster_moves_rows)),
        ("lineup_rows", ser(&lineup_rows)),
        ("costliest_lineup", ser(&costliest)),
        ("positions", ser(&POSITIONS)),
        ("position_rank_rows", ser(&position_rows)),
        ("position_rank_extremes", ser(&position_extremes)),
        (
            "highest_score_teams",
            ser(&teams_for(&awards.highest_score_roster_ids)),
        ),
        (
            "lowest_score_teams",
            ser(&teams_for(&awards.lowest_score_roster_ids)),
        ),
        ("best_bench", ser(&players_for(&awards.best_bench))),
        ("best_starter", ser(&players_for(&awards.best_starter))),
        ("worst_starter", ser(&players_for(&awards.worst_starter))),
        ("asset_prefix", Value::from(asset_prefix)),
        ("is_index", Value::from(is_index)),
        ("playoff", Value::from(wk.week >= season.playoff_week_start)),
        (
            "provisional",
            Value::from(Some(wk.week) == season.provisional_week),
        ),
        ("rankings_frozen", Value::from(rankings_frozen)),
    ])
}

pub fn preseason_rows(season: &SeasonSummary) -> Vec<PreseasonEntry> {
    season
        .preseason
        .iter()
        .map(|row| PreseasonEntry {
            row: row.clone(),
            team: team_of(season, row.roster_id),
        })
        .collect()
}

pub fn preseason_page(rows: &[PreseasonEntry]) -> Ctx {
    BTreeMap::from([
        ("asset_prefix", Value::from("../")),
        ("preseason_rows", ser(&rows)),
    ])
}

pub fn landing(rows: &[PreseasonEntry]) -> Ctx {
    BTreeMap::from([
        ("asset_prefix", Value::from("")),
        ("preseason_rows", ser(&rows)),
    ])
}

pub fn draft(season: &SeasonSummary) -> Ctx {
    BTreeMap::from([
        ("draft", ser(&season.draft)),
        ("teams", ser(&season.teams)),
        ("asset_prefix", Value::from("")),
    ])
}

pub fn recap(season: &SeasonSummary) -> Ctx {
    BTreeMap::from([
        ("asset_prefix", Value::from("../")),
        ("teams", ser(&season.teams)),
        ("records", ser(&season.season_board.records)),
    ])
}

#[derive(Serialize)]
struct Highlights<'a> {
    one_sided: Option<(&'a Manager, &'a HeadToHead)>,
    even: Option<(&'a Manager, &'a HeadToHead)>,
    streak: Option<(&'a Manager, &'a HeadToHead, i64)>,
}

pub fn rivalries(managers: &[Manager]) -> Ctx {
    let by_id: BTreeMap<&str, &Manager> =
        managers.iter().map(|m| (m.owner_id.as_str(), m)).collect();
    let highlights = rivalry_highlights(managers, 4);
    let view = Highlights {
        one_sided: highlights.one_sided,
        even: highlights.even,
        streak: highlights.streak,
    };
    BTreeMap::from([
        ("asset_prefix", Value::from("")),
        ("managers", ser(&managers)),
        ("by_id", ser(&by_id)),
        ("highlights", ser(&view)),
    ])
}

#[derive(Serialize)]
struct HistoryRow {
    side: TradeSide,
    team: Team,
    value: Option<f64>,
}

#[derive(Serialize)]
struct Forward {
    verdict: &'static str,
    team: Option<Team>,
}

#[derive(Serialize)]
struct TradeHistory {
    week: i64,
    rows: Vec<HistoryRow>,
    leader: Team,
    lead: f64,
    forward: Option<Forward>,
}

#[derive(Serialize)]
struct BoardGroup {
    position: String,
    replacement: f64,
    rows: Vec<PlayerValue>,
}

#[derive(Serialize)]
struct StrengthEntry {
    team: Team,
    row: TeamStrengthRow,
}

/// This season's trades, and -- while the season is live -- the calculator.
pub fn trades(season: &SeasonSummary) -> Ctx {
    let tv = season.trade_values.as_ref();
    let mut history: Vec<TradeHistory> = Vec::new();
    for trade in season.acquisitions.iter().flat_map(|a| a.trades.iter()) {
        if trade.sides.is_empty() {
            continue;
        }
        let rows: Vec<HistoryRow> = trade
            .sides
            .iter()
            .map(|side| HistoryRow {
                side: side.clone(),
                team: team_of(season, side.roster_id),
                value: tv.map(|tv| {
                    round_sum(
                        side.received
                            .iter()
                            .filter_map(|a| tv.players.get(&a.player.player_id).map(|p| p.value)),
                    )
                }),
            })
            .collect();
        let leader = &trade.sides[0];
        let forward = tv.map(|_| {
            let values: Vec<f64> = rows.iter().map(|r| r.value.unwrap_or(0.0)).collect();
            let (verdict, ahead) = trade_verdict(&values);
            Forward {
                verdict,
                team: ahead.map(|i| rows[i].team.clone()),
            }
        });
        let second = trade.sides.get(1).map_or(0.0, |s| s.points);
        history.push(TradeHistory {
            week: trade.week,
            rows,
            leader: team_of(season, leader.roster_id),
            lead: leader.points - second,
            forward,
        });
    }

    let mut ctx: Ctx = BTreeMap::from([
        ("trade_history", ser(&history)),
        ("values", ser(&tv)),
        ("weeks_left", Value::from(0)),
        ("asset_prefix", Value::from("")),
        ("teams", ser(&season.teams)),
    ]);
    let Some(tv) = tv else {
        return ctx;
    };

    let rostered: Vec<&PlayerValue> = tv
        .players
        .values()
        .filter(|pv| pv.roster_id.is_some())
        .collect();
    let mut board: Vec<BoardGroup> = Vec::new();
    let mut order: Vec<String> = POSITIONS.iter().map(|p| p.to_string()).collect();
    let mut extra: Vec<String> = tv
        .replacement
        .keys()
        .filter(|p| !POSITIONS.contains(&p.as_str()))
        .cloned()
        .collect();
    extra.sort();
    order.extend(extra);
    for pos in &order {
        let mut rows: Vec<&PlayerValue> = rostered
            .iter()
            .copied()
            .filter(|pv| &pv.player.position == pos)
            .collect();
        rows.sort_by(|a, b| {
            b.value
                .partial_cmp(&a.value)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.projection
                        .partial_cmp(&a.projection)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.player.name.cmp(&b.player.name))
        });
        if let Some(level) = tv.replacement.get(pos).filter(|_| !rows.is_empty()) {
            board.push(BoardGroup {
                position: pos.clone(),
                replacement: *level,
                rows: rows.into_iter().cloned().collect(),
            });
        }
    }

    let mut extremes: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for pos in POSITIONS {
        let ranks: Vec<i64> = tv.strength.iter().map(|row| row.ranks[pos]).collect();
        let extreme = if ranks.is_empty() {
            (1, 1)
        } else {
            (
                *ranks.iter().min().unwrap_or(&1),
                *ranks.iter().max().unwrap_or(&1),
            )
        };
        extremes.insert(pos.to_string(), extreme);
    }

    let mut calc_teams: Vec<Team> = tv
        .rosters
        .iter()
        .filter_map(|(rid, _)| season.teams.get(rid).cloned())
        .collect();
    calc_teams.sort_by_key(|t| t.name.to_lowercase());

    let strength_rows: Vec<StrengthEntry> = tv
        .strength
        .iter()
        .map(|row| StrengthEntry {
            team: team_of(season, row.roster_id),
            row: row.clone(),
        })
        .collect();

    let deadline_passed = tv
        .trade_deadline
        .is_some_and(|deadline| season.through_week >= deadline);
    ctx.extend(BTreeMap::from([
        ("calc_teams", ser(&calc_teams)),
        ("value_board", ser(&board)),
        ("strength_rows", ser(&strength_rows)),
        ("strength_extremes", ser(&extremes)),
        ("positions", ser(&POSITIONS)),
        ("weeks_left", Value::from(tv.weeks_left)),
        ("trade_deadline", ser(&tv.trade_deadline)),
        ("deadline_passed", Value::from(deadline_passed)),
    ]));
    ctx
}

/// Sum of values rounded to cents, as the trades page shows a side's haul.
fn round_sum(values: impl Iterator<Item = f64>) -> f64 {
    crate::util::round2(crate::compute::sum_f64(values))
}

#[derive(Serialize)]
struct CrownRow {
    roster_id: i64,
    count: i64,
}

/// The season scoring board, with its crown counts already in display order.
pub fn season(season: &SeasonSummary) -> Ctx {
    let mut crowns: Vec<CrownRow> = season
        .season_board
        .crown_counts
        .iter()
        .map(|(roster_id, count)| CrownRow {
            roster_id: *roster_id,
            count: *count,
        })
        .collect();
    // Most crowns first; the sort is stable, so ties keep roster order.
    crowns.sort_by_key(|c| std::cmp::Reverse(c.count));
    BTreeMap::from([
        ("season", ser(season)),
        ("board", ser(&season.season_board)),
        ("teams", ser(&season.teams)),
        ("crown_rows", ser(&crowns)),
        ("asset_prefix", Value::from("")),
    ])
}

pub fn rosters(season: &SeasonSummary, report: &RosterReport) -> Ctx {
    let mut faab_order: Vec<&TeamRoster> = report.teams.iter().collect();
    faab_order.sort_by_key(|t| std::cmp::Reverse(t.faab_remaining));
    BTreeMap::from([
        ("report", ser(report)),
        ("faab_order", ser(&faab_order)),
        ("teams", ser(&season.teams)),
        ("asset_prefix", Value::from("")),
    ])
}

#[derive(Serialize)]
struct WeightRow {
    key: &'static str,
    label: &'static str,
    weight: f64,
}

#[derive(Serialize)]
struct TermView {
    label: &'static str,
    weight: f64,
    value: f64,
}

#[derive(Serialize)]
struct ExampleView {
    week: i64,
    team: Team,
    score: f64,
    terms: Vec<TermView>,
}

#[derive(Serialize)]
struct TradeExample {
    pv: PlayerValue,
    team: Option<Team>,
    replacement: f64,
    share: f64,
}

const RANKING_COMPONENTS: [(&str, &str); 4] = [
    ("win_pct", "Win %"),
    ("allplay_pct", "All-play %"),
    ("pf_norm", "Points for, scaled"),
    ("form_norm", "Recent form, scaled"),
];

fn component_value(row: &PowerRankRow, key: &str) -> f64 {
    match key {
        "win_pct" => row.win_pct,
        "allplay_pct" => row.allplay_pct,
        "pf_norm" => row.pf_norm,
        _ => row.form_norm,
    }
}

fn weight_of(season: &SeasonSummary, key: &str) -> f64 {
    let w = &season.weights;
    match key {
        "win_pct" => w.win_pct,
        "allplay_pct" => w.allplay_pct,
        "pf_norm" => w.pf_norm,
        _ => w.form_norm,
    }
}

fn trade_example(season: &SeasonSummary) -> Option<TradeExample> {
    let tv = season.trade_values.as_ref()?;
    let candidates: Vec<&PlayerValue> = tv
        .players
        .values()
        .filter(|pv| pv.roster_id.is_some() && pv.games > 0 && pv.value > 0.0)
        .collect();
    let pv = candidates.into_iter().max_by(|a, b| {
        a.value
            .partial_cmp(&b.value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.player.name.cmp(&b.player.name))
    })?;
    let weight_sum: f64 = tv.weights.iter().sum();
    let played: f64 = tv
        .weights
        .iter()
        .zip(&pv.availability)
        .map(|(w, a)| w * a)
        .sum();
    Some(TradeExample {
        pv: pv.clone(),
        team: pv.roster_id.and_then(|rid| season.teams.get(&rid).cloned()),
        replacement: tv.replacement[&pv.player.position],
        share: if weight_sum != 0.0 {
            played / weight_sum
        } else {
            1.0
        },
    })
}

/// The live numbers the explainer quotes, so it can't drift from the code.
pub fn how_it_works(season: &SeasonSummary) -> Ctx {
    let weights: Vec<WeightRow> = RANKING_COMPONENTS
        .iter()
        .map(|(key, label)| WeightRow {
            key,
            label,
            weight: weight_of(season, key),
        })
        .collect();

    let example = season.weeks.last().and_then(|wk| {
        let top = wk.power_rankings.first()?;
        Some(ExampleView {
            week: wk.week,
            team: team_of(season, top.roster_id),
            score: top.score,
            terms: RANKING_COMPONENTS
                .iter()
                .map(|(key, label)| TermView {
                    label,
                    weight: weight_of(season, key),
                    value: component_value(top, key),
                })
                .collect(),
        })
    });

    let replacement_levels: Vec<(String, f64)> = match &season.trade_values {
        Some(tv) => {
            let mut order: Vec<String> = POSITIONS.iter().map(|p| p.to_string()).collect();
            let mut extra: Vec<String> = tv
                .replacement
                .keys()
                .filter(|p| !POSITIONS.contains(&p.as_str()))
                .cloned()
                .collect();
            extra.sort();
            order.extend(extra);
            order
                .into_iter()
                .filter_map(|pos| tv.replacement.get(&pos).map(|level| (pos, *level)))
                .collect()
        }
        None => Vec::new(),
    };

    let outlook: BTreeMap<&str, Vec<f64>> = injury_outlook_table();
    let ir_weeks = outlook.get("IR").map_or(0, Vec::len);
    let form_window = if season.form_window == 0 {
        crate::compute::DEFAULT_FORM_WINDOW
    } else {
        season.form_window
    };

    BTreeMap::from([
        ("weights", ser(&weights)),
        ("form_window", Value::from(form_window)),
        (
            "league_average_match",
            Value::from(season.league_average_match),
        ),
        ("num_teams", Value::from(season.teams.len() as i64)),
        ("example", ser(&example)),
        ("positions", ser(&POSITIONS)),
        ("playoff_sims", Value::from(crate::compute::PLAYOFF_SIMS)),
        ("playoff_teams", Value::from(season.playoff_teams)),
        ("playoff_byes", Value::from(season.playoff_byes)),
        ("trade_values", ser(&season.trade_values)),
        ("trade_example", ser(&trade_example(season))),
        (
            "prior_games",
            Value::from(
                season
                    .trade_values
                    .as_ref()
                    .map_or(PRIOR_GAMES, |tv| tv.prior_games),
            ),
        ),
        ("fair_gap", Value::from(FAIR_TRADE_GAP)),
        ("verdict_bands", ser(&TRADE_VERDICT_BANDS.to_vec())),
        ("fit_gap", Value::from(LINEUP_FIT_GAP)),
        ("outlook", ser(&outlook)),
        ("ir_weeks", Value::from(ir_weeks as i64)),
        (
            "superflex",
            Value::from(season.roster_positions.iter().any(|s| s == "SUPER_FLEX")),
        ),
        ("replacement_levels", ser(&replacement_levels)),
        ("asset_prefix", Value::from("")),
    ])
}

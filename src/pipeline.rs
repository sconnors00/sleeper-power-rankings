//! Fetching and assembling a season: the orchestration the CLI runs. Each
//! step is best-effort where the Python version was (a missing draft, an
//! unreachable previous league, or an NFL schedule without a cache just means
//! that section is left out, never a failed build).

use std::collections::BTreeMap;

use anyhow::{bail, Result};
use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::compute::draft::grade_draft;
use crate::compute::matchups::{
    apply_official_records, build_provisional_week_summary, build_week_summaries, RosterMap,
};
use crate::compute::playoffs::{
    playoff_byes, playoff_weeks, schedule_pairs, simulate_playoff_odds,
};
use crate::compute::preseason::build_preseason_rankings;
use crate::compute::season::build_season_board;
use crate::compute::trades::{build_trade_values, nfl_byes, WeekSet};
use crate::compute::{acquisitions, build_teams, rosters, PLAYOFF_SIMS};
use crate::config::Settings;
use crate::models::{PlayoffOddsRow, PreseasonRow, SeasonSummary, WeekSummary};
use crate::raw::{
    text_of, League, LeagueSettings, PlayerMap, RawDraft, RawMatchup, RawPick, RawRoster,
    RawTransaction, RawUser, State,
};
use crate::sleeper::SleeperClient;

pub type Remaining = BTreeMap<i64, Vec<(i64, i64)>>;

fn parse<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(Into::into)
}

/// Fetch and parse, or `None` when either step fails.
fn try_parse<T: DeserializeOwned>(value: Result<Value, crate::sleeper::SleeperError>) -> Option<T> {
    value.ok().and_then(|v| parse(v).ok())
}

/// Build the current season and every past season reachable through
/// `previous_league_id`. Returns the current season first.
pub fn build_all(
    client: &SleeperClient,
    league_id: &str,
    season_override: Option<&str>,
    through: Option<i64>,
    provisional: bool,
    settings: &Settings,
) -> Result<Vec<SeasonSummary>> {
    let season = build_season(
        client,
        league_id,
        season_override,
        through,
        provisional,
        settings,
    )?;
    let league: League = parse(client.get_league(league_id)?)?;
    let mut all = vec![season];
    for past_id in walk_previous_league_ids(client, &league) {
        if let Ok(past) = build_season(client, &past_id, None, None, false, settings) {
            all.push(past);
        }
    }
    Ok(all)
}

/// The chat-ready recap printed by `ffpr blurb`.
pub fn blurb(
    client: &SleeperClient,
    league_id: &str,
    week: Option<i64>,
    season_override: Option<&str>,
    settings: &Settings,
) -> Result<String> {
    let summary = build_season(client, league_id, season_override, week, false, settings)?;
    let teams = &summary.teams;
    let site_url = &settings.site_url;

    let Some(wk) = summary.weeks.last() else {
        if !summary.preseason.is_empty() {
            let mut lines = vec![format!(
                "{} -- pre-season power rankings",
                summary.league_name
            )];
            for row in summary.preseason.iter().take(3) {
                let note = if row.new_owner { " (new owner)" } else { "" };
                lines.push(format!(
                    "  {}. {}{}",
                    row.rank, teams[&row.roster_id].name, note
                ));
            }
            if !site_url.is_empty() {
                lines.push(site_url.clone());
            }
            return Ok(lines.join("\n"));
        }
        return Ok(format!("{}: no completed weeks yet.", summary.league_name));
    };

    let awards = &wk.awards;
    let mut lines = vec![format!("{} -- Week {} recap", summary.league_name, wk.week)];
    let names = |ids: &[i64]| {
        ids.iter()
            .map(|rid| teams[rid].name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    lines.push(format!(
        "Top score: {} ({:.2})",
        names(&awards.highest_score_roster_ids),
        awards.highest_score
    ));
    lines.push(format!(
        "Low score: {} ({:.2})",
        names(&awards.lowest_score_roster_ids),
        awards.lowest_score
    ));
    if !awards.best_bench.is_empty() {
        let bench: Vec<String> = awards
            .best_bench
            .iter()
            .map(|(rid, p)| format!("{} ({})", p.name, teams[rid].name))
            .collect();
        lines.push(format!(
            "Best bench: {} ({:.2})",
            bench.join(", "),
            awards.best_bench_points
        ));
    }
    lines.push("Power rankings top 3:".to_string());
    for row in wk.power_rankings.iter().take(3) {
        lines.push(format!(
            "  {}. {} ({})",
            row.rank, teams[&row.roster_id].name, row.record
        ));
    }
    if !site_url.is_empty() {
        lines.push(site_url.clone());
    }
    Ok(lines.join("\n"))
}

/// Whether the regular season is finished (so the recap page exists).
pub fn is_complete(season: &SeasonSummary) -> bool {
    !season.weeks.is_empty() && season.through_week >= season.playoff_week_start - 1
}

pub fn build_season(
    client: &SleeperClient,
    league_id: &str,
    season_override: Option<&str>,
    through_override: Option<i64>,
    provisional: bool,
    settings: &Settings,
) -> Result<SeasonSummary> {
    let state: State = parse(client.get_state()?)?;
    let league: League = parse(client.get_league(league_id)?)?;
    let season = season_override.map_or_else(|| league.season.clone(), str::to_string);
    if let Some(requested) = season_override {
        if requested != league.season {
            bail!(
                "--season {requested} doesn't match league {league_id}'s season ({}).",
                league.season
            );
        }
    }

    let rosters: Vec<RawRoster> = parse(client.get_rosters(league_id, &season)?)?;
    let users: Vec<RawUser> = parse(client.get_users(league_id, &season)?)?;
    let players: PlayerMap = parse(client.get_players()?)?;
    let roster_ids: Vec<i64> = rosters.iter().map(|r| r.roster_id).collect();
    let rosters_by_id: RosterMap = rosters.iter().map(|r| (r.roster_id, r.clone())).collect();
    let teams = build_teams(&rosters, &users);

    let playoff_week_start = league.settings.playoff_week_start;
    let league_average_match = league.settings.league_average_match != 0;
    let is_current = season == state.season;
    let start_week = if is_current {
        state.week
    } else {
        playoff_week_start - 1
    };

    let through_week = match through_override {
        Some(week) => week.min(playoff_week_start - 1),
        None => find_through_week(
            client,
            league_id,
            &season,
            &roster_ids,
            start_week,
            playoff_week_start,
            is_current,
        )?,
    };

    // Only a finished season is immutable: a week in the current season keeps
    // changing after its games end (waiver claims processed midweek, stat
    // corrections), so the current season is always refetched.
    let historical = !is_current;
    let mut weeks_raw: BTreeMap<i64, Vec<RawMatchup>> = BTreeMap::new();
    let mut transactions_raw: BTreeMap<i64, Vec<RawTransaction>> = BTreeMap::new();
    for week in 1..=through_week {
        weeks_raw.insert(
            week,
            parse(client.get_matchups(league_id, &season, week, historical)?)?,
        );
        transactions_raw.insert(
            week,
            parse(client.get_transactions(league_id, &season, week, historical)?)?,
        );
    }

    let mut week_summaries = build_week_summaries(
        &roster_ids,
        &weeks_raw,
        &transactions_raw,
        &rosters_by_id,
        &players,
        league_average_match,
        &settings.weights,
        settings.form_window,
        Some(&league.roster_positions),
    );
    if let Some(last) = week_summaries.last_mut() {
        apply_official_records(&mut last.power_rankings, &rosters_by_id);
    }
    let season_board = build_season_board(&week_summaries, &teams);

    let mut provisional_summary: Option<WeekSummary> = None;
    let mut provisional_week: Option<i64> = None;
    if provisional {
        let candidate = if is_current {
            start_week
        } else {
            through_week + 1
        };
        if through_week < candidate && candidate < playoff_week_start {
            let raw: Vec<RawMatchup> =
                parse(client.get_matchups(league_id, &season, candidate, false)?)?;
            if !raw.is_empty() {
                let raw_tx: Vec<RawTransaction> =
                    parse(client.get_transactions(league_id, &season, candidate, false)?)?;
                provisional_summary = Some(build_provisional_week_summary(
                    candidate,
                    &raw,
                    &raw_tx,
                    &rosters_by_id,
                    &players,
                    &league.roster_positions,
                ));
                provisional_week = Some(candidate);
            }
        }
    }

    let (playoff_odds, remaining) = if is_current {
        playoff_odds_for(
            client,
            &league,
            league_id,
            &season,
            &rosters,
            &week_summaries,
            through_week,
        )
    } else {
        (Vec::new(), Remaining::new())
    };

    let draft = fetch_draft(client, &league)
        .map(|(draft, picks)| grade_draft(&picks, &players, draft.settings.budget.unwrap_or(0)));

    let preseason = build_preseason(client, &league, &rosters, &players, settings);

    let trade_values = if is_current && !week_summaries.is_empty() {
        let deadline = league.settings.trade_deadline.unwrap_or(0);
        let (horizon, playoff_from) =
            trade_horizon(&league.settings, rosters.len(), through_week, state.week);
        let byes = schedule_byes(client, &season);
        Some(build_trade_values(
            &weeks_raw,
            &rosters,
            &players,
            &league.roster_positions,
            (playoff_week_start - 1 - through_week).max(0),
            (0 < deadline && deadline < playoff_week_start).then_some(deadline),
            crate::compute::trades::PRIOR_GAMES,
            &horizon,
            playoff_from,
            &byes,
        ))
    } else {
        None
    };

    let playoff_teams = league.settings.playoff_teams;
    let acquisitions =
        acquisitions::grade_acquisitions(&transactions_raw, &week_summaries, &players);
    let roster_report = rosters::build_roster_report(
        &rosters,
        &players,
        &league.roster_positions,
        league.settings.waiver_budget.unwrap_or(0),
        &transactions_raw,
    );
    Ok(SeasonSummary {
        season,
        league_name: league.name,
        teams,
        weeks: week_summaries,
        season_board,
        playoff_week_start,
        through_week,
        provisional_week,
        roster_positions: league.roster_positions,
        provisional_week_summary: provisional_summary,
        preseason,
        draft,
        weights: settings.weights.clone(),
        form_window: settings.form_window,
        league_average_match,
        playoff_odds_sims: if playoff_odds.is_empty() {
            0
        } else {
            PLAYOFF_SIMS
        },
        playoff_odds,
        playoff_teams,
        playoff_byes: playoff_byes(playoff_teams),
        remaining_weeks: remaining.len() as i64,
        acquisitions: Some(acquisitions),
        trade_values,
        roster_report: Some(roster_report),
    })
}

/// Walk backward from `start_week` to the latest week every roster has scored in.
fn find_through_week(
    client: &SleeperClient,
    league_id: &str,
    season: &str,
    roster_ids: &[i64],
    start_week: i64,
    playoff_week_start: i64,
    is_current: bool,
) -> Result<i64> {
    let upper = start_week.min(playoff_week_start - 1);
    for week in (1..=upper).rev() {
        let raw: Vec<RawMatchup> =
            parse(client.get_matchups(league_id, season, week, !is_current)?)?;
        let complete = !raw.is_empty()
            && roster_ids.iter().all(|rid| {
                raw.iter()
                    .any(|m| m.roster_id == *rid && m.points.unwrap_or(0.0) > 0.0)
            });
        if complete {
            return Ok(week);
        }
    }
    Ok(0)
}

/// The league's completed auction draft and its picks, or `None`. Snake drafts,
/// unfinished drafts and fetch failures just mean no draft grades.
fn fetch_draft(client: &SleeperClient, league: &League) -> Option<(RawDraft, Vec<RawPick>)> {
    let draft_id = league.draft_id.as_deref().filter(|s| !s.is_empty())?;
    let draft: RawDraft = try_parse(client.get_draft(draft_id))?;
    if draft.kind.as_deref() != Some("auction") || draft.status.as_deref() != Some("complete") {
        return None;
    }
    let picks: Vec<RawPick> = try_parse(client.get_draft_picks(draft_id, true))?;
    if picks.is_empty() {
        None
    } else {
        Some((draft, picks))
    }
}

/// Last season's final power rankings mapped onto this season's rosters.
/// Best-effort: any failure just means no pre-season rankings.
fn build_preseason(
    client: &SleeperClient,
    league: &League,
    current_rosters: &[RawRoster],
    players: &PlayerMap,
    settings: &Settings,
) -> Vec<PreseasonRow> {
    let Some(prev_id) = league
        .previous_league_id
        .as_deref()
        .filter(|s| !s.is_empty())
    else {
        return Vec::new();
    };
    preseason_from_previous(client, prev_id, league, current_rosters, players, settings)
        .unwrap_or_default()
}

fn preseason_from_previous(
    client: &SleeperClient,
    prev_id: &str,
    league: &League,
    current_rosters: &[RawRoster],
    players: &PlayerMap,
    settings: &Settings,
) -> Option<Vec<PreseasonRow>> {
    let prev_league: League = try_parse(client.get_league(prev_id))?;
    let prev_season = prev_league.season.clone();
    let prev_rosters: Vec<RawRoster> = try_parse(client.get_rosters(prev_id, &prev_season))?;
    let prev_playoff_start = prev_league.settings.playoff_week_start;
    let mut prev_weeks_raw: BTreeMap<i64, Vec<RawMatchup>> = BTreeMap::new();
    for week in 1..prev_playoff_start {
        prev_weeks_raw.insert(
            week,
            try_parse(client.get_matchups(prev_id, &prev_season, week, true))?,
        );
    }

    let prev_by_id: RosterMap = prev_rosters
        .iter()
        .map(|r| (r.roster_id, r.clone()))
        .collect();
    let prev_ids: Vec<i64> = prev_rosters.iter().map(|r| r.roster_id).collect();
    let prev_summaries = build_week_summaries(
        &prev_ids,
        &prev_weeks_raw,
        &BTreeMap::new(),
        &prev_by_id,
        players,
        prev_league.settings.league_average_match != 0,
        &settings.weights,
        settings.form_window,
        None,
    );
    let last = prev_summaries.last()?;
    let mut prev_rankings = last.power_rankings.clone();
    apply_official_records(&mut prev_rankings, &prev_by_id);

    let prev_pf: BTreeMap<i64, f64> = prev_rosters
        .iter()
        .map(|r| {
            let s = &r.settings;
            (r.roster_id, s.fpts + s.fpts_decimal / 100.0)
        })
        .collect();
    let prev_owner: BTreeMap<i64, Option<String>> = prev_rosters
        .iter()
        .map(|r| (r.roster_id, r.owner_id.clone()))
        .collect();
    let cur_owner: BTreeMap<i64, Option<String>> = current_rosters
        .iter()
        .map(|r| (r.roster_id, r.owner_id.clone()))
        .collect();
    let current_ids: Vec<i64> = current_rosters.iter().map(|r| r.roster_id).collect();

    let champion = league
        .metadata
        .as_ref()
        .and_then(|m| m.latest_league_winner_roster_id.as_ref())
        .filter(|v| match v {
            Value::Null => false,
            Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
            Value::String(s) => !s.is_empty(),
            _ => true,
        })
        .map(text_of)
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .and_then(|s| s.parse::<i64>().ok());

    Some(build_preseason_rankings(
        &prev_rankings,
        &prev_pf,
        &current_ids,
        &prev_owner,
        &cur_owner,
        champion,
    ))
}

/// The weeks trade values look ahead to, each with its weight, and the first
/// playoff week among them. A playoff week counts by the share of the league
/// that plays in it.
fn trade_horizon(
    settings: &LeagueSettings,
    num_teams: usize,
    through_week: i64,
    current_week: i64,
) -> (IndexMap<i64, f64>, Option<i64>) {
    let playoffs = playoff_weeks(
        settings.playoff_teams,
        settings.playoff_week_start,
        settings.playoff_round_type,
    );
    let start = settings.playoff_week_start;
    let last = playoffs.last().copied().unwrap_or(start - 1);
    let share = if num_teams > 0 {
        (settings.playoff_teams as f64 / num_teams as f64).min(1.0)
    } else {
        0.0
    };
    let mut horizon = IndexMap::new();
    for week in (through_week + 1).max(current_week)..=last {
        horizon.insert(week, if week >= start { share } else { 1.0 });
    }
    let playoff_from = (!playoffs.is_empty() && horizon.contains_key(&start)).then_some(start);
    (horizon, playoff_from)
}

/// Best-effort: without the NFL schedule, values just don't account for byes.
fn schedule_byes(client: &SleeperClient, season: &str) -> BTreeMap<String, WeekSet> {
    client
        .get_nfl_schedule(season)
        .map(|schedule| nfl_byes(&schedule))
        .unwrap_or_default()
}

/// Odds, and the remaining schedule they simulate. Best-effort.
fn playoff_odds_for(
    client: &SleeperClient,
    league: &League,
    league_id: &str,
    season: &str,
    rosters: &[RawRoster],
    week_summaries: &[WeekSummary],
    through_week: i64,
) -> (Vec<PlayoffOddsRow>, Remaining) {
    let settings = &league.settings;
    if week_summaries.is_empty() || settings.playoff_teams == 0 {
        return (Vec::new(), Remaining::new());
    }
    let schedule_id = league.league_id.as_deref().unwrap_or(league_id);
    let mut remaining = Remaining::new();
    for week in (through_week + 1)..settings.playoff_week_start {
        let raw: Vec<RawMatchup> =
            match try_parse(client.get_matchups(schedule_id, season, week, false)) {
                Some(raw) => raw,
                None => return (Vec::new(), Remaining::new()),
            };
        let pairs = schedule_pairs(&raw);
        if !pairs.is_empty() {
            remaining.insert(week, pairs);
        }
    }
    let divisions = (settings.divisions != 0).then(|| {
        rosters
            .iter()
            .map(|r| (r.roster_id, r.settings.division))
            .collect::<BTreeMap<i64, Option<i64>>>()
    });
    let seed = if !season.is_empty() && season.chars().all(|c| c.is_ascii_digit()) {
        season.parse::<u64>().unwrap_or(0)
    } else {
        0
    };
    let odds = simulate_playoff_odds(
        week_summaries,
        &remaining,
        settings.league_average_match != 0,
        settings.playoff_teams,
        divisions.as_ref(),
        PLAYOFF_SIMS as usize,
        seed,
    );
    (odds, remaining)
}

/// Every earlier league id in the renewal chain, most recent first. Stops at
/// the first league that can't be fetched.
fn walk_previous_league_ids(client: &SleeperClient, league: &League) -> Vec<String> {
    let mut ids = Vec::new();
    let mut prev = league.previous_league_id.clone().filter(|s| !s.is_empty());
    while let Some(id) = prev {
        ids.push(id.clone());
        match try_parse::<League>(client.get_league(&id)) {
            Some(prev_league) => prev = prev_league.previous_league_id.filter(|s| !s.is_empty()),
            None => break,
        }
    }
    ids
}

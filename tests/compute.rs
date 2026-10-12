//! Pure computations checked against the committed fixtures (one real week of
//! a past season) and against hand-built cases.

mod common;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ffpr::compute::matchups::{
    build_week_summaries, compute_allplay_week, compute_roster_moves, compute_week_awards,
    compute_weekly_head_to_head, parse_week_matchups, RosterMap,
};
use ffpr::compute::playoffs::{playoff_byes, playoff_weeks, seed_playoffs};
use ffpr::compute::trades::{
    availability, nfl_byes, rank_prior, replacement_levels, trade_verdict,
};
use ffpr::compute::{build_teams, format_record, sum_f64};
use ffpr::models::{PlayerScore, Weights};
use ffpr::raw::{PlayerMap, RawMatchup, RawRoster, RawTransaction, RawUser};

use common::{fixture, typed};

struct Week5 {
    rosters: Vec<RawRoster>,
    users: Vec<RawUser>,
    rosters_by_id: RosterMap,
    players: PlayerMap,
    matchups: Vec<RawMatchup>,
    transactions: Vec<RawTransaction>,
}

fn week5() -> Week5 {
    let rosters: Vec<RawRoster> = typed("rosters.json");
    let rosters_by_id = rosters.iter().map(|r| (r.roster_id, r.clone())).collect();
    Week5 {
        users: typed("users.json"),
        rosters,
        rosters_by_id,
        players: typed("players_subset.json"),
        matchups: typed("matchups_week5.json"),
        transactions: typed("transactions_week5.json"),
    }
}

fn weights() -> Weights {
    Weights::default()
}

#[test]
fn teams_follow_roster_order_and_name_precedence() {
    let data = week5();
    let teams = build_teams(&data.rosters, &data.users);
    assert_eq!(teams.len(), 12);
    let ids: Vec<i64> = teams.keys().copied().collect();
    assert_eq!(ids, (1..=12).collect::<Vec<i64>>());
    // Colors cycle through the fixed palette in roster order.
    assert_eq!(teams[&1].color, "#2a78d6");
    assert_eq!(teams[&9].color, "#8a5a2b");
    // The team name, not the display name, is shown when one is set.
    assert_eq!(teams[&1].name, "To Infinity and Bijan");
}

#[test]
fn week_five_awards_match_the_real_scores() {
    let data = week5();
    let matchups = parse_week_matchups(&data.matchups, 5, &data.rosters_by_id, &data.players);
    assert_eq!(matchups.len(), 12);
    let awards = compute_week_awards(&matchups);
    assert!((awards.highest_score - 187.99).abs() < 1e-9);
    assert!((awards.lowest_score - 83.92).abs() < 1e-9);
    assert!(awards.lowest_score <= awards.highest_score);
    assert!(!awards.best_bench.is_empty());
    assert!(awards
        .best_bench
        .iter()
        .all(|(_, p)| p.points == awards.best_bench_points));
    // Every team's bench total is the sum of its bench players' points.
    for m in &matchups {
        let total: f64 = m.bench.iter().map(|p| p.points).sum();
        assert!((awards.bench_points_by_roster[&m.roster_id] - total).abs() < 1e-9);
    }
}

#[test]
fn all_play_counts_every_other_team_once() {
    let data = week5();
    let matchups = parse_week_matchups(&data.matchups, 5, &data.rosters_by_id, &data.players);
    let allplay = compute_allplay_week(&matchups);
    // Without scoring ties, every pair of teams produces exactly one win.
    let total: i64 = allplay.values().sum();
    assert_eq!(total, 12 * 11 / 2);
    assert!(allplay.values().all(|&wins| (0..=11).contains(&wins)));
}

#[test]
fn head_to_head_counts_the_median_game_when_league_average_match_is_on() {
    let data = week5();
    let matchups = parse_week_matchups(&data.matchups, 5, &data.rosters_by_id, &data.players);
    let plain = compute_weekly_head_to_head(&matchups, false);
    let average = compute_weekly_head_to_head(&matchups, true);
    for m in &matchups {
        assert_eq!(plain[&m.roster_id].1, 1);
        assert_eq!(average[&m.roster_id].1, 2);
    }
}

#[test]
fn failed_transactions_are_excluded_from_roster_moves() {
    let data = week5();
    let completed = data
        .transactions
        .iter()
        .filter(|t| t.status.as_deref() == Some("complete"));
    let adds: usize = completed.clone().map(|t| t.adds.len()).sum();
    let drops: usize = completed.map(|t| t.drops.len()).sum();
    let moves = compute_roster_moves(&data.transactions, &data.players);
    let counted_adds: usize = moves.iter().map(|m| m.added.len()).sum();
    let counted_drops: usize = moves.iter().map(|m| m.dropped.len()).sum();
    assert_eq!(counted_adds, adds);
    assert_eq!(counted_drops, drops);
    assert!(moves.windows(2).all(|w| w[0].roster_id < w[1].roster_id));
}

#[test]
fn a_real_week_gives_a_full_ranking_and_valid_lineups() {
    let data = week5();
    let roster_ids: Vec<i64> = data.rosters.iter().map(|r| r.roster_id).collect();
    let raw = BTreeMap::from([(5, data.matchups.clone())]);
    let txs = BTreeMap::from([(5, data.transactions.clone())]);
    let positions: Vec<String> =
        serde_json::from_value(fixture("league.json")["roster_positions"].clone())
            .expect("league has roster positions");
    let weeks = build_week_summaries(
        &roster_ids,
        &raw,
        &txs,
        &data.rosters_by_id,
        &data.players,
        false,
        &weights(),
        3,
        Some(&positions),
    );
    let week = &weeks[0];
    let ranks: Vec<i64> = week.power_rankings.iter().map(|r| r.rank).collect();
    assert_eq!(ranks, (1..=12).collect::<Vec<i64>>());
    assert!(week.power_rankings.iter().all(|r| r.movement.is_none()));
    // Scores are weighted sums of components in [0, 1], so they stay in [0, 1].
    assert!(week
        .power_rankings
        .iter()
        .all(|r| (0.0..=1.0).contains(&r.score)));
    // The best possible lineup never scores less than the one actually set.
    for row in &week.lineups {
        assert!(row.optimal + 1e-9 >= row.actual);
        assert!(row.left_on_bench >= -1e-9);
    }
}

#[test]
fn best_lineup_fills_fixed_slots_then_flex() {
    use ffpr::compute::matchups::best_lineup;
    let player = |id: &str, pos: &str, points: f64| PlayerScore {
        player_id: id.into(),
        name: id.into(),
        position: pos.into(),
        nfl_team: None,
        points,
    };
    let players = vec![
        player("qb1", "QB", 20.0),
        player("rb1", "RB", 15.0),
        player("rb2", "RB", 12.0),
        player("wr1", "WR", 18.0),
        player("wr2", "WR", 9.0),
        player("te1", "TE", 7.0),
        player("wr3", "WR", 11.0),
    ];
    let slots: Vec<String> = ["QB", "RB", "WR", "TE", "FLEX"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let lineup = best_lineup(&players, &slots, &HashSet::new());
    let mut ids: Vec<&str> = lineup.iter().map(|p| p.player_id.as_str()).collect();
    ids.sort();
    // FLEX takes the best remaining RB/WR/TE: rb2 (12).
    assert_eq!(ids, vec!["qb1", "rb1", "rb2", "te1", "wr1"]);
}

#[test]
fn best_lineup_keeps_the_started_player_on_an_equal_score() {
    use ffpr::compute::matchups::best_lineup;
    let player = |id: &str, points: f64| PlayerScore {
        player_id: id.into(),
        name: id.into(),
        position: "WR".into(),
        nfl_team: None,
        points,
    };
    let players = vec![player("started", 10.0), player("bench", 10.0)];
    let slots = vec!["WR".to_string()];
    let starters: HashSet<&str> = HashSet::from(["started"]);
    let lineup = best_lineup(&players, &slots, &starters);
    assert_eq!(lineup.len(), 1);
    assert_eq!(lineup[0].player_id, "started");
}

#[test]
fn trade_verdict_follows_the_bands() {
    assert_eq!(trade_verdict(&[0.0, 0.0]), ("even", None));
    assert_eq!(trade_verdict(&[10.0, 9.5]), ("fair", None)); // gap under 1 point
    assert_eq!(trade_verdict(&[20.0, 19.0]), ("fair", None)); // under 10% of the bigger side
    assert_eq!(trade_verdict(&[20.0, 18.0]), ("slight edge", Some(0))); // exactly 10%
    assert_eq!(trade_verdict(&[20.0, 16.0]), ("slight edge", Some(0)));
    assert_eq!(trade_verdict(&[20.0, 12.0]), ("favors", Some(0)));
    assert_eq!(trade_verdict(&[20.0, 2.0]), ("lopsided", Some(0)));
    assert_eq!(trade_verdict(&[5.0, 20.0]), ("lopsided", Some(1)));
}

#[test]
fn rank_prior_reads_the_fit_only_inside_its_range() {
    // Better ranks score more: a line with negative slope.
    let samples = vec![
        (Some(1), 20.0, 4),
        (Some(10), 14.0, 4),
        (Some(50), 8.0, 4),
        (Some(120), 3.0, 4),
    ];
    let prior = rank_prior(&samples);
    assert!(prior(Some(1)) > prior(Some(120)));
    // Unranked players read the bottom of the fit, never extrapolated above it.
    assert!(prior(None) <= prior(Some(1)));
    // Too few distinct ranks: everyone gets the position average.
    let flat = rank_prior(&[(Some(5), 10.0, 2), (Some(5), 6.0, 2)]);
    assert_eq!(flat(Some(1)), flat(Some(500)));
}

#[test]
fn superflex_slots_take_quarterbacks_before_flex_players() {
    let player = |id: &str, pos: &str, points: f64| PlayerScore {
        player_id: id.into(),
        name: id.into(),
        position: pos.into(),
        nfl_team: None,
        points,
    };
    let players = vec![
        player("qb1", "QB", 30.0),
        player("qb2", "QB", 25.0),
        player("qb3", "QB", 10.0),
        player("rb1", "RB", 14.0),
        player("wr1", "WR", 13.0),
    ];
    let slots: Vec<String> = ["QB", "SUPER_FLEX"].iter().map(|s| s.to_string()).collect();
    let levels = replacement_levels(&players, &slots, 2);
    // Two QB slots and one superflex slot take the top RB and WR and two QBs,
    // so the best QB left over (10) is the replacement level.
    assert_eq!(levels["QB"], 10.0);
    assert_eq!(levels["RB"], 14.0);
}

#[test]
fn playoff_brackets_and_seeding() {
    assert_eq!(playoff_byes(6), 2);
    assert_eq!(playoff_byes(8), 0);
    assert_eq!(playoff_byes(12), 4);
    // Six teams, one round a week, starting week 15: 15 is the first round.
    assert_eq!(playoff_weeks(6, 15, Some(0)), vec![15, 16, 17]);
    assert!(playoff_weeks(0, 15, Some(0)).is_empty());

    let wins = BTreeMap::from([(1, 8.0), (2, 8.0), (3, 6.0), (4, 9.0)]);
    let pf = BTreeMap::from([(1, 900.0), (2, 950.0), (3, 800.0), (4, 700.0)]);
    assert_eq!(seed_playoffs(&wins, &pf, 3, None), vec![4, 2, 1]);

    // With divisions, each division's best team is seeded ahead of the wild cards:
    // team 4 leads division 2 and team 1 leads division 1; the rest follow by record.
    let divisions = BTreeMap::from([(1, Some(1)), (2, Some(2)), (3, Some(1)), (4, Some(2))]);
    let seeds = seed_playoffs(&wins, &pf, 4, Some(&divisions));
    assert_eq!(seeds, vec![4, 1, 2, 3]);
}

#[test]
fn nfl_byes_and_availability() {
    let schedule = serde_json::json!([
        {"week": 1, "home": "KC", "away": "BAL"},
        {"week": 2, "home": "KC", "away": "DEN"},
        {"week": 2, "home": "BAL", "away": "DEN"},
    ]);
    let byes = nfl_byes(&schedule);
    // A team's bye is a week of the schedule in which it has no game.
    assert!(byes["KC"].is_empty());
    assert!(byes["BAL"].is_empty());
    assert_eq!(byes["DEN"], BTreeSet::from([1]));

    let horizon = [3, 4, 5, 6];
    let avail = availability(Some("Questionable"), Some("KC"), &horizon, &byes);
    assert_eq!(avail, vec![0.75, 1.0, 1.0, 1.0]);
    let ir = availability(Some("IR"), Some("KC"), &horizon, &byes);
    assert_eq!(ir, vec![0.0, 0.0, 0.0, 0.0]);
    let bye = availability(None, Some("DEN"), &[1, 3], &byes);
    assert_eq!(bye, vec![0.0, 1.0]);
}

#[test]
fn records_drop_the_decimal_when_whole() {
    assert_eq!(format_record(8.0, 6.0), "8-6");
    assert_eq!(format_record(7.5, 6.5), "7.5-6.5");
    assert_eq!(format_record(0.0, 0.0), "0-0");
}

#[test]
fn sums_match_cpython_compensation() {
    // CPython 3.12's sum() over floats is compensated; a naive sum gives 0.9999999999999999.
    assert_eq!(sum_f64(std::iter::repeat_n(0.1, 10)), 1.0);
}

#[test]
fn rivalries_follow_the_manager_across_rosters() {
    let data = week5();
    let roster_ids: Vec<i64> = data.rosters.iter().map(|r| r.roster_id).collect();
    let raw = BTreeMap::from([(5, data.matchups.clone())]);
    let weeks = build_week_summaries(
        &roster_ids,
        &raw,
        &BTreeMap::new(),
        &data.rosters_by_id,
        &data.players,
        false,
        &weights(),
        3,
        None,
    );
    let teams = build_teams(&data.rosters, &data.users);
    let season = ffpr::models::SeasonSummary {
        season: "2025".into(),
        league_name: "test".into(),
        teams: teams.clone(),
        weeks,
        season_board: ffpr::compute::season::build_season_board(&[], &teams),
        playoff_week_start: 15,
        through_week: 5,
        provisional_week: None,
        roster_positions: vec![],
        provisional_week_summary: None,
        preseason: vec![],
        draft: None,
        weights: weights(),
        form_window: 3,
        league_average_match: false,
        playoff_odds: vec![],
        playoff_odds_sims: 0,
        playoff_teams: 0,
        playoff_byes: 0,
        remaining_weeks: 0,
        acquisitions: None,
        trade_values: None,
        roster_report: None,
    };
    let managers = ffpr::compute::rivalries::build_rivalries(&[season]);
    // One week of six matchups: every manager has exactly one meeting.
    assert_eq!(managers.len(), 12);
    for manager in &managers {
        assert_eq!(manager.rivals.len(), 1);
        assert_eq!(manager.rivals[0].meetings.len(), 1);
        assert_ne!(manager.rivals[0].opponent_id, manager.owner_id);
    }
}

fn raw<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).expect("valid raw JSON")
}

#[test]
fn float_noise_in_scores_parses_exactly_as_python_does() {
    // 109.46000000000001 is a different double from 109.46 in Python's
    // json.loads; a parser that isn't correctly rounded turns this win into a tie.
    let text = r#"[{"roster_id": 1, "matchup_id": 1, "points": 109.46000000000001},
                   {"roster_id": 2, "matchup_id": 1, "points": 109.46}]"#;
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let matchups = parse_week_matchups(
        &raw::<Vec<RawMatchup>>(value),
        1,
        &RosterMap::new(),
        &PlayerMap::new(),
    );
    let h2h = compute_weekly_head_to_head(&matchups, false);
    assert_eq!(h2h[&1], (1.0, 1));
    assert_eq!(h2h[&2], (0.0, 1));
}

#[test]
fn odd_types_in_transactions_do_not_fail_the_response() {
    let tx: RawTransaction = raw(serde_json::json!({
        "status": "complete",
        "type": "waiver",
        "adds": {"8150": "7", "9000": null},
        "drops": {"4046": 7.0},
        "settings": {"waiver_bid": 5.0},
        "draft_picks": [{"season": null, "round": "4", "owner_id": "7"}],
    }));
    assert_eq!(tx.adds.get("8150"), Some(&7));
    assert!(!tx.adds.contains_key("9000"));
    assert_eq!(tx.drops.get("4046"), Some(&7));
    assert_eq!(tx.settings.waiver_bid, Some(5));
    assert_eq!(tx.draft_picks[0].owner_id, Some(7));
    assert_eq!(tx.draft_picks[0].round, Some(4));
}

#[test]
fn a_negative_form_window_slices_like_python() {
    // scores[-(-1):] is scores[1:]: in week 1 that's empty, so every team's
    // form is 0 and the scaled form is 0.5 across the board.
    let data = week5();
    let roster_ids: Vec<i64> = data.rosters.iter().map(|r| r.roster_id).collect();
    let weeks = build_week_summaries(
        &roster_ids,
        &BTreeMap::from([(1, data.matchups.clone())]),
        &BTreeMap::new(),
        &data.rosters_by_id,
        &data.players,
        false,
        &weights(),
        -1,
        None,
    );
    assert!(weeks[0].power_rankings.iter().all(|r| r.form_norm == 0.5));
}

#[test]
fn award_ties_between_zero_and_negative_zero_keep_the_first_like_python() {
    let value = serde_json::json!([
        {"roster_id": 1, "matchup_id": 1, "points": 10.0, "starters": ["a"], "starters_points": [0.0]},
        {"roster_id": 2, "matchup_id": 1, "points": 9.0, "starters": ["b"], "starters_points": [-0.0]},
    ]);
    let matchups = parse_week_matchups(
        &raw::<Vec<RawMatchup>>(value),
        1,
        &RosterMap::new(),
        &PlayerMap::new(),
    );
    let awards = compute_week_awards(&matchups);
    assert!(awards.best_starter_points.is_sign_positive());
    assert!(awards.worst_starter_points.is_sign_positive());
    assert_eq!(awards.best_starter.len(), 2);
}

#[test]
fn head_to_head_totals_use_python_compensated_sums() {
    use ffpr::models::{HeadToHead, Meeting};
    let meetings = [0.1, 0.2, 0.3]
        .iter()
        .enumerate()
        .map(|(i, pts)| Meeting::new("2025", i as i64 + 1, *pts, 0.0))
        .collect();
    let h2h = HeadToHead::new("rival".into(), meetings);
    assert_eq!(h2h.points_for, 0.6); // a plain fold gives 0.6000000000000001
}

#[test]
fn a_traded_pick_without_a_season_reads_none_like_python() {
    let tx: RawTransaction = raw(serde_json::json!({
        "status": "complete",
        "type": "trade",
        "adds": {"4046": 2},
        "roster_ids": [1, 2],
        "draft_picks": [{"season": null, "round": 4, "owner_id": 2}],
        "status_updated": 1,
    }));
    let summary = ffpr::compute::acquisitions::grade_acquisitions(
        &BTreeMap::from([(3, vec![tx])]),
        &[],
        &PlayerMap::new(),
    );
    let side = summary.trades[0]
        .sides
        .iter()
        .find(|s| s.roster_id == 2)
        .unwrap();
    assert_eq!(side.picks, vec!["None Rd 4".to_string()]);
}

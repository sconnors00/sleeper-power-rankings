from ffpr.compute import (
    POSITIONS,
    build_season_board,
    build_teams,
    build_week_summaries,
    compute_allplay_week,
    compute_position_ranks,
    compute_roster_moves,
    compute_week_awards,
    compute_weekly_head_to_head,
    parse_week_matchups,
    resolve_avatar_url,
    resolve_team_name,
)

WEIGHTS = {"win_pct": 0.30, "allplay_pct": 0.25, "pf_norm": 0.30, "form_norm": 0.15}


def test_build_teams_assigns_names_avatars_and_stable_colors(rosters, users):
    teams = build_teams(rosters, users)
    assert len(teams) == len(rosters)
    for roster in rosters:
        team = teams[roster["roster_id"]]
        assert team.name
        assert team.color.startswith("#")
    colors = [t.color for t in teams.values()]
    assert len(set(colors)) == len(colors)  # every team gets a distinct color


def test_resolve_team_name_prefers_team_name_over_display_name():
    user = {"display_name": "someuser", "metadata": {"team_name": "Custom Name"}}
    assert resolve_team_name(user, 1) == "Custom Name"


def test_resolve_team_name_falls_back_to_display_name():
    user = {"display_name": "someuser", "metadata": {}}
    assert resolve_team_name(user, 1) == "someuser"


def test_resolve_team_name_orphaned_roster_uses_placeholder():
    assert resolve_team_name(None, 7) == "Team 7"


def test_resolve_avatar_url_prefers_full_url_in_metadata():
    user = {"avatar": "somehash", "metadata": {"avatar": "https://sleepercdn.com/uploads/x.jpg"}}
    assert resolve_avatar_url(user) == "https://sleepercdn.com/uploads/x.jpg"


def test_resolve_avatar_url_falls_back_to_thumbs_cdn():
    user = {"avatar": "somehash", "metadata": {}}
    assert resolve_avatar_url(user) == "https://sleepercdn.com/avatars/thumbs/somehash"


def test_resolve_avatar_url_none_when_missing():
    assert resolve_avatar_url({"avatar": None, "metadata": {}}) is None


def test_parse_week_matchups_pairs_opponents_and_splits_bench(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    assert len(matchups) == 12
    by_roster = {m.roster_id: m for m in matchups}
    m1 = by_roster[1]
    assert m1.opponent_roster_id == by_roster[m1.opponent_roster_id].roster_id
    assert by_roster[m1.opponent_roster_id].opponent_roster_id == 1
    # bench = players - starters - reserve - taxi
    assert not (set(p.player_id for p in m1.starters) & set(p.player_id for p in m1.bench))


def test_parse_week_matchups_falls_back_to_points_when_custom_points_null(
    rosters, matchups_week5, players
):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    raw_by_roster = {m["roster_id"]: m for m in matchups_week5}
    for m in matchups:
        raw = raw_by_roster[m.roster_id]
        assert raw["custom_points"] is None
        assert m.team_points == raw["points"]


def test_compute_week_awards_finds_high_low_best_worst(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    awards = compute_week_awards(matchups)

    team_points = {m.roster_id: m.team_points for m in matchups}
    assert awards.highest_score == max(team_points.values())
    assert set(awards.highest_score_roster_ids) == {
        rid for rid, pts in team_points.items() if pts == awards.highest_score
    }
    assert awards.lowest_score == min(team_points.values())

    all_starter_points = [p.points for m in matchups for p in m.starters]
    assert awards.best_starter_points == max(all_starter_points)
    assert awards.worst_starter_points == min(all_starter_points)


def test_compute_week_awards_empty_starter_slots_are_skipped(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    for m in matchups:
        assert all(p.player_id != "0" for p in m.starters)


def test_compute_allplay_week_sums_to_expected_total(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    allplay = compute_allplay_week(matchups)
    n = len(matchups)
    # every pairwise comparison resolves to exactly one team "winning" it
    # (no ties in this fixture), so the counts across all teams sum to
    # the number of ordered pairs with a strict winner.
    total_pairs = n * (n - 1) // 2
    scores = [m.team_points for m in matchups]
    ties = sum(1 for i in range(n) for j in range(i + 1, n) if scores[i] == scores[j])
    assert sum(allplay.values()) == total_pairs - ties


def test_compute_allplay_week_equiv_splits_ties():
    from ffpr.compute import compute_allplay_week_equiv
    from ffpr.models import Matchup

    def mk(rid, pts):
        return Matchup(
            week=1,
            matchup_id=rid,
            roster_id=rid,
            opponent_roster_id=None,
            team_points=pts,
            starters=[],
            bench=[],
        )

    matchups = [mk(1, 100.0), mk(2, 100.0), mk(3, 90.0), mk(4, 110.0)]
    equiv = compute_allplay_week_equiv(matchups)
    # teams 1 and 2 tie each other (0.5), beat team 3 (1), lose to team 4 (0)
    assert equiv[1] == 1.5
    assert equiv[2] == 1.5
    assert equiv[3] == 0.0
    assert equiv[4] == 3.0
    # win-equivalents across the league always sum to the number of pairs
    assert sum(equiv.values()) == 4 * 3 / 2


def test_compute_weekly_head_to_head_with_league_average_match(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    h2h = compute_weekly_head_to_head(matchups, league_average_match=True)
    # every team plays 2 games this week: their h2h opponent + the median game
    for _, games in h2h.values():
        assert games == 2
    # win-equivalents are bounded by games played
    for win_equiv, games in h2h.values():
        assert 0.0 <= win_equiv <= games


def test_compute_weekly_head_to_head_without_league_average_match(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    h2h = compute_weekly_head_to_head(matchups, league_average_match=False)
    for _, games in h2h.values():
        assert games == 1


def test_compute_roster_moves_excludes_failed_and_groups_by_roster(transactions_week5, players):
    moves = compute_roster_moves(transactions_week5, players)
    failed_rosters = set()
    complete_rosters = set()
    for tx in transactions_week5:
        target = complete_rosters if tx["status"] == "complete" else failed_rosters
        target.update((tx.get("adds") or {}).values())
        target.update((tx.get("drops") or {}).values())

    result_rosters = {m.roster_id for m in moves}
    assert result_rosters == complete_rosters
    # every result roster actually has at least one move
    assert all(m.added or m.dropped for m in moves)
    # sorted by roster_id
    assert [m.roster_id for m in moves] == sorted(result_rosters)


def test_compute_roster_moves_waiver_add_carries_faab(transactions_week5, players):
    moves = compute_roster_moves(transactions_week5, players)
    waiver_tx = next(
        t for t in transactions_week5 if t["status"] == "complete" and t["type"] == "waiver"
    )
    add_roster_id = next(iter(waiver_tx["adds"].values()))
    row = next(m for m in moves if m.roster_id == add_roster_id)
    matching = [m for m in row.added if m.move_type == "waiver" and m.faab is not None]
    assert matching  # at least one waiver add on this roster carried its FAAB bid


def test_build_week_summaries_single_week_produces_power_rankings(
    rosters, matchups_week5, transactions_week5, players
):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5},
        {5: transactions_week5},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    assert len(weeks) == 1
    week = weeks[0]
    assert week.week == 5
    ranks = [row.rank for row in week.power_rankings]
    assert sorted(ranks) == list(range(1, len(roster_ids) + 1))
    # single week in the pass with no prior week -> no movement
    assert all(row.movement is None for row in week.power_rankings)
    # luck = win_pct - allplay_pct for every row
    for row in week.power_rankings:
        assert row.luck == row.win_pct - row.allplay_pct
    # roster moves came through from the transactions fixture
    assert week.roster_moves
    assert all(m.added or m.dropped for m in week.roster_moves)


def test_build_week_summaries_movement_present_on_second_week(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    # reuse the same matchup data as a stand-in "week 6" purely to exercise
    # the movement-arrow wiring across weeks
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5, 6: matchups_week5},
        {},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    assert len(weeks) == 2
    assert all(row.movement is not None for row in weeks[1].power_rankings)
    assert weeks[0].roster_moves == []


def test_fit_price_curve_excludes_keepers_and_sorts(draft_picks, players):
    from ffpr.compute import fit_price_curve

    curve = fit_price_curve(draft_picks, players)
    keeper_count = sum(1 for p in draft_picks if p.get("is_keeper"))
    unranked = sum(
        1
        for p in draft_picks
        if not p.get("is_keeper") and (players.get(p["player_id"]) or {}).get("search_rank") is None
    )
    assert len(curve) == len(draft_picks) - keeper_count - unranked
    ranks = [r for r, _ in curve]
    assert ranks == sorted(ranks)


def test_expected_price_tracks_market(draft_picks, players):
    from ffpr.compute import expected_price, fit_price_curve

    curve = fit_price_curve(draft_picks, players)
    top = expected_price(1, curve)
    mid = expected_price(80, curve)
    deep = expected_price(400, curve)
    assert top > mid >= deep >= 1.0
    assert expected_price(None, curve) == 1.0  # team defenses etc.


def test_build_preseason_rankings_carries_over_final_order(rosters, matchups_week5, players):
    from ffpr.compute import build_preseason_rankings

    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5},
        {},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    final = weeks[-1].power_rankings
    pf = {r["roster_id"]: 1000.0 + r["roster_id"] for r in rosters}

    # same rosters, but roster 8's owner changed hands
    current = [dict(r) for r in rosters]
    for r in current:
        if r["roster_id"] == 8:
            r["owner_id"] = "brand-new-owner"

    rows = build_preseason_rankings(
        final, pf, current, rosters, champion_roster_id=final[0].roster_id
    )
    assert [r.roster_id for r in rows] == [r.roster_id for r in final]
    assert [r.rank for r in rows] == list(range(1, 13))
    assert rows[0].champion and not rows[1].champion
    by_rid = {r.roster_id: r for r in rows}
    assert by_rid[8].new_owner
    assert sum(1 for r in rows if r.new_owner) == 1
    assert by_rid[8].prev_pf == pf[8]


def test_build_preseason_rankings_unknown_roster_ranks_last(rosters, matchups_week5, players):
    from ffpr.compute import build_preseason_rankings

    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5},
        {},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    final = weeks[-1].power_rankings
    current = [dict(r) for r in rosters] + [{"roster_id": 99, "owner_id": "someone"}]
    rows = build_preseason_rankings(final, {}, current, rosters, champion_roster_id=None)
    assert rows[-1].roster_id == 99
    assert rows[-1].prev_rank is None
    assert rows[-1].rank == 13


def test_grade_draft_produces_grades_and_steals(draft_picks, players):
    from ffpr.compute import grade_draft

    summary = grade_draft(draft_picks, players, budget=200)
    assert len(summary.teams) == 12
    assert summary.budget == 200
    # sorted by surplus, grades ordered A-side to C/D-side
    surpluses = [t.surplus for t in summary.teams]
    assert surpluses == sorted(surpluses, reverse=True)
    assert summary.teams[0].grade.startswith("A")
    # keepers never graded or listed as steals/overpays
    for t in summary.teams:
        assert all(not g.is_keeper for g in t.picks)
        assert all(g.is_keeper for g in t.keepers)
    assert all(not g.is_keeper for g in summary.steals + summary.overpays)
    assert all(g.surplus > 0 for g in summary.steals)
    assert all(g.surplus < 0 and g.amount >= 5 for g in summary.overpays)


def test_build_season_board_crowns_and_pf_leaderboard(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    users_data = []
    teams = build_teams(rosters, users_data)
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5},
        {},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    board = build_season_board(weeks, teams)
    assert sum(board.crown_counts.values()) == len(weeks[0].awards.highest_score_roster_ids)
    pf_sorted = [row.pf for row in board.pf_leaderboard]
    assert pf_sorted == sorted(pf_sorted, reverse=True)
    for row in board.pf_leaderboard:
        assert row.avg == row.pf  # only one week of data, so avg == total


def test_build_season_board_records_span_full_season(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    roster_ids = [r["roster_id"] for r in rosters]
    teams = build_teams(rosters, [])
    # reuse week 5's data as a stand-in week 6 so records have two weeks to
    # actually span, exercising the season-wide (not just per-week) tracking
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5, 6: matchups_week5},
        {},
        rosters_by_id,
        players,
        league_average_match=True,
        weights=WEIGHTS,
        form_window=3,
    )
    board = build_season_board(weeks, teams)
    records = board.records

    all_starter_points = [p.points for wk in weeks for m in wk.matchups for p in m.starters]
    assert records.highest_starter[0][0][1].points == max(all_starter_points)
    assert records.lowest_starter[0][0][1].points == min(all_starter_points)

    all_team_points = [m.team_points for wk in weeks for m in wk.matchups]
    assert records.highest_team_score[2] == max(all_team_points)
    assert records.lowest_team_score[2] == min(all_team_points)

    # every closest/blowout game record actually has that season's margin
    assert all(g.margin == records.closest_games[0].margin for g in records.closest_games)
    assert all(g.margin == records.biggest_blowouts[0].margin for g in records.biggest_blowouts)
    # closest margin is never worse (bigger) than the blowout margin
    assert records.closest_games[0].margin <= records.biggest_blowouts[0].margin
    for g in records.closest_games + records.biggest_blowouts:
        assert round(g.margin, 6) == round(abs(g.points_a - g.points_b), 6)


def _team(rid, *starters):
    from ffpr.models import Matchup, PlayerScore

    players = [
        PlayerScore(f"p{rid}{i}", f"P{i}", pos, None, pts) for i, (pos, pts) in enumerate(starters)
    ]
    return Matchup(1, rid, rid, None, sum(pts for _, pts in starters), players, [])


def test_compute_position_ranks_on_real_week(rosters, matchups_week5, players):
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    rows = compute_position_ranks(matchups)

    assert sorted(r.roster_id for r in rows) == sorted(m.roster_id for m in matchups)
    by_rid = {m.roster_id: m for m in matchups}
    for row in rows:
        starters_total = sum(p.points for p in by_rid[row.roster_id].starters)
        assert round(sum(row.points.values()), 2) == round(starters_total, 2)
    for pos in POSITIONS:
        best = max(rows, key=lambda r: r.points[pos])
        assert best.ranks[pos] == 1
        assert all(1 <= r.ranks[pos] <= len(rows) for r in rows)


def test_compute_position_ranks_flex_counts_at_own_position():
    """A superflex QB lands in the QB total, not in some FLEX bucket."""
    rows = compute_position_ranks(
        [_team(1, ("QB", 20.0), ("QB", 15.0), ("RB", 10.0)), _team(2, ("QB", 25.0), ("RB", 30.0))]
    )
    two_qb = next(r for r in rows if r.roster_id == 1)
    assert two_qb.points["QB"] == 35.0
    assert two_qb.ranks["QB"] == 1
    assert two_qb.ranks["RB"] == 2


def test_compute_position_ranks_ties_share_rank():
    rows = compute_position_ranks(
        [_team(1, ("K", 9.0)), _team(2, ("K", 9.0)), _team(3, ("K", 4.0)), _team(4, ("K", 12.0))]
    )
    ranks = {r.roster_id: r.ranks["K"] for r in rows}
    assert ranks == {4: 1, 1: 2, 2: 2, 3: 4}


def test_compute_position_ranks_ignores_unknown_positions():
    rows = compute_position_ranks([_team(1, ("DL", 8.0), ("DEF", 6.0))])
    assert rows[0].points == {"QB": 0.0, "RB": 0.0, "WR": 0.0, "TE": 0.0, "K": 0.0, "DEF": 6.0}


def _p(pid, pos, pts):
    from ffpr.models import PlayerScore

    return PlayerScore(pid, pid, pos, None, pts)


def test_best_lineup_fills_fixed_then_flex_slots():
    from ffpr.compute import best_lineup

    players = [
        _p("qb1", "QB", 20),
        _p("qb2", "QB", 15),
        _p("rb1", "RB", 10),
        _p("rb2", "RB", 8),
        _p("wr1", "WR", 12),
        _p("wr2", "WR", 5),
        _p("te1", "TE", 7),
    ]
    lineup = best_lineup(players, ["QB", "RB", "WR", "FLEX", "SUPER_FLEX", "BN"], set())
    assert sorted(p.player_id for p in lineup) == ["qb1", "qb2", "rb1", "rb2", "wr1"]


def test_best_lineup_exact_when_flex_slots_overlap():
    """Greedy would put the WR in WRRB_FLEX and leave REC_FLEX the TE (11 pts)."""
    from ffpr.compute import best_lineup

    players = [_p("wr", "WR", 10), _p("rb", "RB", 9), _p("te", "TE", 1)]
    lineup = best_lineup(players, ["WRRB_FLEX", "REC_FLEX"], set())
    assert sum(p.points for p in lineup) == 19


def test_best_lineup_fills_a_slot_even_when_negative():
    from ffpr.compute import best_lineup

    lineup = best_lineup([_p("d", "DEF", -3.0)], ["DEF"], {"d"})
    assert [p.player_id for p in lineup] == ["d"]


def test_lineup_efficiency_ignores_equal_point_swaps():
    from ffpr.compute import compute_lineup_efficiency
    from ffpr.models import Matchup

    m = Matchup(1, 1, 1, None, 0.0, [_p("bye", "WR", 0.0)], [_p("benchbye", "WR", 0.0)])
    row = compute_lineup_efficiency([m], ["WR"])[0]
    assert row.left_on_bench == 0
    assert row.should_have_started == [] and row.should_have_sat == []


def test_lineup_efficiency_on_real_week(rosters, matchups_week5, players, league):
    from ffpr.compute import compute_lineup_efficiency

    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    rows = compute_lineup_efficiency(matchups, league["roster_positions"])
    assert len(rows) == len(matchups)
    for row in rows:
        assert row.optimal >= row.actual
        assert 0 < row.efficiency <= 1
        for benched, starter in row.swaps:
            if starter is not None and starter.position == benched.position:
                assert benched.points >= starter.points
    assert any(row.left_on_bench > 0 for row in rows)


def test_pair_swaps_flags_an_empty_slot():
    from ffpr.compute import pair_swaps

    pairs = pair_swaps([_p("a", "WR", 12.0), _p("b", "RB", 8.0)], [_p("c", "WR", 0.0)])
    assert [(i.player_id, o.player_id if o else None) for i, o in pairs] == [
        ("a", "c"),
        ("b", None),
    ]


def test_pair_swaps_matches_like_for_like_first():
    """Best-in/worst-out pairing read as "Engram for Dart", a downgrade."""
    from ffpr.compute import pair_swaps

    started = [_p("lawrence", "QB", 26.24), _p("engram", "TE", 13.30)]
    sat = [_p("slayton", "WR", 4.10), _p("dart", "QB", 15.58)]
    pairs = [(i.player_id, o.player_id) for i, o in pair_swaps(started, sat)]
    assert pairs == [("lawrence", "dart"), ("engram", "slayton")]


def test_playoff_byes_fill_to_a_power_of_two():
    from ffpr.compute import playoff_byes

    assert [playoff_byes(n) for n in (4, 6, 7, 8)] == [0, 2, 1, 0]


def test_seed_playoffs_by_wins_then_points_for():
    from ffpr.compute import seed_playoffs

    wins = {1: 5, 2: 5, 3: 4, 4: 6}
    pf = {1: 400, 2: 500, 3: 900, 4: 300}
    assert seed_playoffs(wins, pf, 3) == [4, 2, 1]


def test_seed_playoffs_division_winners_first():
    """2024: roster 6 won its division and took a bye over wild card roster 8."""
    from ffpr.compute import seed_playoffs

    wins = {9: 9, 8: 9, 6: 9, 7: 9, 11: 9, 10: 7}
    pf = {9: 2186.71, 8: 2168.44, 6: 2152.15, 7: 2126.81, 11: 2026.03, 10: 2169.95}
    divisions = {9: 3, 8: 3, 6: 2, 7: 3, 11: 1, 10: 2}
    assert seed_playoffs(wins, pf, 6, divisions) == [9, 6, 11, 8, 7, 10]


def _odds_inputs(rosters, matchups_week5, players):
    from ffpr.compute import build_week_summaries, schedule_pairs

    weeks = build_week_summaries(
        [r["roster_id"] for r in rosters],
        {5: matchups_week5},
        {},
        {r["roster_id"]: r for r in rosters},
        players,
        league_average_match=False,
        weights=WEIGHTS,
        form_window=3,
    )
    pairs = schedule_pairs(matchups_week5)
    return weeks, {wk: pairs for wk in range(6, 15)}


def test_playoff_odds_are_consistent(rosters, matchups_week5, players):
    from ffpr.compute import simulate_playoff_odds

    weeks, remaining = _odds_inputs(rosters, matchups_week5, players)
    rows = simulate_playoff_odds(weeks, remaining, False, 6, sims=400, seed=1)
    assert len(rows) == len(rosters)
    assert abs(sum(r.playoff_pct for r in rows) - 6) < 1e-9
    assert abs(sum(r.bye_pct for r in rows) - 2) < 1e-9
    assert abs(sum(r.top_seed_pct for r in rows) - 1) < 1e-9
    assert rows == simulate_playoff_odds(weeks, remaining, False, 6, sims=400, seed=1)
    # 1 win so far + 9 games, one win per game across the league
    assert abs(sum(r.projected_wins for r in rows) - 6 * 10) < 1e-6


def test_playoff_odds_count_the_median_game(rosters, matchups_week5, players):
    from ffpr.compute import simulate_playoff_odds

    weeks, remaining = _odds_inputs(rosters, matchups_week5, players)
    rows = simulate_playoff_odds(weeks, remaining, True, 6, sims=200, seed=1)
    # every week is now worth 12 wins: 6 head-to-head + 6 against the median
    assert abs(sum(r.projected_wins for r in rows) - 12 * 10) < 1e-6
    assert all("-" in r.record for r in rows)


def test_playoff_odds_lock_in_a_clinched_team(rosters, matchups_week5, players):
    from ffpr.compute import simulate_playoff_odds

    weeks, remaining = _odds_inputs(rosters, matchups_week5, players)
    leader = max(weeks[0].matchups, key=lambda m: m.team_points).roster_id
    last = {14: remaining[14]}
    rows = {r.roster_id: r for r in simulate_playoff_odds(weeks * 12, last, False, 6, sims=200)}
    assert rows[leader].playoff_pct == 1.0


def _week(num, *team_starters):
    from types import SimpleNamespace

    from ffpr.models import Matchup

    matchups = [
        Matchup(num, rid, rid, None, sum(p.points for p in starters), list(starters), [])
        for rid, starters in team_starters
    ]
    return SimpleNamespace(week=num, matchups=matchups)


def test_grade_acquisitions_splits_credit_between_stints():
    from ffpr.compute import grade_acquisitions

    txs = {
        1: [
            {
                "type": "waiver",
                "status": "complete",
                "adds": {"p1": 1},
                "settings": {"waiver_bid": 10},
            },
            {
                "type": "waiver",
                "status": "failed",
                "adds": {"p1": 3},
                "settings": {"waiver_bid": 9},
            },
        ],
        3: [
            {
                "type": "trade",
                "status": "complete",
                "adds": {"p1": 2},
                "roster_ids": [1, 2],
                "draft_picks": [{"season": "2027", "round": 2, "owner_id": 1}],
            }
        ],
    }
    weeks = [
        _week(1, (1, [_p("p1", "WR", 10.0)])),
        _week(2, (1, [_p("p1", "WR", 20.0)])),
        _week(3, (2, [_p("p1", "WR", 30.0)]), (1, [])),
        _week(4, (2, [])),  # benched: no credit
    ]
    summary = grade_acquisitions(txs, weeks, {})

    assert [(a.roster_id, a.player.points, a.starts, a.faab) for a in summary.pickups] == [
        (1, 30.0, 2, 10)
    ]
    [trade] = summary.trades
    assert [s.roster_id for s in trade.sides] == [2, 1]
    assert trade.sides[0].points == 30.0 and trade.sides[0].received[0].starts == 1
    assert trade.sides[1].received == [] and trade.sides[1].picks == ["2027 Rd 2"]


def test_grade_acquisitions_on_a_real_week(rosters, matchups_week5, transactions_week5, players):
    from types import SimpleNamespace

    from ffpr.compute import grade_acquisitions

    rosters_by_id = {r["roster_id"]: r for r in rosters}
    matchups = parse_week_matchups(matchups_week5, 5, rosters_by_id, players)
    summary = grade_acquisitions(
        {5: transactions_week5}, [SimpleNamespace(week=5, matchups=matchups)], players
    )
    completed_adds = sum(
        len(t.get("adds") or {})
        for t in transactions_week5
        if t["status"] == "complete" and t["type"] != "trade"
    )
    assert len(summary.pickups) == completed_adds
    started = {(m.roster_id, p.player_id): p.points for m in matchups for p in m.starters}
    for a in summary.pickups:
        assert a.player.points == started.get((a.roster_id, a.player.player_id), 0.0)
    [trade] = summary.trades
    assert sorted(s.roster_id for s in trade.sides) == [1, 10]
    assert all(len(s.received) == 3 for s in trade.sides)


def _season(year, owners, *weeks):
    """weeks: (week, [(matchup_id, roster_id, points), ...])"""
    from types import SimpleNamespace

    from ffpr.models import Matchup, Team

    teams = {
        rid: Team(rid, owner, f"{owner}-{year}", None, "#000") for rid, owner in owners.items()
    }
    return SimpleNamespace(
        season=year,
        teams=teams,
        weeks=[
            SimpleNamespace(
                week=num,
                matchups=[Matchup(num, mid, rid, None, pts, [], []) for mid, rid, pts in games],
            )
            for num, games in weeks
        ],
    )


def test_rivalries_follow_the_manager_not_the_roster():
    from ffpr.compute import build_rivalries

    older = _season("2024", {1: "ann", 2: "bob"}, (1, [(1, 1, 100.0), (1, 2, 90.0)]))
    # roster 1 changed hands: 'cat' must not inherit ann's record
    newer = _season(
        "2025",
        {1: "cat", 2: "bob"},
        (1, [(1, 1, 80.0), (1, 2, 95.0)]),
        (2, [(1, 1, 70.0), (1, 2, 60.0)]),
    )
    managers = {m.owner_id: m for m in build_rivalries([newer, older])}

    bob = {h.opponent_id: h for h in managers["bob"].rivals}
    assert bob["ann"].record == "0-1"
    assert bob["cat"].record == "1-1"
    assert managers["ann"].seasons == ["2024"]
    assert managers["bob"].seasons == ["2025", "2024"]
    assert managers["bob"].name == "bob-2025"  # latest team name
    assert bob["cat"].streak == ("L", 1)


def test_rivalry_highlights():
    from ffpr.compute import build_rivalries, rivalry_highlights

    weeks = [
        (w, [(1, 1, 100.0 + w), (1, 2, 90.0), (2, 3, 80.0), (2, 4, 80.0 + (w % 2) * 20)])
        for w in range(1, 7)
    ]
    season = _season("2025", {1: "ann", 2: "bob", 3: "cat", 4: "dan"}, *weeks)
    hl = rivalry_highlights(build_rivalries([season]))
    assert hl["one_sided"][0].owner_id == "ann" and hl["one_sided"][1].record == "6-0"
    assert hl["even"][1].record in ("3-0-3", "0-3-3")
    assert hl["streak"][0].owner_id == "ann" and hl["streak"][2] == 6


def test_active_streak_needs_both_managers_still_in_the_league():
    from ffpr.compute import build_rivalries, rivalry_highlights

    # ann beat bob every week in 2024, then bob left; cat and ann split 2025
    older = _season(
        "2024", {1: "ann", 2: "bob"}, *[(w, [(1, 1, 100.0), (1, 2, 90.0)]) for w in range(1, 6)]
    )
    newer = _season(
        "2025",
        {1: "ann", 2: "cat"},
        (1, [(1, 1, 100.0), (1, 2, 90.0)]),
        (2, [(1, 1, 80.0), (1, 2, 95.0)]),
    )
    hl = rivalry_highlights(build_rivalries([newer, older]))
    assert hl["streak"][0].owner_id == "cat" and hl["streak"][2] == 1
    assert hl["one_sided"][1].record == "5-0"  # history still counts for the other cards

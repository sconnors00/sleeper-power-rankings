from ffpr.compute import (
    build_season_board,
    build_teams,
    build_week_summaries,
    compute_allplay_week,
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

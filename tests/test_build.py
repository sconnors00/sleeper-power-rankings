import json
import re

from ffpr.build import build_data_js, render_site
from ffpr.compute import build_season_board, build_teams, build_week_summaries
from ffpr.models import SeasonSummary

WEIGHTS = {"win_pct": 0.30, "allplay_pct": 0.25, "pf_norm": 0.30, "form_norm": 0.15}


def _make_season(rosters, users, matchups_week5, transactions_week5, players, league):
    roster_ids = [r["roster_id"] for r in rosters]
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    teams = build_teams(rosters, users)
    weeks = build_week_summaries(
        roster_ids,
        {5: matchups_week5},
        {5: transactions_week5},
        rosters_by_id,
        players,
        league_average_match=bool(league["settings"]["league_average_match"]),
        weights=WEIGHTS,
        form_window=3,
        roster_positions=league["roster_positions"],
    )
    board = build_season_board(weeks, teams)
    return SeasonSummary(
        season=league["season"],
        league_name=league["name"],
        teams=teams,
        weeks=weeks,
        season_board=board,
        playoff_week_start=league["settings"]["playoff_week_start"],
        through_week=5,
        provisional_week=None,
        roster_positions=league["roster_positions"],
    )


def test_render_site_produces_every_page(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)

    assert (out / "index.html").exists()
    assert (out / "season.html").exists()
    assert (out / "weeks" / "week-5.html").exists()
    assert (out / "data.js").exists()
    assert (out / "static" / "style.css").exists()
    assert (out / "static" / "app.js").exists()

    week_html = (out / "weeks" / "week-5.html").read_text()
    assert "Week 5" in week_html
    assert 'href="../static/style.css?v=' in week_html  # relative asset paths


def test_build_data_js_is_valid_json_payload(
    rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    content = build_data_js(season)
    assert content.startswith("window.FFPR = ")
    assert content.rstrip().endswith(";")
    payload = json.loads(content[len("window.FFPR = ") : -2])
    assert len(payload["teams"]) == len(rosters)
    assert payload["rankTrajectory"]["weeks"] == [5]


def test_build_data_js_includes_provisional_week_matchups(
    rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import build_provisional_week_summary

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    # reuse week 5's raw data as a stand-in in-progress week 6
    season.provisional_week_summary = build_provisional_week_summary(
        6, matchups_week5, transactions_week5, rosters_by_id, players
    )
    season.provisional_week = 6
    content = build_data_js(season)
    payload = json.loads(content[len("window.FFPR = ") : -2])
    # the provisional week's page needs its scores chart data...
    assert "6" in payload["weekMatchups"]
    # ...but it must stay out of the rankings and season-level series
    assert payload["rankTrajectory"]["weeks"] == [5]
    assert [s["week"] for s in payload["scoringSpread"]] == [5]


def test_render_site_landing_page_when_no_completed_weeks(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert (out / "index.html").exists()
    assert not (out / "season.html").exists()
    html = (out / "index.html").read_text()
    assert "Week 1 rankings land Tuesday morning" in html
    assert "Pre-season power rankings" not in html  # no preseason data -> no table


def _make_preseason_rows(teams_only_season):
    from ffpr.models import PreseasonRow

    rids = sorted(teams_only_season.teams)
    return [
        PreseasonRow(
            roster_id=rid,
            rank=i + 1,
            prev_rank=i + 1,
            prev_record="8-6",
            prev_pf=1500.0 + i,
            new_owner=(i == 3),
            champion=(i == 0),
        )
        for i, rid in enumerate(rids)
    ]


def test_render_site_landing_page_with_preseason_rankings(tmp_path, teams_only_season):
    teams_only_season.preseason = _make_preseason_rows(teams_only_season)
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    html = (out / "index.html").read_text()
    assert "Pre-season power rankings" in html
    assert "new owner" in html
    assert "&#127942;" in html  # champion trophy
    assert "#1 (8-6)" in html


def test_render_site_writes_week0_when_preseason_present(tmp_path, teams_only_season):
    teams_only_season.preseason = _make_preseason_rows(teams_only_season)
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert (out / "weeks" / "week-0.html").exists()
    html = (out / "weeks" / "week-0.html").read_text()
    assert "Pre-season power rankings" in html
    # the week picker names the page it's on
    assert '<option value="../weeks/week-0.html" selected>Preseason</option>' in html


def test_render_site_no_week0_without_preseason_data(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert not (out / "weeks" / "week-0.html").exists()


def test_render_site_writes_season_recap_when_complete(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    # pretend the regular season (up through this league's real
    # playoff_week_start) is fully built, using week 5's data as a stand-in
    season.playoff_week_start = 6
    season.through_week = 5
    out = tmp_path / "site"
    render_site(season, out)
    assert (out / "weeks" / "week-6.html").exists()
    html = (out / "weeks" / "week-6.html").read_text()
    assert "Season recap" in html
    assert "Highest scoring team" in html
    assert "Lowest scoring active player" in html
    assert "Closest game of the year" in html
    assert "Biggest blowout of the year" in html
    assert '<option value="../weeks/week-6.html" selected>Season recap</option>' in html


def test_render_site_no_season_recap_when_incomplete(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    # league fixture's real playoff_week_start (15) is far beyond week 5
    out = tmp_path / "site"
    render_site(season, out)
    assert not (out / "weeks" / f"week-{season.playoff_week_start}.html").exists()


def test_render_site_draft_page(tmp_path, teams_only_season, draft_picks, players):
    from ffpr.compute import grade_draft

    teams_only_season.draft = grade_draft(draft_picks, players, budget=200)
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert (out / "draft.html").exists()
    html = (out / "draft.html").read_text()
    assert "Draft grades" in html
    assert "Biggest steals" in html
    # every team's grade shows up
    for team in teams_only_season.draft.teams:
        assert teams_only_season.teams[team.roster_id].name in html


def test_asset_urls_carry_version_token(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    """data.js lives at a fixed URL, so pages must request a versioned copy."""
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)

    index = (out / "index.html").read_text()
    token = re.search(r'data\.js\?v=([0-9a-f]+)"', index).group(1)
    assert f'static/app.js?v={token}"' in index
    assert f'static/style.css?v={token}"' in index


def test_asset_version_changes_when_data_changes(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    """A rebuilt week must not be servable from a browser's cached payload."""
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)

    render_site(season, tmp_path / "a")
    before = re.search(r'data\.js\?v=([0-9a-f]+)"', (tmp_path / "a" / "index.html").read_text())

    season.weeks[-1].matchups[0].team_points += 1.0
    render_site(season, tmp_path / "b")
    after = re.search(r'data\.js\?v=([0-9a-f]+)"', (tmp_path / "b" / "index.html").read_text())

    assert before.group(1) != after.group(1)


def test_week_page_shows_position_rankings(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "weeks" / "week-5.html").read_text()
    section = html.split("<h2>Position rankings</h2>")[1].split("</section>")[0]
    for pos in ("QB", "RB", "WR", "TE", "K", "DEF"):
        assert f'<th class="num">{pos}</th>' in section
    assert section.count("team-cell") == len(rosters)
    assert "pos-best" in section and "pos-worst" in section


def test_position_rankings_hidden_before_kickoff(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import build_provisional_week_summary

    unplayed = [
        {
            **m,
            "points": 0.0,
            "custom_points": None,
            "starters_points": [0.0] * len(m["starters"]),
            "players_points": {pid: 0.0 for pid in m.get("players") or []},
        }
        for m in matchups_week5
    ]
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    season.provisional_week_summary = build_provisional_week_summary(
        6, unplayed, [], rosters_by_id, players
    )
    season.provisional_week = 6
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "weeks" / "week-6.html").read_text()
    assert "Position rankings appear once this week" in html


def test_how_it_works_page_explains_the_real_ranking(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.weights = WEIGHTS
    season.form_window = 3
    out = tmp_path / "site"
    render_site(season, out)

    html = (out / "how-it-works.html").read_text()
    top = season.weeks[-1].power_rankings[0]
    assert season.teams[top.roster_id].name in html
    assert f"{top.score:.3f}" in html
    assert "30%" in html and "25%" in html and "15%" in html
    assert "last 3 weeks" in html
    assert "all 11 other teams" in html
    assert "median" not in html  # this league plays no median game

    assert 'href="how-it-works.html"' in (out / "index.html").read_text()
    assert 'href="../how-it-works.html#power"' in (out / "weeks" / "week-5.html").read_text()


def test_how_it_works_worked_example_adds_up(
    rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.build import _how_it_works_context

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    example = _how_it_works_context(season)["example"]
    total = sum(t["value"] * t["weight"] for t in example["terms"])
    assert abs(total - example["score"]) < 1e-9


def test_how_it_works_reflects_configured_weights_and_median_game(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.weights = {"win_pct": 0.40, "allplay_pct": 0.20, "pf_norm": 0.20, "form_norm": 0.20}
    season.league_average_match = True
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "how-it-works.html").read_text()
    assert "40%" in html
    assert "median score" in html


def test_how_it_works_page_exists_before_week_one(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    html = (out / "how-it-works.html").read_text()
    assert "How the rankings work" in html
    assert "Worked example" not in html


def test_lineup_efficiency_on_week_and_season_pages(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)

    week = (out / "weeks" / "week-5.html").read_text()
    section = week.split("<h2>Lineup efficiency</h2>")[1].split("</section>")[0]
    assert section.count("team-cell") == len(rosters)
    worst = max(season.weeks[-1].lineups, key=lambda r: r.left_on_bench)
    assert "Costliest lineup call" in week
    assert f"{worst.left_on_bench:.2f}" in week

    board = (out / "season.html").read_text()
    assert "<h2>Lineup efficiency</h2>" in board
    assert "of 1</td>" in board  # perfect weeks out of weeks played


def test_season_page_shows_playoff_odds(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import schedule_pairs, simulate_playoff_odds

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    remaining = {wk: schedule_pairs(matchups_week5) for wk in range(6, 15)}
    season.playoff_odds = simulate_playoff_odds(season.weeks, remaining, False, 6, sims=300)
    season.playoff_odds_sims, season.playoff_teams, season.playoff_byes = 300, 6, 2
    season.remaining_weeks = len(remaining)
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "season.html").read_text()
    section = html.split('id="playoff-odds"')[1].split("</section>")[0]
    assert "300 simulations of the remaining 9 regular-season weeks" in section
    assert "top 2 get first-round byes" in section
    assert section.count("team-cell") == len(rosters)


def test_season_page_omits_playoff_odds_without_them(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)
    assert 'id="playoff-odds"' not in (out / "season.html").read_text()


def test_pct_hedges_the_extremes():
    from ffpr.build import _pct

    assert [_pct(0), _pct(0.4567), _pct(1)] == ["<0.1%", "45.7%", ">99.9%"]


def test_season_page_grades_pickups_and_trades(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import grade_acquisitions

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.acquisitions = grade_acquisitions({5: transactions_week5}, season.weeks, players)
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "season.html").read_text()

    best = season.acquisitions.pickups[0]
    pickups = html.split('id="pickups"')[1].split("</section>")[0]
    assert best.player.name in pickups and f"{best.player.points:.2f}" in pickups
    assert pickups.count("<tr>") <= 11  # header + at most ten

    # Trades are graded on their own page; the season page points there.
    pointer = html.split('id="trades"')[1].split("</section>")[0]
    assert "1 trade this season" in pointer and 'href="trades.html#history"' in pointer

    page = (out / "trades.html").read_text()
    history = page.split('id="history"')[1].split("</section>")[0]
    assert "Week 5" in history
    for side in season.acquisitions.trades[0].sides:
        assert season.teams[side.roster_id].name in history
    # No trade values (a finished season): history only, no calculator.
    assert "Value now" not in history
    assert 'id="calculator"' not in page
    assert 'href="trades.html"' in (out / "index.html").read_text()


def test_rivalries_page(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import build_rivalries

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    rivalries = build_rivalries([season])
    out = tmp_path / "site"
    render_site(season, out, rivalries=rivalries)

    html = (out / "rivalries.html").read_text()
    assert html.count('<details class="rivalry">') == len(rivalries)
    for team in season.teams.values():
        if team.owner_name and team.owner_id in {m.owner_id for m in rivalries}:
            assert team.owner_name in html
            if team.name != team.owner_name:
                assert team.name not in html
    assert "All-time standings" in html
    assert 'href="rivalries.html"' in (out / "index.html").read_text()


def test_no_rivalries_page_without_rivalries(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert not (out / "rivalries.html").exists()
    assert "rivalries.html" not in (out / "index.html").read_text()


def test_how_it_works_covers_the_new_features(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.playoff_teams, season.playoff_byes = 6, 2
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "how-it-works.html").read_text()
    for anchor in ("playoff-odds", "lineups", "pickups", "rivalries"):
        assert f'id="{anchor}"' in html
    assert "10,000 times" in html
    assert "top 6 make it, with the top 2 getting byes" in html
    assert "league median" not in html.split('id="playoff-odds"')[1].split("</section>")[0]


def _with_trade_values(season, rosters, matchups_week5, transactions_week5, players, league):
    from ffpr.compute import build_trade_values, grade_acquisitions

    season.acquisitions = grade_acquisitions({5: transactions_week5}, season.weeks, players)
    season.trade_values = build_trade_values(
        {5: matchups_week5},
        rosters,
        players,
        league["roster_positions"],
        weeks_left=9,
        trade_deadline=12,
    )
    return season


def test_trades_page_has_the_calculator_and_analysis(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from markupsafe import escape

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    _with_trade_values(season, rosters, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "trades.html").read_text()

    calculator = html.split('id="calculator"')[1].split("</section>")[0]
    assert 'id="trade-calculator"' in calculator
    assert calculator.count('<select class="trade-team">') == 2
    assert calculator.count("<option ") == 2 * len(rosters)
    assert "Trade deadline: Week 12." in calculator

    history = html.split('id="history"')[1].split("</section>")[0]
    assert "Value now" in history and "going forward" in history

    strength = html.split('id="strength"')[1].split("</section>")[0]
    assert strength.count("team-cell") == len(rosters)
    assert "pos-best" in strength and "pos-worst" in strength

    values = html.split('id="values"')[1].split("</section>")[0]
    assert values.count('<details class="fold">') == 6
    top = max(season.trade_values.players.values(), key=lambda pv: pv.value)
    assert str(escape(top.player.name)) in values
    assert f"{top.value:.2f}" in values
    assert 'href="trades.html"' in (out / "index.html").read_text()


def test_trade_deadline_note_once_it_has_passed(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    _with_trade_values(season, rosters, matchups_week5, transactions_week5, players, league)
    season.trade_values.trade_deadline = 5
    out = tmp_path / "site"
    render_site(season, out)
    assert "The trade deadline (Week 5) has passed" in (out / "trades.html").read_text()


def test_data_js_carries_the_calculator_payload(
    rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import FAIR_TRADE_GAP, LINEUP_FIT_GAP, TRADE_VERDICT_BANDS

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    _with_trade_values(season, rosters, matchups_week5, transactions_week5, players, league)
    content = build_data_js(season)
    trade = json.loads(content[len("window.FFPR = ") : -2])["trade"]

    assert trade["rosterPositions"] == league["roster_positions"]
    assert trade["flex"]["SUPER_FLEX"] == ["QB", "RB", "TE", "WR"]
    assert (trade["rosterLimit"], trade["weeksLeft"]) == (17, 9)
    assert (trade["fairGap"], trade["bands"]) == (FAIR_TRADE_GAP, list(TRADE_VERDICT_BANDS))
    assert set(trade["rosters"]) == {str(r["roster_id"]) for r in rosters}
    for pids in trade["rosters"].values():
        for pid in pids:
            pv = season.trade_values.players[pid]
            assert trade["players"][pid]["value"] == pv.value
            assert trade["players"][pid]["proj"] == pv.projection
            assert trade["players"][pid]["active"] == pv.active
            assert trade["players"][pid]["avail"] == pv.availability
    assert trade["fitGap"] == LINEUP_FIT_GAP
    assert trade["replacement"] == season.trade_values.replacement
    assert (trade["horizon"], trade["weights"], trade["playoffFrom"]) == ([], [], None)
    assert trade["playoffPct"] == {}

    season.trade_values = None
    content = build_data_js(season)
    assert "trade" not in json.loads(content[len("window.FFPR = ") : -2])


def test_no_trades_page_without_trades_or_values(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert not (out / "trades.html").exists()
    assert "trades.html" not in (out / "index.html").read_text()


def test_how_it_works_explains_trade_values(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from markupsafe import escape

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "plain"
    render_site(season, out)
    assert 'id="trade-values"' not in (out / "how-it-works.html").read_text()

    _with_trade_values(season, rosters, matchups_week5, transactions_week5, players, league)
    out = tmp_path / "site"
    render_site(season, out)
    html = (out / "how-it-works.html").read_text()
    section = html.split('id="trade-values"')[1].split("</section>")[0]
    assert "counts as 5 games of evidence" in section
    assert "Superflex spots mostly go to quarterbacks" in section
    levels = season.trade_values.replacement
    assert f"replacement level is QB {levels['QB']:.2f}, RB {levels['RB']:.2f}" in section
    top = max(
        (pv for pv in season.trade_values.players.values() if pv.roster_id and pv.games),
        key=lambda pv: pv.value,
    )
    assert f"Worked example: {escape(top.player.name)}" in section
    assert f"= {top.projection:.2f}" in section
    assert "Byes and injuries" in section
    assert "Weeks he's expected to play" not in section  # no weeks left to scale by


def test_trade_values_show_availability_with_weeks_left(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import build_trade_values

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.trade_values = build_trade_values(
        {5: matchups_week5},
        rosters,
        players,
        league["roster_positions"],
        weeks_left=9,
        horizon={6: 1.0, 7: 1.0, 15: 0.5},
        playoff_from=15,
        byes={team: {7} for team in {p.get("team") for p in players.values()} if team},
    )
    out = tmp_path / "site"
    render_site(season, out)
    values = (out / "trades.html").read_text().split('id="values"')[1].split("</section>")[0]
    assert ">Avail.</th>" in values and "of the 3 weeks left, playoffs included," in values

    section = (out / "how-it-works.html").read_text().split('id="trade-values"')[1]
    top = max(
        (pv for pv in season.trade_values.players.values() if pv.roster_id and pv.games),
        key=lambda pv: pv.value,
    )
    share = (sum(w * a for w, a in zip([1.0, 1.0, 0.5], top.availability, strict=True)) / 2.5) * 100
    assert f"{top.surplus:.2f} &times; {share:.1f}% = {top.value:.2f}" in section
    assert "10%" in section and "25%" in section and "50%" in section


def test_rosters_page_and_json_agree(
    tmp_path, rosters, users, matchups_week5, transactions_week5, players, league
):
    from ffpr.compute import build_roster_report

    season = _make_season(rosters, users, matchups_week5, transactions_week5, players, league)
    season.roster_report = build_roster_report(
        rosters, players, league["roster_positions"], 200, {5: transactions_week5}
    )
    out = tmp_path / "site"
    render_site(season, out)

    data = json.loads((out / "rosters.json").read_text())
    assert data["season"] == season.season and data["faab_budget"] == 200
    assert [t["roster_id"] for t in data["teams"]] == sorted(r["roster_id"] for r in rosters)
    for t in data["teams"]:
        assert t["faab"]["used"] + t["faab"]["remaining"] == 200
        assert set(t) >= {"starters", "bench", "ir", "taxi", "faab_claims", "record", "owner"}
    assert (out / "rosters.json").read_text() == (
        render_site(season, tmp_path / "again") or (tmp_path / "again" / "rosters.json").read_text()
    )  # deterministic: no timestamps

    html = (out / "rosters.html").read_text()
    assert 'href="rosters.json"' in html
    faab = html.split('id="faab"')[1].split("</section>")[0]
    for t in data["teams"]:
        assert f"${t['faab']['remaining']}" in faab
        assert f'id="team-{t["roster_id"]}"' in html
    assert html.count('id="team-') == len(rosters)
    assert 'href="rosters.html"' in (out / "index.html").read_text()


def test_no_rosters_page_without_a_report(tmp_path, teams_only_season):
    out = tmp_path / "site"
    render_site(teams_only_season, out)
    assert not (out / "rosters.html").exists() and not (out / "rosters.json").exists()


def _css_tokens(css: str, selector: str) -> dict[str, str]:
    """The --custom-property: value pairs declared in `selector { ... }`."""
    body = css.split(selector + " {", 1)[1].split("}", 1)[0]
    return dict(re.findall(r"(--[\w-]+):\s*([^;]+);", body))


def test_dark_tokens_in_sync():
    """style.css writes the dark theme twice (follow-the-OS, and the theme
    button's explicit choice); a token added to one block must be in both."""
    from ffpr.build import STATIC_DIR

    css = (STATIC_DIR / "style.css").read_text()
    light = _css_tokens(css, ":root")
    os_dark = _css_tokens(css, ':root:not([data-theme="light"])')
    picked_dark = _css_tokens(css, ':root[data-theme="dark"]')
    assert os_dark and os_dark == picked_dark
    assert set(os_dark) <= set(light)

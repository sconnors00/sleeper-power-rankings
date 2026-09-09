import json

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
    assert 'href="../static/style.css"' in week_html  # relative asset paths


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
    assert '<option value="../weeks/week-0.html">Preseason</option>' in html


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
    assert '<option value="../weeks/week-6.html">Season recap</option>' in html


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

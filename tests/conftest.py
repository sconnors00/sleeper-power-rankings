import json
from pathlib import Path

import pytest

FIXTURES = Path(__file__).parent / "fixtures"


@pytest.fixture
def rosters():
    return json.loads((FIXTURES / "rosters.json").read_text())


@pytest.fixture
def users():
    return json.loads((FIXTURES / "users.json").read_text())


@pytest.fixture
def matchups_week5():
    return json.loads((FIXTURES / "matchups_week5.json").read_text())


@pytest.fixture
def players():
    return json.loads((FIXTURES / "players_subset.json").read_text())


@pytest.fixture
def league():
    return json.loads((FIXTURES / "league.json").read_text())


@pytest.fixture
def draft_picks():
    return json.loads((FIXTURES / "draft_picks.json").read_text())


@pytest.fixture
def teams_only_season(rosters, users, league):
    from ffpr.compute import build_season_board, build_teams
    from ffpr.models import SeasonSummary

    teams = build_teams(rosters, users)
    board = build_season_board([], teams)
    return SeasonSummary(
        season=league["season"],
        league_name=league["name"],
        teams=teams,
        weeks=[],
        season_board=board,
        playoff_week_start=league["settings"]["playoff_week_start"],
        through_week=0,
        provisional_week=None,
        roster_positions=league["roster_positions"],
    )

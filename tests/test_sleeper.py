import json

import pytest

from ffpr.sleeper import SleeperClient, week_has_scores

SCORED = [{"roster_id": 1, "points": 101.5}, {"roster_id": 2, "points": 97.0}]
UNPLAYED = [{"roster_id": 1, "points": 0.0}, {"roster_id": 2, "points": 0.0}]


@pytest.fixture
def client(tmp_path):
    with SleeperClient(tmp_path) as c:
        yield c


def _stub_fetch(client, response):
    """Point the client at a canned response and record every request."""
    calls = []

    def fake_fetch(path):
        calls.append(path)
        return response

    client._fetch = fake_fetch
    return calls


def test_week_has_scores_distinguishes_played_from_scheduled():
    assert week_has_scores(SCORED)
    assert not week_has_scores(UNPLAYED)
    assert not week_has_scores([])


def test_completed_week_ignores_all_zero_cache_and_refetches(client, tmp_path):
    """The week-1 bug: a snapshot taken before kickoff must not pin the week."""
    cache_file = tmp_path / "2026" / "raw" / "matchups_week1.json"
    cache_file.parent.mkdir(parents=True)
    cache_file.write_text(json.dumps(UNPLAYED))

    calls = _stub_fetch(client, SCORED)
    result = client.get_matchups("L1", "2026", 1, completed=True)

    assert result == SCORED
    assert calls, "an all-zero cache should be refetched, not trusted"
    assert json.loads(cache_file.read_text()) == SCORED


def test_completed_week_serves_scored_cache_without_refetching(client, tmp_path):
    cache_file = tmp_path / "2026" / "raw" / "matchups_week1.json"
    cache_file.parent.mkdir(parents=True)
    cache_file.write_text(json.dumps(SCORED))

    calls = _stub_fetch(client, UNPLAYED)
    result = client.get_matchups("L1", "2026", 1, completed=True)

    assert result == SCORED
    assert not calls


def test_unplayed_week_is_returned_but_never_cached(client, tmp_path):
    calls = _stub_fetch(client, UNPLAYED)
    result = client.get_matchups("L1", "2026", 2, completed=False)

    assert result == UNPLAYED
    assert calls
    assert not (tmp_path / "2026" / "raw" / "matchups_week2.json").exists()


def test_in_progress_fetch_refreshes_a_stale_cache(client, tmp_path):
    """The current season is refetched: a week keeps changing after it ends."""
    cache_file = tmp_path / "2026" / "raw" / "transactions_week1.json"
    cache_file.parent.mkdir(parents=True)
    cache_file.write_text(json.dumps([{"type": "waiver", "status": "complete"}]))

    fresh = [{"type": "waiver", "status": "complete"} for _ in range(3)]
    calls = _stub_fetch(client, fresh)
    result = client.get_transactions("L1", "2026", 1, completed=False)

    assert result == fresh
    assert calls, "a current-season week must not be served from cache"
    assert json.loads(cache_file.read_text()) == fresh


def test_historical_transactions_still_served_from_cache(client, tmp_path):
    """A finished season is immutable, so it stays a cache hit."""
    cached = [{"type": "waiver", "status": "complete"}]
    cache_file = tmp_path / "2024" / "raw" / "transactions_week1.json"
    cache_file.parent.mkdir(parents=True)
    cache_file.write_text(json.dumps(cached))

    calls = _stub_fetch(client, [])
    assert client.get_transactions("L1", "2024", 1, completed=True) == cached
    assert not calls

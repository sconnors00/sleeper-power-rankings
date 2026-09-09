"""Sleeper API client with an on-disk cache under data/<season>/raw/."""

from __future__ import annotations

import json
import time
from pathlib import Path
from typing import Any

import httpx

BASE_URL = "https://api.sleeper.app/v1"
PLAYERS_MAX_AGE_SECONDS = 24 * 60 * 60


class SleeperError(RuntimeError):
    pass


class SleeperClient:
    """Thin wrapper over the Sleeper read-only API.

    Completed-week responses are cached to disk and reused; the in-progress
    week and league/roster/user data are always refetched, falling back to a
    cached copy if the network is unavailable (so offline builds still work).
    """

    def __init__(self, data_dir: Path, timeout: float = 15.0) -> None:
        self.data_dir = data_dir
        self._client = httpx.Client(base_url=BASE_URL, timeout=timeout)

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> SleeperClient:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()

    def _raw_dir(self, season: str) -> Path:
        d = self.data_dir / season / "raw"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def _fetch(self, path: str) -> Any:
        resp = self._client.get(path)
        resp.raise_for_status()
        return resp.json()

    def _fetch_with_fallback_cache(self, path: str, cache_file: Path) -> Any:
        """Always try the network; fall back to a cached copy if it fails."""
        try:
            data = self._fetch(path)
        except httpx.HTTPError as exc:
            if cache_file.exists():
                return json.loads(cache_file.read_text())
            raise SleeperError(f"GET {path} failed and no cache available: {exc}") from exc
        cache_file.write_text(json.dumps(data))
        return data

    def _fetch_cache_first(self, path: str, cache_file: Path) -> Any:
        """Use the cache if present; otherwise fetch and cache the result."""
        if cache_file.exists():
            return json.loads(cache_file.read_text())
        data = self._fetch(path)
        cache_file.write_text(json.dumps(data))
        return data

    def get_state(self) -> dict[str, Any]:
        return self._fetch("/state/nfl")

    def get_league(self, league_id: str) -> dict[str, Any]:
        # Cached by league_id, not season: the season string isn't known
        # until this call returns.
        cache_dir = self.data_dir / "leagues"
        cache_dir.mkdir(parents=True, exist_ok=True)
        cache_file = cache_dir / f"{league_id}.json"
        return self._fetch_with_fallback_cache(f"/league/{league_id}", cache_file)

    def get_rosters(self, league_id: str, season: str) -> list[dict[str, Any]]:
        cache_file = self._raw_dir(season) / "rosters.json"
        return self._fetch_with_fallback_cache(f"/league/{league_id}/rosters", cache_file)

    def get_users(self, league_id: str, season: str) -> list[dict[str, Any]]:
        cache_file = self._raw_dir(season) / "users.json"
        return self._fetch_with_fallback_cache(f"/league/{league_id}/users", cache_file)

    def get_matchups(
        self, league_id: str, season: str, week: int, *, completed: bool
    ) -> list[dict[str, Any]]:
        cache_file = self._raw_dir(season) / f"matchups_week{week}.json"
        if completed:
            return self._fetch_cache_first(f"/league/{league_id}/matchups/{week}", cache_file)
        return self._fetch_with_fallback_cache(f"/league/{league_id}/matchups/{week}", cache_file)

    def get_transactions(
        self, league_id: str, season: str, week: int, *, completed: bool
    ) -> list[dict[str, Any]]:
        cache_file = self._raw_dir(season) / f"transactions_week{week}.json"
        if completed:
            return self._fetch_cache_first(f"/league/{league_id}/transactions/{week}", cache_file)
        return self._fetch_with_fallback_cache(
            f"/league/{league_id}/transactions/{week}", cache_file
        )

    def get_draft(self, draft_id: str) -> dict[str, Any]:
        cache_dir = self.data_dir / "drafts"
        cache_dir.mkdir(parents=True, exist_ok=True)
        return self._fetch_with_fallback_cache(f"/draft/{draft_id}", cache_dir / f"{draft_id}.json")

    def get_draft_picks(self, draft_id: str, *, completed: bool) -> list[dict[str, Any]]:
        cache_dir = self.data_dir / "drafts"
        cache_dir.mkdir(parents=True, exist_ok=True)
        cache_file = cache_dir / f"{draft_id}_picks.json"
        if completed:
            return self._fetch_cache_first(f"/draft/{draft_id}/picks", cache_file)
        return self._fetch_with_fallback_cache(f"/draft/{draft_id}/picks", cache_file)

    def get_players(self) -> dict[str, Any]:
        cache_file = self.data_dir / "players_nfl.json"
        if cache_file.exists():
            age = time.time() - cache_file.stat().st_mtime
            if age < PLAYERS_MAX_AGE_SECONDS:
                return json.loads(cache_file.read_text())
        try:
            data = self._fetch("/players/nfl")
        except httpx.HTTPError as exc:
            if cache_file.exists():
                return json.loads(cache_file.read_text())
            raise SleeperError(f"GET /players/nfl failed and no cache available: {exc}") from exc
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(json.dumps(data))
        return data

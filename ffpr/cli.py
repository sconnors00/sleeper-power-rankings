"""Typer CLI: build, serve, blurb."""

from __future__ import annotations

import functools
import http.server
import os
import socketserver
import tomllib
from pathlib import Path

import httpx
import typer

from ffpr.build import render_site
from ffpr.compute import (
    apply_official_records,
    build_preseason_rankings,
    build_provisional_week_summary,
    build_season_board,
    build_teams,
    build_week_summaries,
    grade_draft,
)
from ffpr.models import PreseasonRow, SeasonSummary
from ffpr.sleeper import SleeperClient, SleeperError

app = typer.Typer(help="Sleeper fantasy football power rankings static site generator.")

DEFAULT_WEIGHTS = {"win_pct": 0.30, "allplay_pct": 0.25, "pf_norm": 0.30, "form_norm": 0.15}
DEFAULT_FORM_WINDOW = 3
CONFIG_PATH = Path("config.toml")
DATA_DIR = Path("data")
SITE_DIR = Path("site")


def load_config() -> dict:
    if CONFIG_PATH.exists():
        return tomllib.loads(CONFIG_PATH.read_text())
    return {}


def resolve_league_id(league_override: str | None, config: dict) -> str:
    league_id = league_override or os.environ.get("SLEEPER_LEAGUE_ID") or config.get("league_id")
    if not league_id:
        raise typer.BadParameter(
            "No league id. Pass --league, set SLEEPER_LEAGUE_ID, or set league_id in config.toml."
        )
    return league_id


def _find_through_week(
    client: SleeperClient,
    league_id: str,
    season: str,
    roster_ids: list[int],
    start_week: int,
    playoff_week_start: int,
    is_current_season: bool,
) -> int:
    """Walk backward from start_week until every roster has points > 0."""
    for week in range(min(start_week, playoff_week_start - 1), 0, -1):
        # Only the current season's leading week can still be in progress;
        # past seasons are fully completed, so always trust the cache there.
        completed = not (is_current_season and week == start_week)
        raw = client.get_matchups(league_id, season, week, completed=completed)
        if raw and all(
            any(m["roster_id"] == rid and (m.get("points") or 0) > 0 for m in raw)
            for rid in roster_ids
        ):
            return week
    return 0


def _fetch_draft(client: SleeperClient, league_obj: dict) -> tuple[dict, list[dict]] | None:
    """(draft, picks) for the league's completed auction draft, or None.

    Only feeds draft.html's grading -- pre-season rankings are roster-talent
    based and don't need this. Best-effort: no draft, a snake draft, or a
    fetch failure with no cache just means no draft grades.
    """
    draft_id = league_obj.get("draft_id")
    if not draft_id:
        return None
    try:
        draft = client.get_draft(draft_id)
        if draft.get("type") != "auction" or draft.get("status") != "complete":
            return None
        picks = client.get_draft_picks(draft_id, completed=True)
    except (SleeperError, httpx.HTTPError):
        return None
    return (draft, picks) if picks else None


def _build_preseason(
    client: SleeperClient,
    league_obj: dict,
    current_rosters: list[dict],
    players: dict,
) -> list[PreseasonRow]:
    """Roster-talent rankings before week 1 -- only the players on a roster
    affect this, never draft prices or last season's results.
    """
    prev_rosters: list[dict] = []
    prev_id = league_obj.get("previous_league_id")
    if prev_id:
        try:
            prev_league = client.get_league(prev_id)
            prev_rosters = client.get_rosters(prev_id, prev_league["season"])
        except (SleeperError, httpx.HTTPError):
            prev_rosters = []

    champion_raw = (league_obj.get("metadata") or {}).get("latest_league_winner_roster_id")
    champion_roster_id = int(champion_raw) if champion_raw and str(champion_raw).isdigit() else None

    return build_preseason_rankings(
        current_rosters,
        prev_rosters,
        players,
        league_obj["roster_positions"],
        champion_roster_id,
    )


def _build_season_summary(
    client: SleeperClient,
    league_id: str,
    season_override: str | None,
    through_override: int | None,
    provisional: bool,
    weights: dict[str, float],
    form_window: int,
) -> SeasonSummary:
    state = client.get_state()
    league_obj = client.get_league(league_id)
    season = season_override or league_obj["season"]
    if season_override and season_override != league_obj["season"]:
        raise typer.BadParameter(
            f"--season {season_override} doesn't match league {league_id}'s season "
            f"({league_obj['season']})."
        )

    rosters = client.get_rosters(league_id, season)
    users = client.get_users(league_id, season)
    players = client.get_players()
    roster_ids = [r["roster_id"] for r in rosters]
    rosters_by_id = {r["roster_id"]: r for r in rosters}
    teams = build_teams(rosters, users)

    playoff_week_start = league_obj["settings"]["playoff_week_start"]
    league_average_match = bool(league_obj["settings"].get("league_average_match", 0))
    is_current_season = season == state["season"]
    start_week = state["week"] if is_current_season else playoff_week_start - 1

    if through_override is not None:
        through_week = min(through_override, playoff_week_start - 1)
    else:
        through_week = _find_through_week(
            client, league_id, season, roster_ids, start_week, playoff_week_start, is_current_season
        )

    weeks_raw = {
        wk: client.get_matchups(league_id, season, wk, completed=True)
        for wk in range(1, through_week + 1)
    }
    week_summaries = build_week_summaries(
        roster_ids, weeks_raw, rosters_by_id, players, league_average_match, weights, form_window
    )
    if week_summaries:
        apply_official_records(week_summaries[-1].power_rankings, rosters_by_id)
    season_board = build_season_board(week_summaries, teams)

    provisional_week_summary = None
    provisional_week_num = None
    if provisional:
        candidate = start_week if is_current_season else through_week + 1
        if through_week < candidate < playoff_week_start:
            raw = client.get_matchups(league_id, season, candidate, completed=False)
            if raw:
                provisional_week_summary = build_provisional_week_summary(
                    candidate, raw, rosters_by_id, players
                )
                provisional_week_num = candidate

    draft_data = _fetch_draft(client, league_obj)
    draft_summary = None
    if draft_data is not None:
        draft, picks = draft_data
        draft_summary = grade_draft(picks, players, draft.get("settings", {}).get("budget", 0))

    preseason: list[PreseasonRow] = []
    if through_week == 0:
        preseason = _build_preseason(client, league_obj, rosters, players)

    return SeasonSummary(
        season=season,
        league_name=league_obj["name"],
        teams=teams,
        weeks=week_summaries,
        season_board=season_board,
        playoff_week_start=playoff_week_start,
        through_week=through_week,
        provisional_week=provisional_week_num,
        roster_positions=league_obj["roster_positions"],
        provisional_week_summary=provisional_week_summary,
        preseason=preseason,
        draft=draft_summary,
    )


@app.command()
def build(
    through: int | None = typer.Option(None, help="Build only through this week."),
    provisional: bool = typer.Option(
        False, help="Also include the in-progress week, excluded from rankings."
    ),
    league: str | None = typer.Option(None, "--league", help="Sleeper league id override."),
    season: str | None = typer.Option(
        None, "--season", help="Season override (must match league)."
    ),
) -> None:
    """Fetch (using the cache), compute, and write site/."""
    config = load_config()
    league_id = resolve_league_id(league, config)
    weights = config.get("weights", DEFAULT_WEIGHTS)
    form_window = config.get("form", {}).get("window", DEFAULT_FORM_WINDOW)
    site_url = config.get("site_url", "")

    with SleeperClient(DATA_DIR) as client:
        season_summary = _build_season_summary(
            client, league_id, season, through, provisional, weights, form_window
        )

    render_site(season_summary, SITE_DIR, site_url=site_url)
    msg = f"Built site/ through week {season_summary.through_week}"
    if season_summary.provisional_week:
        msg += f" (+ provisional week {season_summary.provisional_week})"
    typer.echo(msg)


@app.command()
def serve(port: int = typer.Option(8000, help="Local port to serve site/ on.")) -> None:
    """Serve site/ at http://localhost:<port> for local preview."""
    if not SITE_DIR.exists():
        raise typer.BadParameter("site/ doesn't exist yet -- run `ffpr build` first.")
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(SITE_DIR))
    with socketserver.TCPServer(("", port), handler) as httpd:
        typer.echo(f"Serving site/ at http://localhost:{port}/ (Ctrl+C to stop)")
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            pass


@app.command()
def blurb(
    week: int | None = typer.Option(None, "--week", help="Week to summarize (default: latest)."),
    league: str | None = typer.Option(None, "--league", help="Sleeper league id override."),
    season: str | None = typer.Option(
        None, "--season", help="Season override (must match league)."
    ),
) -> None:
    """Print a short chat-ready summary for pasting into the league chat."""
    config = load_config()
    league_id = resolve_league_id(league, config)
    weights = config.get("weights", DEFAULT_WEIGHTS)
    form_window = config.get("form", {}).get("window", DEFAULT_FORM_WINDOW)
    site_url = config.get("site_url", "")

    with SleeperClient(DATA_DIR) as client:
        season_summary = _build_season_summary(
            client, league_id, season, week, False, weights, form_window
        )

    if not season_summary.weeks:
        if season_summary.preseason:
            teams = season_summary.teams
            lines = [f"{season_summary.league_name} -- pre-season power rankings"]
            for row in season_summary.preseason[:3]:
                note = " (new owner)" if row.new_owner else ""
                lines.append(f"  {row.rank}. {teams[row.roster_id].name}{note}")
            if site_url:
                lines.append(site_url)
            typer.echo("\n".join(lines))
        else:
            typer.echo(f"{season_summary.league_name}: no completed weeks yet.")
        raise typer.Exit()

    wk = season_summary.weeks[-1]
    teams = season_summary.teams
    awards = wk.awards

    lines = [f"{season_summary.league_name} -- Week {wk.week} recap"]
    high_names = ", ".join(teams[rid].name for rid in awards.highest_score_roster_ids)
    lines.append(f"Top score: {high_names} ({awards.highest_score:.2f})")
    low_names = ", ".join(teams[rid].name for rid in awards.lowest_score_roster_ids)
    lines.append(f"Low score: {low_names} ({awards.lowest_score:.2f})")
    if awards.best_bench:
        bench_names = ", ".join(f"{p.name} ({teams[rid].name})" for rid, p in awards.best_bench)
        lines.append(f"Best bench: {bench_names} ({awards.best_bench_points:.2f})")
    lines.append("Power rankings top 3:")
    for row in wk.power_rankings[:3]:
        lines.append(f"  {row.rank}. {teams[row.roster_id].name} ({row.record})")
    if site_url:
        lines.append(site_url)

    typer.echo("\n".join(lines))

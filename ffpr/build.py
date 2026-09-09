"""Renders site/ from a SeasonSummary using Jinja2, plus site/data.js for Chart.js."""

from __future__ import annotations

import json
import shutil
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from ffpr.models import SeasonSummary

PACKAGE_ROOT = Path(__file__).resolve().parent.parent
TEMPLATES_DIR = PACKAGE_ROOT / "templates"
STATIC_DIR = PACKAGE_ROOT / "static"

CHART_JS_URL = "https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js"
CHART_JS_SRI = "sha384-jb8JQMbMoBUzgWatfe6COACi2ljcDdZQ2OxczGA3bGNeWe+6DChMTBJemed7ZnvJ"


def _fmt2(value: float) -> str:
    return f"{value:.2f}"


def _build_jinja_env() -> Environment:
    env = Environment(
        loader=FileSystemLoader(str(TEMPLATES_DIR)),
        autoescape=select_autoescape(["html"]),
        trim_blocks=True,
        lstrip_blocks=True,
    )
    env.filters["fmt2"] = _fmt2
    return env


def _week_url(week: int) -> str:
    return f"weeks/week-{week}.html"


def _year_url(year: str) -> str:
    return f"{year}/index.html"


def build_data_js(season: SeasonSummary) -> str:
    """Serialize everything the charts need into window.FFPR = {...}."""
    teams = {
        str(rid): {
            "name": t.name,
            "color": t.color,
            "avatar": t.avatar_url,
        }
        for rid, t in season.teams.items()
    }

    week_numbers = [w.week for w in season.weeks]

    rank_trajectory = {
        "weeks": week_numbers,
        "series": {
            str(rid): [
                next((row.rank for row in wk.power_rankings if row.roster_id == rid), None)
                for wk in season.weeks
            ]
            for rid in season.teams
        },
    }

    # Charts on every week page need that week's matchups -- including the
    # provisional week, which is excluded from rankings but still gets a page.
    chart_weeks = list(season.weeks)
    if season.provisional_week_summary is not None:
        chart_weeks.append(season.provisional_week_summary)

    week_matchups = {}
    for wk in chart_weeks:
        rows = []
        for m in wk.matchups:
            opp = next((x for x in wk.matchups if x.roster_id == m.opponent_roster_id), None)
            rows.append(
                {
                    "rosterId": m.roster_id,
                    "points": m.team_points,
                    "opponentRosterId": m.opponent_roster_id,
                    "opponentPoints": opp.team_points if opp else None,
                    "margin": (m.team_points - opp.team_points) if opp else None,
                }
            )
        week_matchups[str(wk.week)] = rows

    weekly_points = {
        "weeks": week_numbers,
        "series": {
            str(rid): [
                next((m.team_points for m in wk.matchups if m.roster_id == rid), None)
                for wk in season.weeks
            ]
            for rid in season.teams
        },
        "leagueAvg": [
            (sum(m.team_points for m in wk.matchups) / len(wk.matchups) if wk.matchups else 0)
            for wk in season.weeks
        ],
    }

    scoring_spread = []
    for wk in season.weeks:
        if not wk.matchups:
            continue
        scores = sorted((m.team_points, m.roster_id) for m in wk.matchups)
        low_score, low_rid = scores[0]
        high_score, high_rid = scores[-1]
        mid = len(scores) // 2
        median = scores[mid][0] if len(scores) % 2 else (scores[mid - 1][0] + scores[mid][0]) / 2
        scoring_spread.append(
            {
                "week": wk.week,
                "low": low_score,
                "high": high_score,
                "median": median,
                "lowRosterId": low_rid,
                "highRosterId": high_rid,
            }
        )

    last_week = season.weeks[-1] if season.weeks else None
    luck = []
    pf_vs_pa = []
    bench_points = []
    if last_week is not None:
        for row in sorted(last_week.power_rankings, key=lambda r: r.roster_id):
            luck.append(
                {
                    "rosterId": row.roster_id,
                    "allplayPct": row.allplay_pct,
                    "winPct": row.win_pct,
                    "record": row.record,
                    "allplayRecord": row.allplay_record,
                }
            )
    for row in season.season_board.pf_leaderboard:
        pf_vs_pa.append({"rosterId": row.roster_id, "pf": row.pf, "pa": row.pa})
        bench_points.append({"rosterId": row.roster_id, "points": row.bench_points})

    payload = {
        "season": season.season,
        "leagueName": season.league_name,
        "throughWeek": season.through_week,
        "provisionalWeek": season.provisional_week,
        "playoffWeekStart": season.playoff_week_start,
        "teams": teams,
        "weekMatchups": week_matchups,
        "rankTrajectory": rank_trajectory,
        "weeklyPointsByTeam": weekly_points,
        "scoringSpread": scoring_spread,
        "luck": luck,
        "pfVsPa": pf_vs_pa,
        "benchPoints": bench_points,
    }
    return "window.FFPR = " + json.dumps(payload) + ";\n"


def _matchup_pair(wk, matchup_id: int, teams: dict) -> list[dict]:
    pair = [m for m in wk.matchups if m.matchup_id == matchup_id]
    return [{"team": teams[m.roster_id], "points": m.team_points} for m in pair]


def _all_week_numbers(season: SeasonSummary) -> list[int]:
    weeks = [w.week for w in season.weeks]
    if season.provisional_week_summary is not None:
        weeks = weeks + [season.provisional_week_summary.week]
    return weeks


def _week_context(season: SeasonSummary, wk, asset_prefix: str, is_index: bool) -> dict:
    teams = season.teams
    matchups_by_roster = {m.roster_id: m for m in wk.matchups}
    rankings_frozen = not wk.power_rankings and bool(season.weeks)
    ranking_source = season.weeks[-1].power_rankings if rankings_frozen else wk.power_rankings
    ranking_rows = []
    for row in ranking_source:
        team = teams[row.roster_id]
        m = matchups_by_roster.get(row.roster_id)
        ranking_rows.append(
            {
                "row": row,
                "team": team,
                "week_points": m.team_points if m else None,
            }
        )

    results = []
    seen_matchup_ids = set()
    for m in wk.matchups:
        if m.matchup_id in seen_matchup_ids or m.opponent_roster_id is None:
            continue
        seen_matchup_ids.add(m.matchup_id)
        opp = matchups_by_roster.get(m.opponent_roster_id)
        if opp is None:
            continue
        winner = m if m.team_points >= opp.team_points else opp
        loser = opp if winner is m else m
        results.append(
            {
                "winner_team": teams[winner.roster_id],
                "winner_points": winner.team_points,
                "loser_team": teams[loser.roster_id],
                "loser_points": loser.team_points,
                "margin": winner.team_points - loser.team_points,
            }
        )
    results.sort(key=lambda r: r["margin"])

    awards = wk.awards
    closest = [_matchup_pair(wk, mid, teams) for mid in awards.closest_matchup_ids]
    blowouts = [_matchup_pair(wk, mid, teams) for mid in awards.biggest_blowout_matchup_ids]
    roster_moves_rows = [{"team": teams[m.roster_id], "moves": m} for m in wk.roster_moves]

    return {
        "season": season,
        "week": wk,
        "teams": teams,
        "ranking_rows": ranking_rows,
        "results": results,
        "closest_matchups": closest,
        "blowout_matchups": blowouts,
        "roster_moves_rows": roster_moves_rows,
        "highest_score_teams": [teams[rid] for rid in awards.highest_score_roster_ids],
        "lowest_score_teams": [teams[rid] for rid in awards.lowest_score_roster_ids],
        "best_bench": [(teams[rid], p) for rid, p in awards.best_bench],
        "best_starter": [(teams[rid], p) for rid, p in awards.best_starter],
        "worst_starter": [(teams[rid], p) for rid, p in awards.worst_starter],
        "asset_prefix": asset_prefix,
        "is_index": is_index,
        "all_weeks": _all_week_numbers(season),
        "week_url": _week_url,
        "playoff": wk.week >= season.playoff_week_start,
        "provisional": wk.week == season.provisional_week,
        "rankings_frozen": rankings_frozen,
    }


def render_site(
    season: SeasonSummary,
    output_dir: Path,
    site_url: str = "",
    all_seasons: list[str] | None = None,
    site_root_prefix: str = "",
) -> None:
    """Render one season's site into output_dir.

    For a multi-season build, call this once per season with output_dir set
    to a per-season subdirectory (plus once more at the true site root for
    whichever season should be the default landing page). `all_seasons` (every
    built season, for the year switcher in the nav) and `site_root_prefix`
    (this season's relative path back to the site root, "" if this call's
    output_dir *is* the site root) stay the same across all those calls
    except for site_root_prefix, which is "" only for the root call.
    """
    output_dir.mkdir(parents=True, exist_ok=True)
    weeks_dir = output_dir / "weeks"
    weeks_dir.mkdir(parents=True, exist_ok=True)

    static_out = output_dir / "static"
    if static_out.exists():
        shutil.rmtree(static_out)
    shutil.copytree(STATIC_DIR, static_out)

    (output_dir / "data.js").write_text(build_data_js(season))

    env = _build_jinja_env()
    common = {
        "site_url": site_url,
        "chart_js_url": CHART_JS_URL,
        "chart_js_sri": CHART_JS_SRI,
        "league_name": season.league_name,
        "season_year": season.season,
        "has_draft": season.draft is not None,
        "has_season": bool(season.weeks),
        "all_seasons": all_seasons or [season.season],
        "site_root_prefix": site_root_prefix,
        "year_url": _year_url,
    }

    week_template = env.get_template("week.html")

    if season.draft is not None:
        draft_template = env.get_template("draft.html")
        draft_html = draft_template.render(
            **common,
            draft=season.draft,
            teams=season.teams,
            asset_prefix="",
        )
        (output_dir / "draft.html").write_text(draft_html)

    if not season.weeks:
        template = env.get_template("landing.html")
        preseason_rows = [
            {"row": row, "team": season.teams[row.roster_id]} for row in season.preseason
        ]
        html = template.render(**common, asset_prefix="", preseason_rows=preseason_rows)
        (output_dir / "index.html").write_text(html)
        if season.provisional_week_summary is not None:
            ctx = _week_context(
                season, season.provisional_week_summary, asset_prefix="../", is_index=False
            )
            html = week_template.render(**common, **ctx)
            (weeks_dir / f"week-{season.provisional_week_summary.week}.html").write_text(html)
        return

    for wk in season.weeks:
        ctx = _week_context(season, wk, asset_prefix="../", is_index=False)
        html = week_template.render(**common, **ctx)
        (weeks_dir / f"week-{wk.week}.html").write_text(html)

    if season.provisional_week_summary is not None:
        ctx = _week_context(
            season, season.provisional_week_summary, asset_prefix="../", is_index=False
        )
        html = week_template.render(**common, **ctx)
        (weeks_dir / f"week-{season.provisional_week_summary.week}.html").write_text(html)

    latest = season.weeks[-1]
    ctx = _week_context(season, latest, asset_prefix="", is_index=True)
    html = week_template.render(**common, **ctx)
    (output_dir / "index.html").write_text(html)

    season_template = env.get_template("season.html")
    season_html = season_template.render(
        **common,
        season=season,
        teams=season.teams,
        board=season.season_board,
        asset_prefix="",
        all_weeks=_all_week_numbers(season),
        week_url=_week_url,
    )
    (output_dir / "season.html").write_text(season_html)

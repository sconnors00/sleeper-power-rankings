"""Renders site/ from a SeasonSummary using Jinja2, plus site/data.js for Chart.js."""

from __future__ import annotations

import hashlib
import json
import shutil
from pathlib import Path

from jinja2 import Environment, FileSystemLoader, select_autoescape

from ffpr.compute import (
    DEFAULT_FORM_WINDOW,
    DEFAULT_WEIGHTS,
    FAIR_TRADE_GAP,
    FLEX_ELIGIBILITY,
    INJURY_OUTLOOK,
    LINEUP_FIT_GAP,
    PLAYOFF_SIMS,
    POSITIONS,
    PRIOR_GAMES,
    TRADE_VERDICT_BANDS,
    rivalry_highlights,
    trade_verdict,
)
from ffpr.models import Manager, SeasonSummary

PACKAGE_ROOT = Path(__file__).resolve().parent.parent
TEMPLATES_DIR = PACKAGE_ROOT / "templates"
STATIC_DIR = PACKAGE_ROOT / "static"

CHART_JS_URL = "https://cdn.jsdelivr.net/npm/chart.js@4.5.1/dist/chart.umd.min.js"
CHART_JS_SRI = "sha384-jb8JQMbMoBUzgWatfe6COACi2ljcDdZQ2OxczGA3bGNeWe+6DChMTBJemed7ZnvJ"


def _fmt2(value: float) -> str:
    return f"{value:.2f}"


def _pct(share: float) -> str:
    """A simulated probability. The extremes are hedged: 0 of 10,000 runs is
    unlikely, not impossible, since clinching isn't worked out exactly."""
    if share <= 0:
        return "<0.1%"
    if share >= 1:
        return ">99.9%"
    return f"{share:.1%}"


def _build_jinja_env() -> Environment:
    env = Environment(
        loader=FileSystemLoader(str(TEMPLATES_DIR)),
        autoescape=select_autoescape(["html"]),
        trim_blocks=True,
        lstrip_blocks=True,
    )
    env.filters["fmt2"] = _fmt2
    env.filters["pct"] = _pct
    return env


def _asset_version(data_js: str) -> str:
    """Short digest of every file a page loads, used as a ?v= cache buster.

    data.js changes every build but lives at a fixed URL, so without this a
    browser will happily pair freshly fetched HTML asking for week N with a
    cached payload that predates it -- the charts then read an empty
    weekMatchups[N] and render blank.
    """
    digest = hashlib.sha256(data_js.encode())
    for name in ("app.js", "style.css"):
        digest.update((STATIC_DIR / name).read_bytes())
    return digest.hexdigest()[:12]


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
            "colorDark": t.color_dark or t.color,
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
    # Every team plays every regular-season week (twice, counting a median game).
    games = len(season.weeks) * (2 if season.league_average_match else 1)
    luck = []
    pf_vs_pa = []
    if last_week is not None:
        for row in sorted(last_week.power_rankings, key=lambda r: r.roster_id):
            luck.append(
                {
                    "rosterId": row.roster_id,
                    "allplayPct": row.allplay_pct,
                    "winPct": row.win_pct,
                    "record": row.record,
                    "allplayRecord": row.allplay_record,
                    "games": games,
                }
            )
    for row in season.season_board.pf_leaderboard:
        pf_vs_pa.append({"rosterId": row.roster_id, "pf": row.pf, "pa": row.pa})

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
    }
    trade = _trade_payload(season)
    if trade is not None:
        payload["trade"] = trade
    return "window.FFPR = " + json.dumps(payload) + ";\n"


def _trade_payload(season: SeasonSummary) -> dict | None:
    """What the trade calculator needs: every rostered player's value and
    projection, who holds whom, and the rules it has to mirror (lineup slots,
    flex eligibility, verdict bands) so the numbers can't drift from Python's.
    The weeks still to play come with each week's weight and each player's
    expected share of it, so the lineup check can play the rest of the season
    out week by week; playoff weeks count by each team's own playoff odds."""
    tv = season.trade_values
    if tv is None:
        return None
    players = {}
    for pids in tv.rosters.values():
        for pid in pids:
            pv = tv.players[pid]
            players[pid] = {
                "name": pv.player.name,
                "pos": pv.player.position,
                "nfl": pv.player.nfl_team,
                "proj": pv.projection,
                "value": pv.value,
                "injury": pv.injury,
                "active": pv.active,
                "avail": pv.availability,
            }
    odds = {row.roster_id: row.playoff_pct for row in season.playoff_odds}
    return {
        "rosterPositions": season.roster_positions,
        "flex": {slot: sorted(eligible) for slot, eligible in FLEX_ELIGIBILITY.items()},
        "rosterLimit": tv.roster_limit,
        "weeksLeft": tv.weeks_left,
        "fairGap": FAIR_TRADE_GAP,
        "fitGap": LINEUP_FIT_GAP,
        "bands": list(TRADE_VERDICT_BANDS),
        "horizon": tv.horizon,
        "weights": tv.weights,
        "playoffFrom": tv.playoff_from,
        "playoffPct": {str(rid): pct for rid, pct in odds.items()},
        "replacement": tv.replacement,
        "players": players,
        "rosters": {str(rid): pids for rid, pids in tv.rosters.items()},
    }


def _matchup_pair(wk, matchup_id: int, teams: dict) -> list[dict]:
    pair = [m for m in wk.matchups if m.matchup_id == matchup_id]
    return [{"team": teams[m.roster_id], "points": m.team_points} for m in pair]


def _season_is_complete(season: SeasonSummary) -> bool:
    return bool(season.weeks) and season.through_week >= season.playoff_week_start - 1


def _all_week_numbers(season: SeasonSummary) -> list[int]:
    weeks = [w.week for w in season.weeks]
    if season.provisional_week_summary is not None:
        weeks = weeks + [season.provisional_week_summary.week]
    if season.preseason:
        weeks = [0, *weeks]
    if _season_is_complete(season):
        weeks = [*weeks, season.playoff_week_start]
    return weeks


def _rank_extremes(rows) -> dict[str, tuple[int, int]]:
    """Each position's best and worst rank across these rows (with per-position
    `ranks`), so templates can mark the league's best and worst at it."""
    extremes = {}
    for pos in POSITIONS:
        ranks = [row.ranks[pos] for row in rows]
        extremes[pos] = (min(ranks), max(ranks)) if ranks else (1, 1)
    return extremes


def _week_context(season: SeasonSummary, wk, is_index: bool) -> dict:
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

    # A lineup can only be judged once its games are played.
    lineup_rows = []
    costliest_lineup = None
    if wk.lineups and wk.week != season.provisional_week:
        lineup_rows = sorted(
            ({"team": teams[row.roster_id], "row": row} for row in wk.lineups),
            key=lambda e: (-e["row"].left_on_bench, e["team"].name),
        )
        if lineup_rows[0]["row"].left_on_bench > 0.005:
            costliest_lineup = lineup_rows[0]

    # Before kickoff every total is 0.0 and every team "ties for 1st" -- noise.
    position_rank_rows = []
    position_rank_extremes = {}
    if any(pts > 0 for row in wk.position_ranks for pts in row.points.values()):
        position_rank_rows = sorted(
            ({"team": teams[row.roster_id], "row": row} for row in wk.position_ranks),
            key=lambda e: -(matchups_by_roster[e["row"].roster_id].team_points),
        )
        position_rank_extremes = _rank_extremes(wk.position_ranks)

    return {
        "week": wk,
        "ranking_rows": ranking_rows,
        "results": results,
        "closest_matchups": closest,
        "blowout_matchups": blowouts,
        "roster_moves_rows": roster_moves_rows,
        "lineup_rows": lineup_rows,
        "costliest_lineup": costliest_lineup,
        "positions": POSITIONS,
        "position_rank_rows": position_rank_rows,
        "position_rank_extremes": position_rank_extremes,
        "highest_score_teams": [teams[rid] for rid in awards.highest_score_roster_ids],
        "lowest_score_teams": [teams[rid] for rid in awards.lowest_score_roster_ids],
        "best_bench": [(teams[rid], p) for rid, p in awards.best_bench],
        "best_starter": [(teams[rid], p) for rid, p in awards.best_starter],
        "worst_starter": [(teams[rid], p) for rid, p in awards.worst_starter],
        "is_index": is_index,
        "playoff": wk.week >= season.playoff_week_start,
        "provisional": wk.week == season.provisional_week,
        "rankings_frozen": rankings_frozen,
    }


RANKING_COMPONENTS = {
    "win_pct": "Win %",
    "allplay_pct": "All-play %",
    "pf_norm": "Points for, scaled",
    "form_norm": "Recent form, scaled",
}


def _spot_json(spot) -> dict:
    return {
        "slot": spot.slot,
        "player_id": spot.player_id,
        "name": spot.name,
        "position": spot.position,
        "nfl_team": spot.nfl_team,
        "injury": spot.injury,
    }


def build_rosters_json(season: SeasonSummary) -> str:
    """rosters.json: the same rosters and FAAB the page shows, for scripts.

    Stable keys and ordering, no timestamps, so a rebuild with unchanged
    league data produces an identical file.
    """
    report = season.roster_report
    teams = []
    for tr in report.teams:
        team = season.teams[tr.roster_id]
        teams.append(
            {
                "roster_id": tr.roster_id,
                "team_name": team.name,
                "owner": team.owner_name,
                "owner_id": team.owner_id,
                "record": tr.record,
                "faab": {
                    "budget": report.faab_budget,
                    "used": tr.faab_used,
                    "remaining": tr.faab_remaining,
                },
                "starters": [_spot_json(p) for p in tr.starters],
                "bench": [_spot_json(p) for p in tr.bench],
                "ir": [_spot_json(p) for p in tr.ir],
                "taxi": [_spot_json(p) for p in tr.taxi],
                "faab_claims": [
                    {
                        "week": c.week,
                        "player_id": c.player.player_id,
                        "player": c.player.name,
                        "position": c.player.position,
                        "bid": c.bid,
                        "dropped": c.dropped,
                    }
                    for c in tr.claims
                ],
            }
        )
    payload = {
        "season": season.season,
        "league": season.league_name,
        "through_week": season.through_week,
        "roster_positions": season.roster_positions,
        "faab_budget": report.faab_budget,
        "teams": teams,
    }
    return json.dumps(payload, indent=2) + "\n"


def _has_trades_page(season: SeasonSummary) -> bool:
    return season.trade_values is not None or bool(
        season.acquisitions and season.acquisitions.trades
    )


def _trades_context(season: SeasonSummary) -> dict:
    """The trades page: this season's trades judged on points so far and --
    while the season is live -- on value going forward, plus the calculator's
    team list, each team's projected strength, and the player value board."""
    tv = season.trade_values
    teams = season.teams

    history = []
    for trade in season.acquisitions.trades if season.acquisitions else []:
        if not trade.sides:
            continue
        rows = []
        for side in trade.sides:
            value = None
            if tv is not None:
                value = round(
                    sum(
                        tv.players[a.player.player_id].value
                        for a in side.received
                        if a.player.player_id in tv.players
                    ),
                    2,
                )
            rows.append({"side": side, "team": teams[side.roster_id], "value": value})
        leader = trade.sides[0]
        forward = None
        if tv is not None:
            verdict, ahead = trade_verdict([r["value"] for r in rows])
            forward = {
                "verdict": verdict,
                "team": rows[ahead]["team"] if ahead is not None else None,
            }
        history.append(
            {
                "week": trade.week,
                "rows": rows,
                "leader": teams[leader.roster_id],
                "lead": leader.points - (trade.sides[1].points if len(trade.sides) > 1 else 0.0),
                "forward": forward,
            }
        )

    ctx: dict = {"trade_history": history, "values": tv}
    if tv is None:
        return ctx

    rostered = [pv for pv in tv.players.values() if pv.roster_id is not None]
    board = []
    for pos in [*POSITIONS, *sorted(set(tv.replacement) - set(POSITIONS))]:
        rows = sorted(
            (pv for pv in rostered if pv.player.position == pos),
            key=lambda pv: (-pv.value, -pv.projection, pv.player.name),
        )
        if pos in tv.replacement and rows:
            board.append({"position": pos, "replacement": tv.replacement[pos], "rows": rows})

    ctx.update(
        {
            "calc_teams": sorted(
                (teams[rid] for rid in tv.rosters if rid in teams), key=lambda t: t.name.lower()
            ),
            "value_board": board,
            "strength_rows": [{"team": teams[row.roster_id], "row": row} for row in tv.strength],
            "strength_extremes": _rank_extremes(tv.strength),
            "positions": POSITIONS,
            "trade_deadline": tv.trade_deadline,
            "deadline_passed": (
                tv.trade_deadline is not None and season.through_week >= tv.trade_deadline
            ),
        }
    )
    return ctx


def _trade_example(season: SeasonSummary) -> dict | None:
    """The most valuable rostered player with games played, for the worked example."""
    tv = season.trade_values
    if tv is None:
        return None
    candidates = [
        pv for pv in tv.players.values() if pv.roster_id is not None and pv.games and pv.value > 0
    ]
    if not candidates:
        return None
    pv = max(candidates, key=lambda pv: (pv.value, pv.player.name))
    played = sum(w * a for w, a in zip(tv.weights, pv.availability, strict=True))
    return {
        "pv": pv,
        "replacement": tv.replacement[pv.player.position],
        # the weighted share of the weeks left he plays, which scales his surplus
        "share": played / sum(tv.weights) if sum(tv.weights) else 1.0,
    }


def _how_it_works_context(season: SeasonSummary) -> dict:
    """The live numbers the explainer quotes, so it can't drift from the code."""
    weights = season.weights or DEFAULT_WEIGHTS
    example = None
    if season.weeks:
        wk = season.weeks[-1]
        top = wk.power_rankings[0]
        example = {
            "week": wk.week,
            "team": season.teams[top.roster_id],
            "score": top.score,
            "terms": [
                {
                    "label": RANKING_COMPONENTS[key],
                    "weight": weights[key],
                    "value": getattr(top, key),
                }
                for key in RANKING_COMPONENTS
            ],
        }
    return {
        "weights": [
            {"key": key, "label": label, "weight": weights[key]}
            for key, label in RANKING_COMPONENTS.items()
        ],
        "form_window": season.form_window or DEFAULT_FORM_WINDOW,
        "league_average_match": season.league_average_match,
        "num_teams": len(season.teams),
        "example": example,
        "positions": POSITIONS,
        "playoff_sims": PLAYOFF_SIMS,
        "playoff_teams": season.playoff_teams,
        "playoff_byes": season.playoff_byes,
        "trade_values": season.trade_values,
        "trade_example": _trade_example(season),
        "prior_games": (
            season.trade_values.prior_games if season.trade_values is not None else PRIOR_GAMES
        ),
        "fair_gap": FAIR_TRADE_GAP,
        "verdict_bands": TRADE_VERDICT_BANDS,
        "fit_gap": LINEUP_FIT_GAP,
        "outlook": INJURY_OUTLOOK,
        "ir_weeks": len(INJURY_OUTLOOK["IR"]),
        "superflex": "SUPER_FLEX" in season.roster_positions,
        "replacement_levels": _replacement_in_order(season),
    }


def _replacement_in_order(season: SeasonSummary) -> list[tuple[str, float]]:
    """Replacement levels in the site's usual position order."""
    levels = season.trade_values.replacement if season.trade_values is not None else {}
    order = [*POSITIONS, *sorted(set(levels) - set(POSITIONS))]
    return [(pos, levels[pos]) for pos in order if pos in levels]


def render_site(
    season: SeasonSummary,
    output_dir: Path,
    all_seasons: list[str] | None = None,
    site_root_prefix: str = "",
    rivalries: list[Manager] | None = None,
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

    data_js = build_data_js(season)
    (output_dir / "data.js").write_text(data_js)

    preseason_rows = [{"row": row, "team": season.teams[row.roster_id]} for row in season.preseason]
    week_summaries = [w for w in (*season.weeks, season.provisional_week_summary) if w is not None]

    # Every page this season gets, as (path, template, page context). The nav
    # shows a page only if it's in this list, so adding a page is one entry.
    pages: list[tuple[str, str, dict]] = []
    if season.weeks:
        latest = _week_context(season, season.weeks[-1], is_index=True)
        pages.append(("index.html", "week.html", latest))
        board = {"season": season, "board": season.season_board}
        pages.append(("season.html", "season.html", board))
    else:
        pages.append(("index.html", "landing.html", {"preseason_rows": preseason_rows}))
    if season.preseason:
        pages.append((_week_url(0), "week0.html", {"preseason_rows": preseason_rows}))
    for wk in week_summaries:
        pages.append((_week_url(wk.week), "week.html", _week_context(season, wk, is_index=False)))
    if _season_is_complete(season):
        recap = {"records": season.season_board.records}
        pages.append((_week_url(season.playoff_week_start), "season_recap.html", recap))
    if season.roster_report is not None:
        pages.append(("rosters.html", "rosters.html", {"report": season.roster_report}))
    if _has_trades_page(season):
        pages.append(("trades.html", "trades.html", _trades_context(season)))
    if season.draft is not None:
        pages.append(("draft.html", "draft.html", {"draft": season.draft}))
    if rivalries:
        rivals = {
            "managers": rivalries,
            "by_id": {m.owner_id: m for m in rivalries},
            "highlights": rivalry_highlights(rivalries),
        }
        pages.append(("rivalries.html", "rivalries.html", rivals))
    pages.append(("how-it-works.html", "how_it_works.html", _how_it_works_context(season)))

    env = _build_jinja_env()
    common = {
        "asset_version": _asset_version(data_js),
        "chart_js_url": CHART_JS_URL,
        "chart_js_sri": CHART_JS_SRI,
        "league_name": season.league_name,
        "season_year": season.season,
        "teams": season.teams,
        "built": {path for path, _, _ in pages},
        "all_seasons": all_seasons or [season.season],
        "site_root_prefix": site_root_prefix,
        "year_url": _year_url,
        "recap_week": season.playoff_week_start,
        "all_weeks": _all_week_numbers(season),
        "week_url": _week_url,
    }
    for path, template, ctx in pages:
        html = env.get_template(template).render(
            **common, asset_prefix="../" * path.count("/"), **ctx
        )
        (output_dir / path).write_text(html)
    if season.roster_report is not None:
        (output_dir / "rosters.json").write_text(build_rosters_json(season))

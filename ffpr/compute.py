"""Pure functions: raw Sleeper JSON in, models.py dataclasses out.

No I/O here — everything is a plain function of its arguments so it can be
unit tested against committed fixtures.
"""

from __future__ import annotations

from ffpr.models import (
    DraftPickGrade,
    DraftSummary,
    Matchup,
    PFLeaderboardRow,
    PlayerMove,
    PlayerScore,
    PowerRankRow,
    PreseasonRow,
    SeasonBoard,
    SeasonBoardEntry,
    SeasonRecords,
    Team,
    TeamDraftGrade,
    TeamRosterMoves,
    WeekAwards,
    WeekSummary,
)

# 12 fixed, stable-order team colors. The first 8 are the validated
# categorical palette (dataviz skill, references/palette.md); slots 9-12
# extend it for a 12-team league. At this series count no ordering clears
# the all-pairs CVD floor (the palette tops out at 3 slots for that), so
# every chart that uses these colors also direct-labels or offers a legend
# tap-to-isolate interaction and a plain-text table alternative -- identity
# never rests on color alone.
TEAM_COLORS_LIGHT = [
    "#2a78d6",  # 1 blue
    "#eb6834",  # 2 orange
    "#1baf7a",  # 3 aqua
    "#eda100",  # 4 yellow
    "#e87ba4",  # 5 magenta
    "#008300",  # 6 green
    "#4a3aa7",  # 7 violet
    "#e34948",  # 8 red
    "#8a5a2b",  # 9 brown
    "#5ac8c8",  # 10 teal
    "#c77dff",  # 11 lavender
    "#6b7280",  # 12 gray
]
TEAM_COLORS_DARK = [
    "#3987e5",
    "#d95926",
    "#199e70",
    "#c98500",
    "#d55181",
    "#008300",
    "#9085e9",
    "#e66767",
    "#a87642",
    "#4fb3b3",
    "#b565f0",
    "#8b93a1",
]


def resolve_team_name(user: dict | None, roster_id: int) -> str:
    if user is None:
        return f"Team {roster_id}"
    team_name = (user.get("metadata") or {}).get("team_name")
    if team_name:
        return team_name
    return user.get("display_name") or f"Team {roster_id}"


def resolve_avatar_url(user: dict | None) -> str | None:
    if user is None:
        return None
    meta_avatar = (user.get("metadata") or {}).get("avatar")
    if meta_avatar and str(meta_avatar).startswith("http"):
        return meta_avatar
    avatar_hash = user.get("avatar")
    if avatar_hash:
        return f"https://sleepercdn.com/avatars/thumbs/{avatar_hash}"
    return None


def build_teams(rosters: list[dict], users: list[dict]) -> dict[int, Team]:
    users_by_id = {u["user_id"]: u for u in users}
    teams: dict[int, Team] = {}
    for i, roster in enumerate(sorted(rosters, key=lambda r: r["roster_id"])):
        roster_id = roster["roster_id"]
        owner_id = roster.get("owner_id")
        user = users_by_id.get(owner_id) if owner_id else None
        teams[roster_id] = Team(
            roster_id=roster_id,
            owner_id=owner_id,
            name=resolve_team_name(user, roster_id),
            avatar_url=resolve_avatar_url(user),
            color=TEAM_COLORS_LIGHT[i % len(TEAM_COLORS_LIGHT)],
        )
    return teams


def _player_score(player_id: str, points: float, players_map: dict) -> PlayerScore:
    info = players_map.get(player_id) or {}
    name = (
        info.get("full_name") or f"{info.get('first_name', '')} {info.get('last_name', '')}".strip()
    )
    if not name:
        name = player_id
    return PlayerScore(
        player_id=player_id,
        name=name,
        position=info.get("position") or info.get("fantasy_positions", [None])[0] or "?",
        nfl_team=info.get("team"),
        points=points,
    )


def parse_week_matchups(
    raw_matchups: list[dict],
    week: int,
    rosters_by_id: dict[int, dict],
    players_map: dict,
) -> list[Matchup]:
    by_matchup_id: dict[int, list[dict]] = {}
    for m in raw_matchups:
        by_matchup_id.setdefault(m["matchup_id"], []).append(m)

    matchups: list[Matchup] = []
    for _mid, pair in by_matchup_id.items():
        roster_ids = [m["roster_id"] for m in pair]
        for m in pair:
            opponent_roster_id = next((r for r in roster_ids if r != m["roster_id"]), None)
            roster_id = m["roster_id"]
            roster = rosters_by_id.get(roster_id, {})
            reserve = set(roster.get("reserve") or [])
            taxi = set(roster.get("taxi") or [])
            starter_ids = [s for s in (m.get("starters") or [])]
            starters_points = m.get("starters_points") or []
            players_points = m.get("players_points") or {}

            starters = [
                _player_score(
                    pid, starters_points[i] if i < len(starters_points) else 0.0, players_map
                )
                for i, pid in enumerate(starter_ids)
                if pid != "0"
            ]
            starter_id_set = set(pid for pid in starter_ids if pid != "0")
            bench_ids = [
                pid
                for pid in (m.get("players") or [])
                if pid not in starter_id_set and pid not in reserve and pid not in taxi
            ]
            bench = [
                _player_score(pid, players_points.get(pid, 0.0), players_map) for pid in bench_ids
            ]

            team_points = m.get("custom_points")
            if team_points is None:
                team_points = m.get("points") or 0.0

            matchups.append(
                Matchup(
                    week=week,
                    matchup_id=m["matchup_id"],
                    roster_id=roster_id,
                    opponent_roster_id=opponent_roster_id,
                    team_points=team_points,
                    starters=starters,
                    bench=bench,
                )
            )
    return matchups


def _tied_max(items: list[tuple[int, float]]) -> tuple[list[int], float]:
    """items: list of (roster_id, value). Returns (roster_ids at max, max value)."""
    if not items:
        return [], 0.0
    best = max(v for _, v in items)
    return [rid for rid, v in items if v == best], best


def _tied_min(items: list[tuple[int, float]]) -> tuple[list[int], float]:
    if not items:
        return [], 0.0
    worst = min(v for _, v in items)
    return [rid for rid, v in items if v == worst], worst


def compute_week_awards(matchups: list[Matchup]) -> WeekAwards:
    team_scores = [(m.roster_id, m.team_points) for m in matchups]
    highest_ids, highest_score = _tied_max(team_scores)
    lowest_ids, lowest_score = _tied_min(team_scores)

    bench_points_by_roster: dict[int, float] = {}
    all_bench: list[tuple[int, PlayerScore]] = []
    for m in matchups:
        bench_points_by_roster[m.roster_id] = sum(p.points for p in m.bench)
        all_bench.extend((m.roster_id, p) for p in m.bench)
    if all_bench:
        best_bench_points = max(p.points for _, p in all_bench)
        best_bench = [(rid, p) for rid, p in all_bench if p.points == best_bench_points]
    else:
        best_bench_points = 0.0
        best_bench = []

    all_starters: list[tuple[int, PlayerScore]] = []
    for m in matchups:
        all_starters.extend((m.roster_id, p) for p in m.starters)
    if all_starters:
        best_starter_points = max(p.points for _, p in all_starters)
        best_starter = [(rid, p) for rid, p in all_starters if p.points == best_starter_points]
        worst_starter_points = min(p.points for _, p in all_starters)
        worst_starter = [(rid, p) for rid, p in all_starters if p.points == worst_starter_points]
    else:
        best_starter_points = 0.0
        best_starter = []
        worst_starter_points = 0.0
        worst_starter = []
    worst_starter_zero = worst_starter_points == 0.0

    by_matchup_id: dict[int, list[Matchup]] = {}
    for m in matchups:
        by_matchup_id.setdefault(m.matchup_id, []).append(m)
    margins: list[tuple[int, float]] = []
    for mid, pair in by_matchup_id.items():
        if len(pair) == 2:
            margin = abs(pair[0].team_points - pair[1].team_points)
            margins.append((mid, margin))
    closest_ids, closest_margin = _tied_min(margins) if margins else ([], 0.0)
    blowout_ids, blowout_margin = _tied_max(margins) if margins else ([], 0.0)

    return WeekAwards(
        highest_score_roster_ids=highest_ids,
        highest_score=highest_score,
        lowest_score_roster_ids=lowest_ids,
        lowest_score=lowest_score,
        best_bench=best_bench,
        best_bench_points=best_bench_points,
        bench_points_by_roster=bench_points_by_roster,
        best_starter=best_starter,
        best_starter_points=best_starter_points,
        worst_starter=worst_starter,
        worst_starter_points=worst_starter_points,
        worst_starter_zero=worst_starter_zero,
        closest_matchup_ids=closest_ids,
        closest_margin=closest_margin,
        biggest_blowout_matchup_ids=blowout_ids,
        biggest_blowout_margin=blowout_margin,
    )


def compute_allplay_week(matchups: list[Matchup]) -> dict[int, int]:
    """Roster id -> number of the OTHER teams it outscored this week."""
    scores = [(m.roster_id, m.team_points) for m in matchups]
    result: dict[int, int] = {}
    for rid, pts in scores:
        result[rid] = sum(
            1 for other_rid, other_pts in scores if other_rid != rid and pts > other_pts
        )
    return result


def compute_allplay_week_equiv(matchups: list[Matchup]) -> dict[int, float]:
    """Roster id -> all-play win-equivalents this week, with ties worth 0.5.

    compute_allplay_week is the displayed "teams outscored" count; this is the
    value accumulated into the all-play record so a scoring tie doesn't count
    as a loss for both teams.
    """
    scores = [(m.roster_id, m.team_points) for m in matchups]
    result: dict[int, float] = {}
    for rid, pts in scores:
        equiv = 0.0
        for other_rid, other_pts in scores:
            if other_rid == rid:
                continue
            if pts > other_pts:
                equiv += 1.0
            elif pts == other_pts:
                equiv += 0.5
        result[rid] = equiv
    return result


def _median(values: list[float]) -> float:
    s = sorted(values)
    n = len(s)
    mid = n // 2
    if n % 2:
        return s[mid]
    return (s[mid - 1] + s[mid]) / 2


def compute_weekly_head_to_head(
    matchups: list[Matchup], league_average_match: bool
) -> dict[int, tuple[float, int]]:
    """Roster id -> (win-equivalent this week, games played this week).

    Head-to-head result from the matchup pairing, plus (if league_average_match
    is on) a bonus game against the week's median score -- mirrors how Sleeper
    computes the official record so our derived per-week win_pct lines up.
    """
    by_matchup_id: dict[int, list[Matchup]] = {}
    for m in matchups:
        by_matchup_id.setdefault(m.matchup_id, []).append(m)

    result: dict[int, tuple[float, int]] = {}
    for pair in by_matchup_id.values():
        if len(pair) != 2:
            continue
        a, b = pair
        if a.team_points > b.team_points:
            result[a.roster_id] = (1.0, 1)
            result[b.roster_id] = (0.0, 1)
        elif a.team_points < b.team_points:
            result[a.roster_id] = (0.0, 1)
            result[b.roster_id] = (1.0, 1)
        else:
            result[a.roster_id] = (0.5, 1)
            result[b.roster_id] = (0.5, 1)

    if league_average_match:
        median = _median([m.team_points for m in matchups])
        for m in matchups:
            win_equiv, games = result.get(m.roster_id, (0.0, 0))
            if m.team_points > median:
                win_equiv += 1.0
            elif m.team_points < median:
                win_equiv += 0.0
            else:
                win_equiv += 0.5
            result[m.roster_id] = (win_equiv, games + 1)

    return result


def compute_power_rankings(
    roster_ids: list[int],
    cumulative_win: dict[int, float],
    cumulative_games: dict[int, int],
    cumulative_allplay_win: dict[int, float],
    cumulative_allplay_games: dict[int, int],
    cumulative_pf: dict[int, float],
    last_n_scores: dict[int, list[float]],
    weights: dict[str, float],
    prev_ranks: dict[int, int] | None,
) -> list[PowerRankRow]:
    win_pct = {
        rid: (cumulative_win[rid] / cumulative_games[rid] if cumulative_games[rid] else 0.0)
        for rid in roster_ids
    }
    allplay_pct = {
        rid: (
            cumulative_allplay_win[rid] / cumulative_allplay_games[rid]
            if cumulative_allplay_games[rid]
            else 0.0
        )
        for rid in roster_ids
    }

    pf_values = [cumulative_pf[rid] for rid in roster_ids]
    pf_min, pf_max = min(pf_values), max(pf_values)
    pf_norm = {
        rid: ((cumulative_pf[rid] - pf_min) / (pf_max - pf_min) if pf_max > pf_min else 0.5)
        for rid in roster_ids
    }

    form_avg = {
        rid: (sum(last_n_scores[rid]) / len(last_n_scores[rid]) if last_n_scores[rid] else 0.0)
        for rid in roster_ids
    }
    form_values = list(form_avg.values())
    form_min, form_max = min(form_values), max(form_values)
    form_norm = {
        rid: ((form_avg[rid] - form_min) / (form_max - form_min) if form_max > form_min else 0.5)
        for rid in roster_ids
    }

    scores = {
        rid: (
            weights["win_pct"] * win_pct[rid]
            + weights["allplay_pct"] * allplay_pct[rid]
            + weights["pf_norm"] * pf_norm[rid]
            + weights["form_norm"] * form_norm[rid]
        )
        for rid in roster_ids
    }

    ranked = sorted(roster_ids, key=lambda rid: (-scores[rid], -cumulative_pf[rid]))
    rows: list[PowerRankRow] = []
    for i, rid in enumerate(ranked):
        rank = i + 1
        movement = None
        if prev_ranks is not None and rid in prev_ranks:
            movement = prev_ranks[rid] - rank
        wins = cumulative_win[rid]
        losses_ties = cumulative_games[rid] - wins
        record = (
            f"{wins:.1f}-{losses_ties:.1f}"
            if wins % 1 or losses_ties % 1
            else f"{int(wins)}-{int(losses_ties)}"
        )
        ap_wins = cumulative_allplay_win[rid]
        ap_total = cumulative_allplay_games[rid]
        allplay_record = f"{ap_wins:.1f}-{ap_total - ap_wins:.1f}"
        rows.append(
            PowerRankRow(
                roster_id=rid,
                rank=rank,
                score=scores[rid],
                win_pct=win_pct[rid],
                allplay_pct=allplay_pct[rid],
                pf_norm=pf_norm[rid],
                form_norm=form_norm[rid],
                luck=win_pct[rid] - allplay_pct[rid],
                record=record,
                allplay_record=allplay_record,
                movement=movement,
            )
        )
    return rows


def compute_roster_moves(raw_transactions: list[dict], players_map: dict) -> list[TeamRosterMoves]:
    """Adds/drops for the week, grouped by roster -- the roster differences
    a team page shows. Failed transactions (an outbid waiver claim, a vetoed
    trade) never happened, so they're excluded.
    """
    by_roster: dict[int, TeamRosterMoves] = {}

    def bucket(rid: int) -> TeamRosterMoves:
        if rid not in by_roster:
            by_roster[rid] = TeamRosterMoves(roster_id=rid, added=[], dropped=[])
        return by_roster[rid]

    for tx in raw_transactions:
        if tx.get("status") != "complete":
            continue
        tx_type = tx.get("type") or "waiver"
        faab = (tx.get("settings") or {}).get("waiver_bid") if tx_type == "waiver" else None
        for pid, rid in (tx.get("adds") or {}).items():
            bucket(rid).added.append(
                PlayerMove(
                    player=_player_score(pid, 0.0, players_map), move_type=tx_type, faab=faab
                )
            )
        for pid, rid in (tx.get("drops") or {}).items():
            bucket(rid).dropped.append(
                PlayerMove(
                    player=_player_score(pid, 0.0, players_map), move_type=tx_type, faab=None
                )
            )

    return [by_roster[rid] for rid in sorted(by_roster)]


def build_week_summaries(
    roster_ids: list[int],
    weeks_raw_matchups: dict[int, list[dict]],
    weeks_raw_transactions: dict[int, list[dict]],
    rosters_by_id: dict[int, dict],
    players_map: dict,
    league_average_match: bool,
    weights: dict[str, float],
    form_window: int,
) -> list[WeekSummary]:
    weeks = sorted(weeks_raw_matchups.keys())
    cumulative_win = dict.fromkeys(roster_ids, 0.0)
    cumulative_games = dict.fromkeys(roster_ids, 0)
    cumulative_allplay_win = dict.fromkeys(roster_ids, 0.0)
    cumulative_allplay_games = dict.fromkeys(roster_ids, 0)
    cumulative_pf = dict.fromkeys(roster_ids, 0.0)
    score_history: dict[int, list[float]] = {rid: [] for rid in roster_ids}

    prev_ranks: dict[int, int] | None = None
    summaries: list[WeekSummary] = []

    for week in weeks:
        matchups = parse_week_matchups(weeks_raw_matchups[week], week, rosters_by_id, players_map)
        awards = compute_week_awards(matchups)
        allplay_week = compute_allplay_week(matchups)
        allplay_equiv = compute_allplay_week_equiv(matchups)
        h2h = compute_weekly_head_to_head(matchups, league_average_match)

        for m in matchups:
            rid = m.roster_id
            cumulative_pf[rid] += m.team_points
            score_history[rid].append(m.team_points)

            win_equiv, games = h2h.get(rid, (0.0, 0))
            cumulative_win[rid] += win_equiv
            cumulative_games[rid] += games

            cumulative_allplay_win[rid] += allplay_equiv.get(rid, 0.0)
            cumulative_allplay_games[rid] += len(matchups) - 1

        last_n_scores = {rid: score_history[rid][-form_window:] for rid in roster_ids}
        power_rankings = compute_power_rankings(
            roster_ids,
            cumulative_win,
            cumulative_games,
            cumulative_allplay_win,
            cumulative_allplay_games,
            cumulative_pf,
            last_n_scores,
            weights,
            prev_ranks,
        )
        prev_ranks = {row.roster_id: row.rank for row in power_rankings}
        roster_moves = compute_roster_moves(weeks_raw_transactions.get(week, []), players_map)

        summaries.append(
            WeekSummary(
                week=week,
                matchups=matchups,
                awards=awards,
                allplay_week_wins=allplay_week,
                power_rankings=power_rankings,
                roster_moves=roster_moves,
            )
        )

    return summaries


# --- Player valuation from the league's own auction ---
#
# The auction produces a market price for every drafted player. Fitting
# expected price against Sleeper's global search_rank ("what does this league
# pay for a player ranked around there?") gives a value function that works
# for any player, drafted or not -- used by both the pre-season rankings and
# the draft grades.

PriceCurve = list[tuple[int, int]]  # (search_rank, amount) sorted by rank


def fit_price_curve(picks: list[dict], players_map: dict) -> PriceCurve:
    """(search_rank, amount) pairs from non-keeper picks, sorted by rank.

    Keeper prices are set by keeper rules, not open bidding, so they'd skew
    the market curve.
    """
    points: PriceCurve = []
    for p in picks:
        if p.get("is_keeper"):
            continue
        info = players_map.get(p["player_id"]) or {}
        rank = info.get("search_rank")
        amount = int((p.get("metadata") or {}).get("amount") or 0)
        if rank is not None:
            points.append((rank, amount))
    points.sort()
    return points


def expected_price(rank: int | None, curve: PriceCurve, max_window: int = 15) -> float:
    """Median auction price among the curve points nearest in rank.

    The window narrows near the top of the board (down to 5 points), where
    prices fall steeply and a wide median would drag every elite player's
    expected price toward mid-tier money; out on the flat tail it widens to
    `max_window`. Unknown rank (team defenses, deep stashes) is worth the
    $1 floor.
    """
    if rank is None or not curve:
        return 1.0
    window = max(5, min(max_window, rank // 3))
    nearest = sorted(curve, key=lambda point: abs(point[0] - rank))[:window]
    return max(1.0, _median([float(a) for _, a in nearest]))


def player_dollar_value(player_id: str, players_map: dict, curve: PriceCurve) -> float:
    info = players_map.get(player_id) or {}
    return expected_price(info.get("search_rank"), curve)


def build_preseason_rankings(
    prev_final_rankings: list[PowerRankRow],
    prev_pf_by_roster: dict[int, float],
    current_rosters: list[dict],
    prev_rosters: list[dict],
    champion_roster_id: int | None,
) -> list[PreseasonRow]:
    """Pre-season rankings: last season's final power-ranking order, carried
    over by roster_id (Sleeper keeps roster ids stable across league renewals,
    and in a keeper league the roster is the continuous entity even when the
    owner changes). Rosters with no previous-season history rank last, in
    roster_id order.
    """
    current_ids = {r["roster_id"] for r in current_rosters}
    prev_owner = {r["roster_id"]: r.get("owner_id") for r in prev_rosters}
    cur_owner = {r["roster_id"]: r.get("owner_id") for r in current_rosters}

    ordered: list[tuple[int, PowerRankRow | None]] = [
        (row.roster_id, row) for row in prev_final_rankings if row.roster_id in current_ids
    ]
    seen = {rid for rid, _ in ordered}
    ordered += [(rid, None) for rid in sorted(current_ids - seen)]

    rows: list[PreseasonRow] = []
    for i, (rid, prev_row) in enumerate(ordered):
        rows.append(
            PreseasonRow(
                roster_id=rid,
                rank=i + 1,
                prev_rank=prev_row.rank if prev_row else None,
                prev_record=prev_row.record if prev_row else None,
                prev_pf=prev_pf_by_roster.get(rid) if prev_row else None,
                new_owner=rid in prev_owner and prev_owner[rid] != cur_owner.get(rid),
                champion=rid == champion_roster_id,
            )
        )
    return rows


_GRADE_STEPS = [
    (1.5, "A+"),
    (1.0, "A"),
    (0.5, "A-"),
    (0.15, "B+"),
    (-0.15, "B"),
    (-0.5, "B-"),
    (-1.0, "C+"),
    (-1.5, "C"),
]


def _letter_grade(z: float) -> str:
    for threshold, letter in _GRADE_STEPS:
        if z >= threshold:
            return letter
    return "D"


def grade_draft(
    picks: list[dict],
    players_map: dict,
    budget: int,
    steal_count: int = 10,
    min_overpay_amount: int = 5,
) -> DraftSummary:
    """Auction draft grades: each pick's surplus is its market value (from the
    league's own price curve) minus what was paid. Keepers are listed but
    excluded from grading -- their prices come from keeper rules, not bidding.
    Team letter grades come from the z-score of total surplus.
    """
    curve = fit_price_curve(picks, players_map)

    graded: list[DraftPickGrade] = []
    for p in picks:
        pid = p["player_id"]
        amount = int((p.get("metadata") or {}).get("amount") or 0)
        expected = player_dollar_value(pid, players_map, curve)
        graded.append(
            DraftPickGrade(
                pick_no=p["pick_no"],
                round=p["round"],
                roster_id=p["roster_id"],
                player=_player_score(pid, 0.0, players_map),
                amount=amount,
                expected=expected,
                surplus=expected - amount,
                is_keeper=bool(p.get("is_keeper")),
            )
        )

    by_roster: dict[int, list[DraftPickGrade]] = {}
    for g in graded:
        by_roster.setdefault(g.roster_id, []).append(g)

    team_rows = []
    for rid, team_picks in by_roster.items():
        open_picks = [g for g in team_picks if not g.is_keeper]
        keepers = [g for g in team_picks if g.is_keeper]
        open_picks.sort(key=lambda g: -g.surplus)
        team_rows.append(
            TeamDraftGrade(
                roster_id=rid,
                grade="",
                spent=sum(g.amount for g in open_picks),
                value=sum(g.expected for g in open_picks),
                surplus=sum(g.surplus for g in open_picks),
                picks=open_picks,
                keepers=keepers,
            )
        )

    surpluses = [t.surplus for t in team_rows]
    mean = sum(surpluses) / len(surpluses) if surpluses else 0.0
    variance = sum((s - mean) ** 2 for s in surpluses) / len(surpluses) if surpluses else 0.0
    std = variance**0.5
    for t in team_rows:
        z = (t.surplus - mean) / std if std > 0 else 0.0
        t.grade = _letter_grade(z)
    team_rows.sort(key=lambda t: -t.surplus)

    open_graded = [g for g in graded if not g.is_keeper]
    steals = sorted((g for g in open_graded if g.surplus > 0), key=lambda g: -g.surplus)[
        :steal_count
    ]
    overpays = sorted(
        (g for g in open_graded if g.surplus < 0 and g.amount >= min_overpay_amount),
        key=lambda g: g.surplus,
    )[:steal_count]

    return DraftSummary(budget=budget, teams=team_rows, steals=steals, overpays=overpays)


def official_record_string(roster: dict) -> str:
    """The record as Sleeper reports it (rosters.settings), for display only.

    Distinct from PowerRankRow.record, which is a per-week head-to-head +
    median-game derivation used so every historical week has a comparable
    win_pct -- rosters.settings only ever holds the current, final totals.
    """
    settings = roster.get("settings") or {}
    wins = settings.get("wins", 0)
    losses = settings.get("losses", 0)
    ties = settings.get("ties", 0)
    if ties:
        return f"{wins}-{losses}-{ties}"
    return f"{wins}-{losses}"


def apply_official_records(
    power_rankings: list[PowerRankRow], rosters_by_id: dict[int, dict]
) -> None:
    """Overwrite .record in place with the official rosters.settings record."""
    for row in power_rankings:
        roster = rosters_by_id.get(row.roster_id)
        if roster is not None:
            row.record = official_record_string(roster)


def build_provisional_week_summary(
    week: int,
    raw_matchups: list[dict],
    raw_transactions: list[dict],
    rosters_by_id: dict[int, dict],
    players_map: dict,
) -> WeekSummary:
    """Awards/scores for the in-progress week, with no power rankings.

    Used for --provisional builds: the week is shown for visibility but
    deliberately excluded from the ranking computation.
    """
    matchups = parse_week_matchups(raw_matchups, week, rosters_by_id, players_map)
    awards = compute_week_awards(matchups)
    allplay_week = compute_allplay_week(matchups)
    roster_moves = compute_roster_moves(raw_transactions, players_map)
    return WeekSummary(
        week=week,
        matchups=matchups,
        awards=awards,
        allplay_week_wins=allplay_week,
        power_rankings=[],
        roster_moves=roster_moves,
    )


def build_season_board(weeks: list[WeekSummary], teams: dict[int, Team]) -> SeasonBoard:
    log: list[SeasonBoardEntry] = []
    crown_counts: dict[int, int] = dict.fromkeys(teams, 0)
    pf_totals: dict[int, float] = dict.fromkeys(teams, 0.0)
    pf_against_totals: dict[int, float] = dict.fromkeys(teams, 0.0)
    bench_totals: dict[int, float] = dict.fromkeys(teams, 0.0)
    scores_by_roster: dict[int, dict[int, float]] = {rid: {} for rid in teams}

    highest_team: tuple[list[int], int, float] = ([], 0, float("-inf"))
    lowest_team: tuple[list[int], int, float] = ([], 0, float("inf"))
    highest_starter: tuple[list[tuple[int, PlayerScore]], int] = ([], 0)
    highest_starter_pts = float("-inf")
    highest_bench: tuple[list[tuple[int, PlayerScore]], int] = ([], 0)
    highest_bench_pts = float("-inf")

    for wk in weeks:
        top_ids = wk.awards.highest_score_roster_ids
        top_score = wk.awards.highest_score
        log.append(
            SeasonBoardEntry(week=wk.week, top_scorer_roster_ids=top_ids, top_score=top_score)
        )
        for rid in top_ids:
            crown_counts[rid] += 1

        for rid, bench_pts in wk.awards.bench_points_by_roster.items():
            bench_totals[rid] += bench_pts

        for m in wk.matchups:
            pf_totals[m.roster_id] += m.team_points
            scores_by_roster[m.roster_id][wk.week] = m.team_points
            if m.opponent_roster_id is not None:
                opp = next((x for x in wk.matchups if x.roster_id == m.opponent_roster_id), None)
                if opp is not None:
                    pf_against_totals[m.roster_id] += opp.team_points

            if m.team_points > highest_team[2]:
                highest_team = ([m.roster_id], wk.week, m.team_points)
            elif m.team_points == highest_team[2]:
                highest_team = (highest_team[0] + [m.roster_id], wk.week, m.team_points)
            if m.team_points < lowest_team[2]:
                lowest_team = ([m.roster_id], wk.week, m.team_points)
            elif m.team_points == lowest_team[2]:
                lowest_team = (lowest_team[0] + [m.roster_id], wk.week, m.team_points)

            for p in m.starters:
                if p.points > highest_starter_pts:
                    highest_starter_pts = p.points
                    highest_starter = ([(m.roster_id, p)], wk.week)
                elif p.points == highest_starter_pts:
                    highest_starter = (highest_starter[0] + [(m.roster_id, p)], wk.week)
            for p in m.bench:
                if p.points > highest_bench_pts:
                    highest_bench_pts = p.points
                    highest_bench = ([(m.roster_id, p)], wk.week)
                elif p.points == highest_bench_pts:
                    highest_bench = (highest_bench[0] + [(m.roster_id, p)], wk.week)

    pf_leaderboard: list[PFLeaderboardRow] = []
    for rid in teams:
        weekly = scores_by_roster[rid]
        if not weekly:
            continue
        best_week, best_score = max(weekly.items(), key=lambda kv: kv[1])
        worst_week, worst_score = min(weekly.items(), key=lambda kv: kv[1])
        pf_leaderboard.append(
            PFLeaderboardRow(
                roster_id=rid,
                pf=pf_totals[rid],
                pa=pf_against_totals[rid],
                avg=pf_totals[rid] / len(weekly),
                best_week=best_week,
                best_week_score=best_score,
                worst_week=worst_week,
                worst_week_score=worst_score,
                bench_points=bench_totals[rid],
            )
        )
    pf_leaderboard.sort(key=lambda r: -r.pf)

    records = SeasonRecords(
        highest_team_score=highest_team,
        lowest_team_score=lowest_team if weeks else ([], 0, 0.0),
        highest_starter=highest_starter,
        highest_bench_player=highest_bench,
    )

    return SeasonBoard(
        log=log,
        crown_counts=crown_counts,
        pf_leaderboard=pf_leaderboard,
        records=records,
    )

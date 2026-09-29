"""Plain dataclasses shared between compute.py and build.py."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Team:
    roster_id: int
    owner_id: str | None
    name: str
    avatar_url: str | None
    color: str


@dataclass
class PlayerScore:
    player_id: str
    name: str
    position: str
    nfl_team: str | None
    points: float


@dataclass
class Matchup:
    week: int
    matchup_id: int
    roster_id: int
    opponent_roster_id: int | None
    team_points: float
    starters: list[PlayerScore]
    bench: list[PlayerScore]


@dataclass
class WeekAwards:
    highest_score_roster_ids: list[int]
    highest_score: float
    lowest_score_roster_ids: list[int]
    lowest_score: float
    best_bench: list[tuple[int, PlayerScore]]  # (roster_id, player)
    best_bench_points: float
    bench_points_by_roster: dict[int, float]
    best_starter: list[tuple[int, PlayerScore]]
    best_starter_points: float
    worst_starter: list[tuple[int, PlayerScore]]
    worst_starter_points: float
    worst_starter_zero: bool
    closest_matchup_ids: list[int]
    closest_margin: float
    biggest_blowout_matchup_ids: list[int]
    biggest_blowout_margin: float


@dataclass
class PowerRankRow:
    roster_id: int
    rank: int
    score: float
    win_pct: float
    allplay_pct: float
    pf_norm: float
    form_norm: float
    luck: float
    record: str
    allplay_record: str
    movement: int | None  # positive = moved up, negative = moved down, None = week 1


@dataclass
class PlayerMove:
    player: PlayerScore  # points field unused here; name/position/team
    move_type: str  # "waiver", "free_agent", "trade"
    faab: int | None  # FAAB dollars spent, waiver adds only


@dataclass
class TeamRosterMoves:
    roster_id: int
    added: list[PlayerMove]
    dropped: list[PlayerMove]


@dataclass
class PositionRankRow:
    roster_id: int
    points: dict[str, float]  # position -> starter points this week
    ranks: dict[str, int]  # position -> league rank, 1 = most points; ties share a rank


@dataclass
class LineupRow:
    roster_id: int
    actual: float  # starters' points as the manager set the lineup
    optimal: float  # best legal lineup from the same players
    should_have_started: list[PlayerScore]  # benched players in the best lineup, best first
    should_have_sat: list[PlayerScore]  # starters it left out, worst first

    @property
    def left_on_bench(self) -> float:
        return self.optimal - self.actual

    @property
    def efficiency(self) -> float:
        return self.actual / self.optimal if self.optimal > 0 else 1.0

    @property
    def swaps(self) -> list[tuple[PlayerScore, PlayerScore | None]]:
        """Best benched player paired with worst misplaced starter, and so on.

        None on the right means the player belonged in a slot left empty.
        """
        sat = self.should_have_sat
        return [
            (start, sat[i] if i < len(sat) else None)
            for i, start in enumerate(self.should_have_started)
        ]


@dataclass
class WeekSummary:
    week: int
    matchups: list[Matchup]
    awards: WeekAwards
    allplay_week_wins: dict[int, int]  # roster_id -> teams outscored this week
    power_rankings: list[PowerRankRow]  # sorted by rank ascending
    roster_moves: list[TeamRosterMoves]  # only teams with at least one move
    position_ranks: list[PositionRankRow]  # one row per roster, roster_id order
    lineups: list[LineupRow] = field(default_factory=list)  # empty without roster slots


@dataclass
class SeasonBoardEntry:
    week: int
    top_scorer_roster_ids: list[int]
    top_score: float


@dataclass
class PFLeaderboardRow:
    roster_id: int
    pf: float
    pa: float
    avg: float
    best_week: int
    best_week_score: float
    worst_week: int
    worst_week_score: float
    bench_points: float


@dataclass
class GameRecord:
    week: int
    roster_a: int
    points_a: float
    roster_b: int
    points_b: float
    margin: float


@dataclass
class SeasonRecords:
    highest_team_score: tuple[list[int], int, float]  # (roster_ids, week, score)
    lowest_team_score: tuple[list[int], int, float]
    highest_starter: tuple[list[tuple[int, PlayerScore]], int]  # (roster/player pairs, week)
    lowest_starter: tuple[list[tuple[int, PlayerScore]], int]
    highest_bench_player: tuple[list[tuple[int, PlayerScore]], int]
    closest_games: list[GameRecord]  # every game tied at the season's smallest margin
    biggest_blowouts: list[GameRecord]  # every game tied at the season's largest margin


@dataclass
class SeasonLineupRow:
    roster_id: int
    actual: float
    optimal: float
    weeks: int
    perfect_weeks: int  # weeks where the lineup set was already the best possible

    @property
    def left_on_bench(self) -> float:
        return self.optimal - self.actual

    @property
    def efficiency(self) -> float:
        return self.actual / self.optimal if self.optimal > 0 else 1.0


@dataclass
class SeasonBoard:
    log: list[SeasonBoardEntry]
    crown_counts: dict[int, int]
    pf_leaderboard: list[PFLeaderboardRow]  # sorted by PF descending
    records: SeasonRecords
    lineup_leaderboard: list[SeasonLineupRow] = field(default_factory=list)  # best efficiency first


@dataclass
class PreseasonRow:
    roster_id: int
    rank: int
    prev_rank: int | None  # None for a roster with no history in the previous league
    prev_record: str | None
    prev_pf: float | None
    new_owner: bool
    champion: bool


@dataclass
class DraftPickGrade:
    pick_no: int
    round: int
    roster_id: int
    player: PlayerScore  # points field unused here; name/position/team
    amount: int
    expected: float
    surplus: float  # expected - amount; positive = bargain
    is_keeper: bool


@dataclass
class TeamDraftGrade:
    roster_id: int
    grade: str
    spent: int
    value: float
    surplus: float
    picks: list[DraftPickGrade]  # this team's non-keeper picks, best surplus first
    keepers: list[DraftPickGrade]


@dataclass
class DraftSummary:
    budget: int
    teams: list[TeamDraftGrade]  # sorted by surplus descending
    steals: list[DraftPickGrade]  # league-wide, best surplus first
    overpays: list[DraftPickGrade]  # league-wide, worst surplus first


@dataclass
class SeasonSummary:
    season: str
    league_name: str
    teams: dict[int, Team]
    weeks: list[WeekSummary]
    season_board: SeasonBoard
    playoff_week_start: int
    through_week: int
    provisional_week: int | None
    roster_positions: list[str] = field(default_factory=list)
    provisional_week_summary: WeekSummary | None = None
    preseason: list[PreseasonRow] = field(default_factory=list)
    draft: DraftSummary | None = None
    # How the power rankings were computed, for the how-it-works page.
    weights: dict[str, float] = field(default_factory=dict)
    form_window: int = 0
    league_average_match: bool = False

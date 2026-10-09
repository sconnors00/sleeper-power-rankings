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
    owner_name: str | None = None  # the manager's Sleeper display name
    color_dark: str | None = None  # the same team's color in the dark theme


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
    # benched player -> starter he should have replaced (None: an empty slot), biggest gain first
    swaps: list[tuple[PlayerScore, PlayerScore | None]] = field(default_factory=list)

    @property
    def left_on_bench(self) -> float:
        return self.optimal - self.actual

    @property
    def efficiency(self) -> float:
        return self.actual / self.optimal if self.optimal > 0 else 1.0


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
class Acquisition:
    week: int
    roster_id: int  # the team that acquired the player
    player: PlayerScore  # points = points this team started him for since he arrived
    move_type: str  # "waiver", "free_agent" or "trade"
    faab: int | None  # waiver bid, waiver adds only
    starts: int  # weeks this team started him


@dataclass
class TradeSide:
    roster_id: int
    received: list[Acquisition]
    picks: list[str]  # draft picks received, e.g. "2027 Rd 2"

    @property
    def points(self) -> float:
        return sum(a.player.points for a in self.received)


@dataclass
class TradeGrade:
    week: int
    sides: list[TradeSide]  # most points since the trade first


@dataclass
class AcquisitionSummary:
    pickups: list[Acquisition]  # waiver and free-agent adds, most starter points first
    trades: list[TradeGrade]  # newest first


@dataclass
class Meeting:
    season: str
    week: int
    points_for: float
    points_against: float

    @property
    def result(self) -> str:
        if self.points_for > self.points_against:
            return "W"
        return "L" if self.points_for < self.points_against else "T"


@dataclass
class HeadToHead:
    opponent_id: str
    meetings: list[Meeting]  # oldest first

    @property
    def wins(self) -> int:
        return sum(m.result == "W" for m in self.meetings)

    @property
    def losses(self) -> int:
        return sum(m.result == "L" for m in self.meetings)

    @property
    def ties(self) -> int:
        return sum(m.result == "T" for m in self.meetings)

    @property
    def record(self) -> str:
        base = f"{self.wins}-{self.losses}"
        return f"{base}-{self.ties}" if self.ties else base

    @property
    def points_for(self) -> float:
        return sum(m.points_for for m in self.meetings)

    @property
    def points_against(self) -> float:
        return sum(m.points_against for m in self.meetings)

    @property
    def streak(self) -> tuple[str, int]:
        """The current run of identical results, newest meeting backwards."""
        last = self.meetings[-1].result
        run = 0
        for m in reversed(self.meetings):
            if m.result != last:
                break
            run += 1
        return last, run


@dataclass
class Manager:
    owner_id: str
    name: str  # Sleeper display name as of the latest season they played
    color: str
    seasons: list[str]  # newest first
    rivals: list[HeadToHead]  # most meetings first
    color_dark: str | None = None  # the same color in the dark theme

    @property
    def wins(self) -> int:
        return sum(h.wins for h in self.rivals)

    @property
    def losses(self) -> int:
        return sum(h.losses for h in self.rivals)

    @property
    def ties(self) -> int:
        return sum(h.ties for h in self.rivals)

    @property
    def win_pct(self) -> float:
        games = self.wins + self.losses + self.ties
        return (self.wins + 0.5 * self.ties) / games if games else 0.0


@dataclass
class PlayoffOddsRow:
    roster_id: int
    record: str  # current, median games included when the league plays them
    projected_wins: float  # mean final regular-season wins across simulations
    playoff_pct: float
    bye_pct: float
    top_seed_pct: float


@dataclass
class PlayerValue:
    player: PlayerScore  # points = projected points per week
    roster_id: int | None  # the fantasy team that has him now
    games: int  # weeks he scored while on a roster this season
    ppg: float  # his average over those games
    prior: float  # points per game his Sleeper ranking implies
    value: float  # surplus, scaled by the share of the weeks left he's expected to play
    injury: str | None  # short tag: "Q", "D", "Out", "IR", ...
    active: bool  # False while on IR or the taxi squad, so he can't start
    surplus: float = 0.0  # projection above a replacement starter at his position, floored at 0
    availability: list[float] = field(default_factory=list)  # share of each horizon week he plays

    @property
    def weeks_available(self) -> float:
        return sum(self.availability)

    @property
    def projection(self) -> float:
        return self.player.points


@dataclass
class TeamStrengthRow:
    roster_id: int
    total: float  # best lineup's projected points per week
    rank: int
    points: dict[str, float]  # position -> that lineup's projected points there
    ranks: dict[str, int]  # position -> league rank, 1 = most; ties share a rank


@dataclass
class TradeValues:
    players: dict[str, PlayerValue]  # every rostered or recently rostered player
    replacement: dict[str, float]  # position -> replacement starter's projection
    rosters: dict[int, list[str]]  # roster id -> player ids, most valuable first
    strength: list[TeamStrengthRow]  # best projected lineup first
    prior_games: int  # games of evidence the ranking-based prior is worth
    weeks_left: int  # regular-season weeks still to play
    roster_limit: int  # starters plus bench, IR and taxi aside
    trade_deadline: int | None
    horizon: list[int] = field(default_factory=list)  # weeks still to play, playoffs included
    weights: list[float] = field(default_factory=list)  # each horizon week's weight in value
    playoff_from: int | None = None  # first playoff week, if the horizon reaches it


@dataclass
class RosterSpot:
    player_id: str
    name: str
    position: str
    nfl_team: str | None
    injury: str | None  # Sleeper's injury_status, e.g. "Questionable", "IR"
    slot: str  # lineup slot for starters ("QB", "FLEX", ...); "BN", "IR" or "TAXI" otherwise


@dataclass
class FaabClaim:
    week: int
    player: PlayerScore  # points field unused here
    bid: int
    dropped: list[str]  # names of the players dropped to make room


@dataclass
class TeamRoster:
    roster_id: int
    record: str
    starters: list[RosterSpot]
    bench: list[RosterSpot]
    ir: list[RosterSpot]
    taxi: list[RosterSpot]
    faab_used: int
    faab_remaining: int
    claims: list[FaabClaim]  # winning waiver bids, oldest first

    @property
    def players(self) -> list[RosterSpot]:
        return [*self.starters, *self.bench, *self.ir, *self.taxi]


@dataclass
class RosterReport:
    faab_budget: int  # 0 when the league doesn't use FAAB
    teams: list[TeamRoster]  # roster_id order


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
    playoff_odds: list[PlayoffOddsRow] = field(default_factory=list)  # best odds first
    playoff_odds_sims: int = 0
    playoff_teams: int = 0
    playoff_byes: int = 0
    remaining_weeks: int = 0  # regular-season weeks the odds simulate
    acquisitions: AcquisitionSummary | None = None
    trade_values: TradeValues | None = None  # current season only
    roster_report: RosterReport | None = None

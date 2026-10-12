//! Plain data shared between `compute` and `build`. Everything the templates
//! read is `Serialize`; values the Python version computed as properties are
//! stored as fields, filled in when the struct is built.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub roster_id: i64,
    pub owner_id: Option<String>,
    pub name: String,
    pub avatar_url: Option<String>,
    pub color: String,
    /// The manager's Sleeper display name.
    pub owner_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlayerScore {
    pub player_id: String,
    pub name: String,
    pub position: String,
    pub nfl_team: Option<String>,
    pub points: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Matchup {
    pub week: i64,
    pub matchup_id: i64,
    pub roster_id: i64,
    pub opponent_roster_id: Option<i64>,
    pub team_points: f64,
    pub starters: Vec<PlayerScore>,
    pub bench: Vec<PlayerScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekAwards {
    pub highest_score_roster_ids: Vec<i64>,
    pub highest_score: f64,
    pub lowest_score_roster_ids: Vec<i64>,
    pub lowest_score: f64,
    /// (roster_id, player) pairs.
    pub best_bench: Vec<(i64, PlayerScore)>,
    pub best_bench_points: f64,
    pub bench_points_by_roster: BTreeMap<i64, f64>,
    pub best_starter: Vec<(i64, PlayerScore)>,
    pub best_starter_points: f64,
    pub worst_starter: Vec<(i64, PlayerScore)>,
    pub worst_starter_points: f64,
    pub worst_starter_zero: bool,
    pub closest_matchup_ids: Vec<i64>,
    pub closest_margin: f64,
    pub biggest_blowout_matchup_ids: Vec<i64>,
    pub biggest_blowout_margin: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PowerRankRow {
    pub roster_id: i64,
    pub rank: i64,
    pub score: f64,
    pub win_pct: f64,
    pub allplay_pct: f64,
    pub pf_norm: f64,
    pub form_norm: f64,
    pub luck: f64,
    pub record: String,
    pub allplay_record: String,
    /// Positive = moved up, negative = moved down, None = no earlier week.
    pub movement: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerMove {
    pub player: PlayerScore,
    /// "waiver", "free_agent" or "trade".
    pub move_type: String,
    /// FAAB dollars spent, waiver adds only.
    pub faab: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamRosterMoves {
    pub roster_id: i64,
    pub added: Vec<PlayerMove>,
    pub dropped: Vec<PlayerMove>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionRankRow {
    pub roster_id: i64,
    /// Position -> starter points this week.
    pub points: BTreeMap<String, f64>,
    /// Position -> league rank, 1 = most points; ties share a rank.
    pub ranks: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineupRow {
    pub roster_id: i64,
    /// Starters' points as the manager set the lineup.
    pub actual: f64,
    /// Best legal lineup from the same players.
    pub optimal: f64,
    pub should_have_started: Vec<PlayerScore>,
    pub should_have_sat: Vec<PlayerScore>,
    /// Benched player -> starter he should have replaced (None: an empty slot).
    pub swaps: Vec<(PlayerScore, Option<PlayerScore>)>,
    pub left_on_bench: f64,
    pub efficiency: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekSummary {
    pub week: i64,
    pub matchups: Vec<Matchup>,
    pub awards: WeekAwards,
    pub allplay_week_wins: BTreeMap<i64, i64>,
    /// Sorted by rank ascending.
    pub power_rankings: Vec<PowerRankRow>,
    /// Only teams with at least one move.
    pub roster_moves: Vec<TeamRosterMoves>,
    /// One row per roster, roster_id order.
    pub position_ranks: Vec<PositionRankRow>,
    /// Empty without roster slots.
    pub lineups: Vec<LineupRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonBoardEntry {
    pub week: i64,
    pub top_scorer_roster_ids: Vec<i64>,
    pub top_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PfLeaderboardRow {
    pub roster_id: i64,
    pub pf: f64,
    pub pa: f64,
    pub avg: f64,
    pub best_week: i64,
    pub best_week_score: f64,
    pub worst_week: i64,
    pub worst_week_score: f64,
    pub bench_points: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GameRecord {
    pub week: i64,
    pub roster_a: i64,
    pub points_a: f64,
    pub roster_b: i64,
    pub points_b: f64,
    pub margin: f64,
}

/// (value holders, week) style records. Each holds every roster/player tied
/// for the record.
#[derive(Debug, Clone, Serialize)]
pub struct SeasonRecords {
    pub highest_team_score: (Vec<i64>, i64, f64),
    pub lowest_team_score: (Vec<i64>, i64, f64),
    pub highest_starter: (Vec<(i64, PlayerScore)>, i64),
    pub lowest_starter: (Vec<(i64, PlayerScore)>, i64),
    pub highest_bench_player: (Vec<(i64, PlayerScore)>, i64),
    pub closest_games: Vec<GameRecord>,
    pub biggest_blowouts: Vec<GameRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonLineupRow {
    pub roster_id: i64,
    pub actual: f64,
    pub optimal: f64,
    pub weeks: i64,
    /// Weeks where the lineup set was already the best possible.
    pub perfect_weeks: i64,
    pub left_on_bench: f64,
    pub efficiency: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonBoard {
    pub log: Vec<SeasonBoardEntry>,
    /// (roster_id, crowns) in roster order.
    pub crown_counts: Vec<(i64, i64)>,
    /// Sorted by PF descending.
    pub pf_leaderboard: Vec<PfLeaderboardRow>,
    pub records: SeasonRecords,
    /// Best efficiency first.
    pub lineup_leaderboard: Vec<SeasonLineupRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreseasonRow {
    pub roster_id: i64,
    pub rank: i64,
    /// None for a roster with no history in the previous league.
    pub prev_rank: Option<i64>,
    pub prev_record: Option<String>,
    pub prev_pf: Option<f64>,
    pub new_owner: bool,
    pub champion: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftPickGrade {
    pub pick_no: i64,
    pub round: i64,
    pub roster_id: i64,
    pub player: PlayerScore,
    pub amount: i64,
    pub expected: f64,
    /// expected - amount; positive = bargain.
    pub surplus: f64,
    pub is_keeper: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamDraftGrade {
    pub roster_id: i64,
    pub grade: String,
    pub spent: i64,
    pub value: f64,
    pub surplus: f64,
    /// This team's non-keeper picks, best surplus first.
    pub picks: Vec<DraftPickGrade>,
    pub keepers: Vec<DraftPickGrade>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DraftSummary {
    pub budget: i64,
    /// Sorted by surplus descending.
    pub teams: Vec<TeamDraftGrade>,
    pub steals: Vec<DraftPickGrade>,
    pub overpays: Vec<DraftPickGrade>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Acquisition {
    pub week: i64,
    /// The team that acquired the player.
    pub roster_id: i64,
    /// points = points this team started him for since he arrived.
    pub player: PlayerScore,
    pub move_type: String,
    /// Waiver bid, waiver adds only.
    pub faab: Option<i64>,
    /// Weeks this team started him.
    pub starts: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeSide {
    pub roster_id: i64,
    pub received: Vec<Acquisition>,
    /// Draft picks received, e.g. "2027 Rd 2".
    pub picks: Vec<String>,
    /// Points the received players scored for this side.
    pub points: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeGrade {
    pub week: i64,
    /// Most points since the trade first.
    pub sides: Vec<TradeSide>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcquisitionSummary {
    /// Waiver and free-agent adds, most starter points first.
    pub pickups: Vec<Acquisition>,
    /// Newest first.
    pub trades: Vec<TradeGrade>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Meeting {
    pub season: String,
    pub week: i64,
    pub points_for: f64,
    pub points_against: f64,
    /// "W", "L" or "T".
    pub result: String,
}

impl Meeting {
    pub fn new(season: &str, week: i64, points_for: f64, points_against: f64) -> Self {
        let result = if points_for > points_against {
            "W"
        } else if points_for < points_against {
            "L"
        } else {
            "T"
        };
        Meeting {
            season: season.to_string(),
            week,
            points_for,
            points_against,
            result: result.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HeadToHead {
    pub opponent_id: String,
    /// Oldest first.
    pub meetings: Vec<Meeting>,
    pub wins: i64,
    pub losses: i64,
    pub ties: i64,
    pub record: String,
    pub points_for: f64,
    pub points_against: f64,
    /// The current run of identical results, newest meeting backwards.
    pub streak: (String, i64),
}

impl HeadToHead {
    pub fn new(opponent_id: String, meetings: Vec<Meeting>) -> Self {
        let count = |code: &str| meetings.iter().filter(|m| m.result == code).count() as i64;
        let (wins, losses, ties) = (count("W"), count("L"), count("T"));
        let base = format!("{wins}-{losses}");
        let record = if ties > 0 {
            format!("{base}-{ties}")
        } else {
            base
        };
        // Python's sum() is compensated; a plain fold can land a cent off.
        let points_for = crate::compute::sum_f64(meetings.iter().map(|m| m.points_for));
        let points_against = crate::compute::sum_f64(meetings.iter().map(|m| m.points_against));
        let last = meetings
            .last()
            .map(|m| m.result.clone())
            .unwrap_or_default();
        let run = meetings
            .iter()
            .rev()
            .take_while(|m| m.result == last)
            .count() as i64;
        HeadToHead {
            opponent_id,
            meetings,
            wins,
            losses,
            ties,
            record,
            points_for,
            points_against,
            streak: (last, run),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Manager {
    pub owner_id: String,
    /// Sleeper display name as of the latest season they played.
    pub name: String,
    pub color: String,
    /// Newest first.
    pub seasons: Vec<String>,
    /// Most meetings first.
    pub rivals: Vec<HeadToHead>,
    pub wins: i64,
    pub losses: i64,
    pub ties: i64,
    pub win_pct: f64,
}

impl Manager {
    pub fn new(
        owner_id: String,
        name: String,
        color: String,
        seasons: Vec<String>,
        rivals: Vec<HeadToHead>,
    ) -> Self {
        let wins: i64 = rivals.iter().map(|h| h.wins).sum();
        let losses: i64 = rivals.iter().map(|h| h.losses).sum();
        let ties: i64 = rivals.iter().map(|h| h.ties).sum();
        let games = wins + losses + ties;
        let win_pct = if games > 0 {
            (wins as f64 + 0.5 * ties as f64) / games as f64
        } else {
            0.0
        };
        Manager {
            owner_id,
            name,
            color,
            seasons,
            rivals,
            wins,
            losses,
            ties,
            win_pct,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayoffOddsRow {
    pub roster_id: i64,
    /// Current record; median games included when the league plays them.
    pub record: String,
    /// Mean final regular-season wins across simulations.
    pub projected_wins: f64,
    pub playoff_pct: f64,
    pub bye_pct: f64,
    pub top_seed_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerValue {
    /// points = projected points per week.
    pub player: PlayerScore,
    /// The fantasy team that has him now.
    pub roster_id: Option<i64>,
    /// Weeks he scored while on a roster this season.
    pub games: i64,
    /// His average over those games.
    pub ppg: f64,
    /// Points per game his Sleeper ranking implies.
    pub prior: f64,
    /// Surplus scaled by the share of the weeks left he's expected to play.
    pub value: f64,
    /// Short tag: "Q", "D", "Out", "IR", ...
    pub injury: Option<String>,
    /// False while on IR or the taxi squad, so he can't start.
    pub active: bool,
    /// Projection above a replacement starter, floored at 0.
    pub surplus: f64,
    /// Share of each horizon week he plays.
    pub availability: Vec<f64>,
    pub projection: f64,
    pub weeks_available: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamStrengthRow {
    pub roster_id: i64,
    /// Best lineup's projected points per week.
    pub total: f64,
    pub rank: i64,
    pub points: BTreeMap<String, f64>,
    pub ranks: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradeValues {
    /// Every rostered or recently rostered player, by player id.
    pub players: BTreeMap<String, PlayerValue>,
    /// Position -> replacement starter's projection.
    pub replacement: BTreeMap<String, f64>,
    /// Roster id -> player ids, most valuable first, in league roster order.
    pub rosters: Vec<(i64, Vec<String>)>,
    /// Best projected lineup first.
    pub strength: Vec<TeamStrengthRow>,
    pub prior_games: i64,
    pub weeks_left: i64,
    /// Starters plus bench, IR and taxi aside.
    pub roster_limit: i64,
    pub trade_deadline: Option<i64>,
    /// Weeks still to play, playoffs included.
    pub horizon: Vec<i64>,
    /// Each horizon week's weight in value.
    pub weights: Vec<f64>,
    /// First playoff week, if the horizon reaches it.
    pub playoff_from: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RosterSpot {
    pub player_id: String,
    pub name: String,
    pub position: String,
    pub nfl_team: Option<String>,
    /// Sleeper's injury_status, e.g. "Questionable", "IR".
    pub injury: Option<String>,
    /// Lineup slot for starters ("QB", "FLEX", ...); "BN", "IR" or "TAXI" otherwise.
    pub slot: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FaabClaim {
    pub week: i64,
    pub player: PlayerScore,
    pub bid: i64,
    /// Names of the players dropped to make room.
    pub dropped: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamRoster {
    pub roster_id: i64,
    pub record: String,
    pub starters: Vec<RosterSpot>,
    pub bench: Vec<RosterSpot>,
    pub ir: Vec<RosterSpot>,
    pub taxi: Vec<RosterSpot>,
    pub faab_used: i64,
    pub faab_remaining: i64,
    /// Winning waiver bids, oldest first.
    pub claims: Vec<FaabClaim>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RosterReport {
    /// 0 when the league doesn't use FAAB.
    pub faab_budget: i64,
    /// Roster id order.
    pub teams: Vec<TeamRoster>,
}

/// The power-ranking weights (config.toml `[weights]`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Weights {
    pub win_pct: f64,
    pub allplay_pct: f64,
    pub pf_norm: f64,
    pub form_norm: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Weights {
            win_pct: 0.30,
            allplay_pct: 0.25,
            pf_norm: 0.30,
            form_norm: 0.15,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SeasonSummary {
    pub season: String,
    pub league_name: String,
    pub teams: BTreeMap<i64, Team>,
    pub weeks: Vec<WeekSummary>,
    pub season_board: SeasonBoard,
    pub playoff_week_start: i64,
    pub through_week: i64,
    pub provisional_week: Option<i64>,
    pub roster_positions: Vec<String>,
    pub provisional_week_summary: Option<WeekSummary>,
    pub preseason: Vec<PreseasonRow>,
    pub draft: Option<DraftSummary>,
    /// How the power rankings were computed, for the how-it-works page.
    pub weights: Weights,
    pub form_window: i64,
    pub league_average_match: bool,
    /// Best odds first.
    pub playoff_odds: Vec<PlayoffOddsRow>,
    pub playoff_odds_sims: i64,
    pub playoff_teams: i64,
    pub playoff_byes: i64,
    /// Regular-season weeks the odds simulate.
    pub remaining_weeks: i64,
    pub acquisitions: Option<AcquisitionSummary>,
    /// Current season only.
    pub trade_values: Option<TradeValues>,
    pub roster_report: Option<RosterReport>,
}

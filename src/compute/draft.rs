//! Auction draft grades: each pick's surplus is its market value (from the
//! league's own price-vs-rank curve) minus what was paid. Keepers are listed
//! but excluded, since their prices come from keeper rules, not bidding.

use indexmap::IndexMap;

use super::player_score;
use crate::models::{DraftPickGrade, DraftSummary, TeamDraftGrade};
use crate::raw::{int_of, PlayerMap, RawPick};
use crate::util::median;

/// (search_rank, amount) pairs from the non-keeper picks, sorted by rank.
pub type PriceCurve = Vec<(i64, i64)>;

pub fn fit_price_curve(picks: &[RawPick], players: &PlayerMap) -> PriceCurve {
    let mut points: PriceCurve = Vec::new();
    for p in picks {
        if p.is_keeper.unwrap_or(false) {
            continue;
        }
        let rank = players.get(&p.player_id).and_then(|info| info.rank());
        let amount = int_of(p.metadata.amount.as_ref());
        if let Some(rank) = rank {
            points.push((rank, amount));
        }
    }
    points.sort();
    points
}

/// The median auction price among the curve points nearest in rank. The
/// window narrows near the top of the board, and widens out on the flat tail.
/// An unknown rank is worth the $1 floor.
pub fn expected_price(rank: Option<i64>, curve: &PriceCurve) -> f64 {
    const MAX_WINDOW: i64 = 15;
    let Some(rank) = rank else {
        return 1.0;
    };
    if curve.is_empty() {
        return 1.0;
    }
    let window = rank.div_euclid(3).clamp(5, MAX_WINDOW) as usize;
    let mut nearest: Vec<&(i64, i64)> = curve.iter().collect();
    nearest.sort_by_key(|(point_rank, _)| (point_rank - rank).abs());
    let amounts: Vec<f64> = nearest
        .iter()
        .take(window)
        .map(|(_, a)| *a as f64)
        .collect();
    median(&amounts).max(1.0)
}

fn letter_grade(z: f64) -> &'static str {
    const STEPS: [(f64, &str); 8] = [
        (1.5, "A+"),
        (1.0, "A"),
        (0.5, "A-"),
        (0.15, "B+"),
        (-0.15, "B"),
        (-0.5, "B-"),
        (-1.0, "C+"),
        (-1.5, "C"),
    ];
    STEPS
        .iter()
        .find(|(threshold, _)| z >= *threshold)
        .map_or("D", |(_, letter)| letter)
}

pub fn grade_draft(picks: &[RawPick], players: &PlayerMap, budget: i64) -> DraftSummary {
    const STEAL_COUNT: usize = 10;
    const MIN_OVERPAY_AMOUNT: i64 = 5;

    let curve = fit_price_curve(picks, players);
    let graded: Vec<DraftPickGrade> = picks
        .iter()
        .map(|p| {
            let amount = int_of(p.metadata.amount.as_ref());
            let rank = players.get(&p.player_id).and_then(|info| info.rank());
            let expected = expected_price(rank, &curve);
            DraftPickGrade {
                pick_no: p.pick_no,
                round: p.round,
                roster_id: p.roster_id,
                player: player_score(&p.player_id, 0.0, players),
                amount,
                expected,
                surplus: expected - amount as f64,
                is_keeper: p.is_keeper.unwrap_or(false),
            }
        })
        .collect();

    let mut by_roster: IndexMap<i64, Vec<&DraftPickGrade>> = IndexMap::new();
    for g in &graded {
        by_roster.entry(g.roster_id).or_default().push(g);
    }

    let mut team_rows: Vec<TeamDraftGrade> = by_roster
        .iter()
        .map(|(rid, team_picks)| {
            let mut open: Vec<DraftPickGrade> = team_picks
                .iter()
                .filter(|g| !g.is_keeper)
                .map(|g| (*g).clone())
                .collect();
            let keepers: Vec<DraftPickGrade> = team_picks
                .iter()
                .filter(|g| g.is_keeper)
                .map(|g| (*g).clone())
                .collect();
            open.sort_by(|a, b| {
                b.surplus
                    .partial_cmp(&a.surplus)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            TeamDraftGrade {
                roster_id: *rid,
                grade: String::new(),
                spent: open.iter().map(|g| g.amount).sum(),
                value: crate::compute::sum_f64(open.iter().map(|g| g.expected)),
                surplus: crate::compute::sum_f64(open.iter().map(|g| g.surplus)),
                picks: open,
                keepers,
            }
        })
        .collect();

    let surpluses: Vec<f64> = team_rows.iter().map(|t| t.surplus).collect();
    let count = surpluses.len() as f64;
    let mean = if surpluses.is_empty() {
        0.0
    } else {
        crate::compute::sum_f64(surpluses.iter().copied()) / count
    };
    let variance = if surpluses.is_empty() {
        0.0
    } else {
        crate::compute::sum_f64(surpluses.iter().map(|s| (s - mean) * (s - mean))) / count
    };
    let std = crate::util::pow_half(variance);
    for t in &mut team_rows {
        let z = if std > 0.0 {
            (t.surplus - mean) / std
        } else {
            0.0
        };
        t.grade = letter_grade(z).to_string();
    }
    team_rows.sort_by(|a, b| {
        b.surplus
            .partial_cmp(&a.surplus)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let open_graded: Vec<&DraftPickGrade> = graded.iter().filter(|g| !g.is_keeper).collect();
    let mut steals: Vec<DraftPickGrade> = open_graded
        .iter()
        .filter(|g| g.surplus > 0.0)
        .map(|g| (*g).clone())
        .collect();
    steals.sort_by(|a, b| {
        b.surplus
            .partial_cmp(&a.surplus)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    steals.truncate(STEAL_COUNT);

    let mut overpays: Vec<DraftPickGrade> = open_graded
        .iter()
        .filter(|g| g.surplus < 0.0 && g.amount >= MIN_OVERPAY_AMOUNT)
        .map(|g| (*g).clone())
        .collect();
    overpays.sort_by(|a, b| {
        a.surplus
            .partial_cmp(&b.surplus)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    overpays.truncate(STEAL_COUNT);

    DraftSummary {
        budget,
        teams: team_rows,
        steals,
        overpays,
    }
}

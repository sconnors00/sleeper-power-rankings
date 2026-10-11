//! Pickup and trade grades: every add is credited with the points the
//! acquiring team started the player for after he arrived.

use indexmap::IndexMap;

use super::player_score;
use crate::models::{Acquisition, AcquisitionSummary, TradeGrade, TradeSide, WeekSummary};
use crate::raw::{PlayerMap, RawTransaction};
use crate::util::round2;

/// A trade side while it's being built: indexes into the acquisition arena.
struct SideDraft {
    roster_id: i64,
    received: Vec<usize>,
    picks: Vec<String>,
}

pub fn grade_acquisitions(
    weeks_raw_transactions: &std::collections::BTreeMap<i64, Vec<RawTransaction>>,
    weeks: &[WeekSummary],
    players: &PlayerMap,
) -> AcquisitionSummary {
    // Every acquisition, in the order they happened. Trade sides and the
    // per-player stints both point into this arena so credit lands once.
    let mut arena: Vec<Acquisition> = Vec::new();
    let mut stints: IndexMap<(i64, String), Vec<usize>> = IndexMap::new();
    let mut trades: Vec<(i64, IndexMap<i64, SideDraft>)> = Vec::new();

    for (&week, txs) in weeks_raw_transactions {
        let mut complete: Vec<&RawTransaction> = txs
            .iter()
            .filter(|t| t.status.as_deref() == Some("complete"))
            .collect();
        complete.sort_by_key(|t| t.status_updated.or(t.created).unwrap_or(0));
        for tx in complete {
            let move_type = tx.kind.clone().unwrap_or_else(|| "waiver".to_string());
            let faab = if move_type == "waiver" {
                tx.settings.waiver_bid
            } else {
                None
            };
            let mut sides: IndexMap<i64, SideDraft> = IndexMap::new();
            for (pid, rid) in &tx.adds {
                arena.push(Acquisition {
                    week,
                    roster_id: *rid,
                    player: player_score(pid, 0.0, players),
                    move_type: move_type.clone(),
                    faab,
                    starts: 0,
                });
                let idx = arena.len() - 1;
                stints.entry((*rid, pid.clone())).or_default().push(idx);
                if move_type == "trade" {
                    side(&mut sides, *rid).received.push(idx);
                }
            }
            if move_type == "trade" {
                for rid in &tx.roster_ids {
                    side(&mut sides, *rid);
                }
                for pick in &tx.draft_picks {
                    let Some(owner) = pick.owner_id else { continue };
                    if let Some(s) = sides.get_mut(&owner) {
                        s.picks.push(format!(
                            "{} Rd {}",
                            season_text(&pick.season),
                            pick.round
                                .map_or_else(|| "None".to_string(), |r| r.to_string())
                        ));
                    }
                }
                trades.push((week, sides));
            }
        }
    }

    // Credit each starter to his team's latest acquisition at or before that week.
    for wk in weeks {
        for m in &wk.matchups {
            for p in &m.starters {
                let Some(list) = stints.get(&(m.roster_id, p.player_id.clone())) else {
                    continue;
                };
                let current = list
                    .iter()
                    .rev()
                    .copied()
                    .find(|idx| arena[*idx].week <= wk.week);
                if let Some(idx) = current {
                    let acq = &mut arena[idx];
                    acq.player.points = round2(acq.player.points + p.points);
                    acq.starts += 1;
                }
            }
        }
    }

    let mut pickups: Vec<Acquisition> = stints
        .values()
        .flatten()
        .map(|idx| arena[*idx].clone())
        .filter(|a| a.move_type == "waiver" || a.move_type == "free_agent")
        .collect();
    pickups.sort_by(|a, b| {
        b.player
            .points
            .partial_cmp(&a.player.points)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.week.cmp(&b.week))
            .then_with(|| a.player.name.cmp(&b.player.name))
    });

    let mut graded: Vec<TradeGrade> = trades
        .into_iter()
        .map(|(week, sides)| {
            let mut built: Vec<TradeSide> = sides
                .into_values()
                .map(|s| {
                    let received: Vec<Acquisition> =
                        s.received.iter().map(|idx| arena[*idx].clone()).collect();
                    TradeSide {
                        roster_id: s.roster_id,
                        points: crate::compute::sum_f64(received.iter().map(|a| a.player.points)),
                        received,
                        picks: s.picks,
                    }
                })
                .collect();
            built.sort_by(|a, b| {
                b.points
                    .partial_cmp(&a.points)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.roster_id.cmp(&b.roster_id))
            });
            TradeGrade { week, sides: built }
        })
        .collect();
    graded.reverse();

    AcquisitionSummary {
        pickups,
        trades: graded,
    }
}

fn side(sides: &mut IndexMap<i64, SideDraft>, rid: i64) -> &mut SideDraft {
    sides.entry(rid).or_insert_with(|| SideDraft {
        roster_id: rid,
        received: Vec::new(),
        picks: Vec::new(),
    })
}

/// A pick's season as Sleeper sends it (usually a string like "2027").
fn season_text(season: &serde_json::Value) -> String {
    crate::raw::text_of(season)
}

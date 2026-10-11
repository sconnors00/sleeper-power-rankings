//! Every team's roster split into lineup, bench, IR and taxi, with FAAB.

use std::collections::{BTreeMap, HashSet};

use super::matchups::official_record_string;
use super::{is_starting_slot, player_score, POSITIONS};
use crate::models::{FaabClaim, RosterReport, RosterSpot, TeamRoster};
use crate::raw::{PlayerMap, RawRoster, RawTransaction};

pub fn build_roster_report(
    rosters: &[RawRoster],
    players: &PlayerMap,
    roster_positions: &[String],
    faab_budget: i64,
    weeks_raw_transactions: &BTreeMap<i64, Vec<RawTransaction>>,
) -> RosterReport {
    let slots: Vec<&str> = roster_positions
        .iter()
        .map(String::as_str)
        .filter(|s| is_starting_slot(s))
        .collect();

    let spot = |pid: &str, slot: &str| -> RosterSpot {
        let info = players.get(pid);
        let player = player_score(pid, 0.0, players);
        RosterSpot {
            player_id: pid.to_string(),
            name: player.name,
            position: player.position,
            nfl_team: player.nfl_team,
            injury: info.and_then(|p| p.injury_status.clone()),
            slot: slot.to_string(),
        }
    };

    // Winning waiver bids, per roster, oldest first.
    let mut claims: BTreeMap<i64, Vec<FaabClaim>> = BTreeMap::new();
    for (&week, txs) in weeks_raw_transactions {
        let mut complete: Vec<&RawTransaction> = txs
            .iter()
            .filter(|t| t.status.as_deref() == Some("complete"))
            .collect();
        complete.sort_by_key(|t| t.status_updated.or(t.created).unwrap_or(0));
        for tx in complete {
            let Some(bid) = tx.settings.waiver_bid else {
                continue;
            };
            if tx.kind.as_deref() != Some("waiver") {
                continue;
            }
            let dropped: Vec<String> = tx
                .drops
                .keys()
                .map(|pid| player_score(pid, 0.0, players).name)
                .collect();
            for (pid, rid) in &tx.adds {
                claims.entry(*rid).or_default().push(FaabClaim {
                    week,
                    player: player_score(pid, 0.0, players),
                    bid,
                    dropped: dropped.clone(),
                });
            }
        }
    }

    let mut sorted: Vec<&RawRoster> = rosters.iter().collect();
    sorted.sort_by_key(|r| r.roster_id);
    let teams = sorted
        .into_iter()
        .map(|r| {
            let held: HashSet<&str> = r.players.iter().map(String::as_str).collect();
            let starter_ids = &r.starters;
            let ir_ids: Vec<&str> = r
                .reserve
                .iter()
                .map(String::as_str)
                .filter(|p| held.contains(p))
                .collect();
            let taxi_ids: Vec<&str> = r
                .taxi
                .iter()
                .map(String::as_str)
                .filter(|p| held.contains(p))
                .collect();
            let mut placed: HashSet<&str> = starter_ids
                .iter()
                .map(String::as_str)
                .filter(|p| *p != "0")
                .collect();
            placed.extend(ir_ids.iter().copied());
            placed.extend(taxi_ids.iter().copied());

            let starters: Vec<RosterSpot> = starter_ids
                .iter()
                .enumerate()
                .filter(|(_, pid)| pid.as_str() != "0")
                .map(|(i, pid)| spot(pid, slots.get(i).copied().unwrap_or("?")))
                .collect();

            let mut bench: Vec<RosterSpot> = r
                .players
                .iter()
                .filter(|pid| !placed.contains(pid.as_str()))
                .map(|pid| spot(pid, "BN"))
                .collect();
            bench.sort_by(|a, b| {
                position_order(&a.position)
                    .cmp(&position_order(&b.position))
                    .then_with(|| a.name.cmp(&b.name))
            });

            let used = r.settings.waiver_budget_used as i64;
            TeamRoster {
                roster_id: r.roster_id,
                record: official_record_string(r),
                starters,
                bench,
                ir: ir_ids.iter().map(|pid| spot(pid, "IR")).collect(),
                taxi: taxi_ids.iter().map(|pid| spot(pid, "TAXI")).collect(),
                faab_used: used,
                faab_remaining: faab_budget - used,
                claims: claims.get(&r.roster_id).cloned().unwrap_or_default(),
            }
        })
        .collect();

    RosterReport { faab_budget, teams }
}

fn position_order(position: &str) -> usize {
    POSITIONS.iter().position(|p| *p == position).unwrap_or(99)
}

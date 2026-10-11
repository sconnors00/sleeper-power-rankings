//! Pre-season rankings: last season's final power-ranking order, carried over
//! by roster id (Sleeper keeps roster ids stable across league renewals).

use std::collections::{BTreeMap, BTreeSet};

use crate::models::{PowerRankRow, PreseasonRow};

pub fn build_preseason_rankings(
    prev_final_rankings: &[PowerRankRow],
    prev_pf_by_roster: &BTreeMap<i64, f64>,
    current_roster_ids: &[i64],
    prev_owner: &BTreeMap<i64, Option<String>>,
    cur_owner: &BTreeMap<i64, Option<String>>,
    champion_roster_id: Option<i64>,
) -> Vec<PreseasonRow> {
    let current: BTreeSet<i64> = current_roster_ids.iter().copied().collect();
    let mut ordered: Vec<(i64, Option<&PowerRankRow>)> = prev_final_rankings
        .iter()
        .filter(|row| current.contains(&row.roster_id))
        .map(|row| (row.roster_id, Some(row)))
        .collect();
    let seen: BTreeSet<i64> = ordered.iter().map(|(rid, _)| *rid).collect();
    ordered.extend(current.difference(&seen).map(|rid| (*rid, None)));

    ordered
        .into_iter()
        .enumerate()
        .map(|(i, (rid, prev_row))| {
            let new_owner = match (prev_owner.get(&rid), cur_owner.get(&rid)) {
                (Some(prev), current) => prev.as_ref() != current.and_then(|c| c.as_ref()),
                (None, _) => false,
            };
            PreseasonRow {
                roster_id: rid,
                rank: i as i64 + 1,
                prev_rank: prev_row.map(|r| r.rank),
                prev_record: prev_row.map(|r| r.record.clone()),
                prev_pf: prev_row.and_then(|_| prev_pf_by_roster.get(&rid).copied()),
                new_owner,
                champion: champion_roster_id == Some(rid),
            }
        })
        .collect()
}

//! All-time head-to-head records between managers, keyed by Sleeper user id
//! so a record follows the person through roster changes.

use std::collections::BTreeMap;

use indexmap::IndexMap;

use crate::models::{HeadToHead, Manager, Matchup, Meeting, SeasonSummary};

/// The league's most one-sided and most even rivalries, and its longest
/// active winning streak. Only rivalries with `min_meetings` count.
pub struct Highlights<'a> {
    pub one_sided: Option<(&'a Manager, &'a HeadToHead)>,
    pub even: Option<(&'a Manager, &'a HeadToHead)>,
    pub streak: Option<(&'a Manager, &'a HeadToHead, i64)>,
}

pub fn build_rivalries(seasons: &[SeasonSummary]) -> Vec<Manager> {
    // owner -> (display name, (color, dark color)), from the latest season they
    // appear in.
    let mut profile: BTreeMap<String, (String, (String, String))> = BTreeMap::new();
    let mut ordered: Vec<&SeasonSummary> = seasons.iter().collect();
    ordered.sort_by(|a, b| b.season.cmp(&a.season));

    let mut meetings: IndexMap<String, IndexMap<String, Vec<Meeting>>> = IndexMap::new();
    for s in &ordered {
        for team in s.teams.values() {
            if let Some(owner) = &team.owner_id {
                profile.entry(owner.clone()).or_insert_with(|| {
                    let name = team
                        .owner_name
                        .clone()
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| team.name.clone());
                    (name, (team.color.clone(), team.color_dark.clone()))
                });
            }
        }
        for wk in &s.weeks {
            let mut pairs: IndexMap<i64, Vec<&Matchup>> = IndexMap::new();
            for m in &wk.matchups {
                pairs.entry(m.matchup_id).or_default().push(m);
            }
            for pair in pairs.values() {
                if pair.len() != 2 {
                    continue;
                }
                let (a, b) = (pair[0], pair[1]);
                let owner_a = s.teams.get(&a.roster_id).and_then(|t| t.owner_id.clone());
                let owner_b = s.teams.get(&b.roster_id).and_then(|t| t.owner_id.clone());
                let (Some(owner_a), Some(owner_b)) = (owner_a, owner_b) else {
                    continue;
                };
                if owner_a == owner_b {
                    continue;
                }
                meetings
                    .entry(owner_a.clone())
                    .or_default()
                    .entry(owner_b.clone())
                    .or_default()
                    .push(Meeting::new(
                        &s.season,
                        wk.week,
                        a.team_points,
                        b.team_points,
                    ));
                meetings
                    .entry(owner_b)
                    .or_default()
                    .entry(owner_a)
                    .or_default()
                    .push(Meeting::new(
                        &s.season,
                        wk.week,
                        b.team_points,
                        a.team_points,
                    ));
            }
        }
    }

    let mut managers: Vec<Manager> = Vec::new();
    for (owner, by_opponent) in &meetings {
        let mut rivals: Vec<HeadToHead> = by_opponent
            .iter()
            .map(|(opp, ms)| {
                let mut sorted = ms.clone();
                sorted
                    .sort_by(|a, b| (a.season.as_str(), a.week).cmp(&(b.season.as_str(), b.week)));
                HeadToHead::new(opp.clone(), sorted)
            })
            .collect();
        rivals.sort_by(|a, b| {
            b.meetings
                .len()
                .cmp(&a.meetings.len())
                .then(b.wins.cmp(&a.wins))
                .then_with(|| a.opponent_id.cmp(&b.opponent_id))
        });
        let (name, colors) = profile[owner].clone();
        let mut played: Vec<String> = rivals
            .iter()
            .flat_map(|h| h.meetings.iter().map(|m| m.season.clone()))
            .collect();
        played.sort();
        played.dedup();
        played.reverse();
        managers.push(Manager::new(owner.clone(), name, colors, played, rivals));
    }
    managers.sort_by(|a, b| {
        b.win_pct
            .partial_cmp(&a.win_pct)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.wins.cmp(&a.wins))
            .then_with(|| a.name.cmp(&b.name))
    });
    managers
}

pub fn rivalry_highlights(managers: &[Manager], min_meetings: usize) -> Highlights<'_> {
    let mut one_sided: Option<(&Manager, &HeadToHead, f64)> = None;
    let mut even: Option<(&Manager, &HeadToHead, i64)> = None;
    let mut streak: Option<(&Manager, &HeadToHead, i64)> = None;

    let latest = managers
        .iter()
        .flat_map(|m| m.seasons.iter())
        .max()
        .cloned();
    let current: Vec<&str> = managers
        .iter()
        .filter(|m| latest.as_ref().is_some_and(|l| m.seasons.contains(l)))
        .map(|m| m.owner_id.as_str())
        .collect();

    for manager in managers {
        for h in &manager.rivals {
            let games = h.meetings.len();
            if h.wins >= h.losses && games >= min_meetings {
                let share = (h.wins as f64 + 0.5 * h.ties as f64) / games as f64;
                let better = match one_sided {
                    None => true,
                    Some((_, cur, cur_share)) => (share, games) > (cur_share, cur.meetings.len()),
                };
                if better {
                    one_sided = Some((manager, h, share));
                }
                let gap = h.wins - h.losses;
                let better_even = match even {
                    None => true,
                    Some((_, cur, cur_gap)) => {
                        (-gap, games as i64) > (-cur_gap, cur.meetings.len() as i64)
                    }
                };
                if better_even {
                    even = Some((manager, h, gap));
                }
            }
            let (result, run) = &h.streak;
            let both_current = current.contains(&manager.owner_id.as_str())
                && current.contains(&h.opponent_id.as_str());
            if result == "W" && both_current && streak.is_none_or(|(_, _, best)| *run > best) {
                streak = Some((manager, h, *run));
            }
        }
    }

    Highlights {
        one_sided: one_sided.map(|(m, h, _)| (m, h)),
        even: even.map(|(m, h, _)| (m, h)),
        streak,
    }
}

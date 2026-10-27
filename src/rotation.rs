//! The rotation: which used shows form which slot, in what order, and how long each slot
//! lasts in turns. The build, the Balance view and the Group column all read slots from here.

use crate::build::Mode;
use crate::scan::ShowInfo;
use std::collections::{HashMap, HashSet};

/// One entry in the rotation: a single ungrouped show, or a group's used members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The group's position in the layout's group list, or `None` for an ungrouped show.
    pub group: Option<usize>,
    /// Member labels: the group's used members in group order, or the one show.
    pub members: Vec<String>,
}

/// The group `label` plays in: the first one listing it.
pub fn group_of(groups: &[Vec<String>], label: &str) -> Option<usize> {
    groups.iter().position(|g| g.iter().any(|l| l == label))
}

/// Slots for the `used` labels, in their order. A group's slot sits where its first used
/// member falls; group members that aren't used, or play in an earlier group, are left out.
pub fn slots<S: AsRef<str>>(used: &[S], groups: &[Vec<String>]) -> Vec<Slot> {
    let used_set: HashSet<&str> = used.iter().map(AsRef::as_ref).collect();
    let mut first: HashMap<&str, usize> = HashMap::new();
    for (i, g) in groups.iter().enumerate() {
        for l in g {
            first.entry(l.as_str()).or_insert(i);
        }
    }
    let mut placed = HashSet::new();
    let mut out = vec![];
    for l in used.iter().map(AsRef::as_ref) {
        match first.get(l) {
            Some(&g) if !placed.insert(g) => {}
            Some(&g) => {
                let mut seen = HashSet::new();
                let members = groups[g]
                    .iter()
                    .filter(|m| used_set.contains(m.as_str()) && first[m.as_str()] == g)
                    .filter(|m| seen.insert(m.as_str()))
                    .cloned()
                    .collect();
                out.push(Slot {
                    group: Some(g),
                    members,
                });
            }
            None => out.push(Slot {
                group: None,
                members: vec![l.to_string()],
            }),
        }
    }
    out
}

/// What one show adds to its slot's size: seasons in seasons mode, else episodes.
pub fn size(show: &ShowInfo, mode: Mode) -> usize {
    match mode {
        Mode::Seasons => show.n_seasons,
        _ => show.n_episodes,
    }
}

/// Turns a slot of total `size` lasts. In episodes mode a group's members play as one list
/// cut into blocks, so this is taken over the whole slot, not per member.
pub fn turns(size: usize, mode: Mode, block_size: usize) -> usize {
    match mode {
        Mode::Episodes => size.div_ceil(block_size.max(1)),
        _ => size,
    }
}

/// Auto-sort: pack `(label, size)` shows into new groups lasting at most `target` turns.
/// Largest first (ties by label), each into the first group it fits, else a new one.
/// Groups left with a single show are dropped, since a group needs two members.
pub fn auto_sort(
    shows: &[(String, usize)],
    target: usize,
    mode: Mode,
    block_size: usize,
) -> Vec<Vec<String>> {
    let mut order: Vec<&(String, usize)> = shows.iter().collect();
    // Ties use the show list's alphabetical order (lowercase), then exact label.
    order.sort_by(|a, b| {
        let key = |l: &String| (l.to_lowercase(), l.clone());
        b.1.cmp(&a.1).then_with(|| key(&a.0).cmp(&key(&b.0)))
    });
    let mut bins: Vec<(usize, Vec<String>)> = vec![];
    for (label, h) in order {
        match bins
            .iter_mut()
            .find(|(total, _)| turns(total + h, mode, block_size) <= target)
        {
            Some((total, g)) => {
                *total += h;
                g.push(label.clone());
            }
            None => bins.push((*h, vec![label.clone()])),
        }
    }
    bins.into_iter()
        .map(|(_, g)| g)
        .filter(|g| g.len() >= 2)
        .collect()
}

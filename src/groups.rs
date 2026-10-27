//! Editing the layout's groups, shared by the text menu and GUI. Every edit keeps groups
//! clean: no empty groups, and each show in at most one group.

use std::collections::HashSet;

/// The group `label` plays in: the first one listing it.
pub fn group_of(groups: &[Vec<String>], label: &str) -> Option<usize> {
    groups.iter().position(|g| g.iter().any(|l| l == label))
}

/// Clean groups read from a layout or left by a rescan: keep only labels in `known`, keep
/// each show only in the first group listing it (once), and drop groups that end up empty.
pub fn retain_known(groups: &[Vec<String>], known: &HashSet<String>) -> Vec<Vec<String>> {
    let mut seen = HashSet::new();
    groups
        .iter()
        .map(|g| {
            g.iter()
                .filter(|l| known.contains(*l) && seen.insert(l.as_str()))
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|g| !g.is_empty())
        .collect()
}

/// Remove `labels` from every group, dropping groups that end up empty.
pub fn remove(groups: &mut Vec<Vec<String>>, labels: &[String]) {
    for g in groups.iter_mut() {
        g.retain(|l| !labels.contains(l));
    }
    groups.retain(|g| !g.is_empty());
}

/// New group from `labels` (taken out of any other group first), appended at the end.
pub fn create(groups: &mut Vec<Vec<String>>, labels: &[String]) {
    remove(groups, labels);
    groups.push(labels.to_vec());
}

/// Move `labels` into group `g_idx`, numbered as the groups were before the move. The target
/// keeps its position unless the move empties it, in which case it goes to the end; an
/// out-of-range index makes a new group.
pub fn add(groups: &mut Vec<Vec<String>>, g_idx: usize, labels: &[String]) {
    if g_idx >= groups.len() {
        create(groups, labels);
        return;
    }
    for g in groups.iter_mut() {
        g.retain(|l| !labels.contains(l));
    }
    let mut target = groups[g_idx].clone();
    let keep_position = !target.is_empty();
    for l in labels {
        if !target.contains(l) {
            target.push(l.clone());
        }
    }
    if keep_position {
        groups[g_idx] = target;
        groups.retain(|g| !g.is_empty());
    } else {
        groups.retain(|g| !g.is_empty());
        groups.push(target);
    }
}

pub fn dissolve(groups: &mut Vec<Vec<String>>, g_idx: usize) {
    if g_idx < groups.len() {
        groups.remove(g_idx);
    }
}

/// `Group N (a, b, c...)` labels for group pickers: at most 3 names, then `...`.
pub fn choices(groups: &[Vec<String>]) -> Vec<String> {
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let names = g.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let more = if g.len() > 3 { "..." } else { "" };
            format!("Group {} ({names}{more})", i + 1)
        })
        .collect()
}

use playlistgenrs::groups::*;
use std::collections::HashSet;

fn sv(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn gv(g: &[&[&str]]) -> Vec<Vec<String>> {
    g.iter().map(|g| sv(g)).collect()
}

#[test]
fn edits() {
    let mut g = gv(&[&["A", "B"], &["C", "D"]]);
    create(&mut g, &sv(&["B", "C", "D"]));
    assert_eq!(g, gv(&[&["A"], &["B", "C", "D"]]));

    add(&mut g, 1, &sv(&["A"]));
    assert_eq!(g, gv(&[&["B", "C", "D", "A"]]));
    add(&mut g, 5, &sv(&["E"]));
    assert_eq!(g, gv(&[&["B", "C", "D", "A"], &["E"]]));

    remove(&mut g, &sv(&["E", "B"]));
    assert_eq!(g, gv(&[&["C", "D", "A"]]));
    dissolve(&mut g, 3);
    dissolve(&mut g, 0);
    assert!(g.is_empty());
}

#[test]
fn add_numbers_groups_as_they_were() {
    // Target keeps its position even when an earlier group empties.
    let mut g = gv(&[&["A"], &["X"], &["Y"]]);
    add(&mut g, 1, &sv(&["A"]));
    assert_eq!(g, gv(&[&["X", "A"], &["Y"]]));

    // Target emptied by the move goes to the end.
    let mut g = gv(&[&["A"], &["B", "C"]]);
    add(&mut g, 0, &sv(&["A", "B"]));
    assert_eq!(g, gv(&[&["C"], &["A", "B"]]));
}

#[test]
fn retain_known_cleans_loaded_groups() {
    let known: HashSet<String> = sv(&["A", "B", "C"]).into_iter().collect();
    // Unknown labels go, B stays only in its first group, repeats collapse, and groups
    // left empty are dropped.
    let g = gv(&[&["X"], &["B", "A", "B"], &["B", "C", "C"], &["A"]]);
    assert_eq!(retain_known(&g, &known), gv(&[&["B", "A"], &["C"]]));
}

#[test]
fn group_of_is_first_listing() {
    let g = gv(&[&["B", "C"], &["B", "A"]]);
    assert_eq!(group_of(&g, "B"), Some(0));
    assert_eq!(group_of(&g, "A"), Some(1));
    assert_eq!(group_of(&g, "Z"), None);
}

#[test]
fn choice_labels() {
    let g = gv(&[&["a", "b"], &["a", "b", "c", "d"]]);
    assert_eq!(choices(&g), ["Group 1 (a, b)", "Group 2 (a, b, c...)"]);
}

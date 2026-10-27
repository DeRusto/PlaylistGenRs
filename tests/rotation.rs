use playlistgenrs::build::Mode;
use playlistgenrs::rotation::*;

fn sv(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn gv(g: &[&[&str]]) -> Vec<Vec<String>> {
    g.iter().map(|g| sv(g)).collect()
}

fn hv(v: &[(&str, usize)]) -> Vec<(String, usize)> {
    v.iter().map(|&(l, h)| (l.to_string(), h)).collect()
}

fn slot(group: Option<usize>, members: &[&str]) -> Slot {
    Slot {
        group,
        members: sv(members),
    }
}

#[test]
fn slots_follow_used_order() {
    // The group sits where its first used member (B) falls, members in group order.
    let s = slots(&["A", "B", "C", "D"], &gv(&[&["C", "B"]]));
    assert_eq!(
        s,
        [
            slot(None, &["A"]),
            slot(Some(0), &["C", "B"]),
            slot(None, &["D"])
        ]
    );
}

#[test]
fn slots_keep_layout_numbering_and_drop_unused_members() {
    // Group 0 has no used members, so it has no slot, and group 1 keeps its number.
    let s = slots(&["B", "C"], &gv(&[&["X"], &["B", "Y", "C"]]));
    assert_eq!(s, [slot(Some(1), &["B", "C"])]);
}

#[test]
fn show_in_two_groups_plays_in_the_first() {
    let g = gv(&[&["B", "C"], &["B", "A"], &["C", "C"]]);
    // B only plays in group 0; group 1 is left with A; group 2 has nothing of its own.
    let s = slots(&["A", "B", "C"], &g);
    assert_eq!(s, [slot(Some(1), &["A"]), slot(Some(0), &["B", "C"])]);
    // Repeats within a group play once.
    let s = slots(&["C"], &gv(&[&["C", "C"]]));
    assert_eq!(s, [slot(Some(0), &["C"])]);
}

#[test]
fn turns_by_mode() {
    assert_eq!(turns(7, Mode::None, 3), 7);
    assert_eq!(turns(7, Mode::Seasons, 3), 7);
    assert_eq!(turns(7, Mode::Episodes, 3), 3);
    assert_eq!(turns(6, Mode::Episodes, 3), 2);
    assert_eq!(turns(0, Mode::Episodes, 3), 0);
}

#[test]
fn auto_sort_first_fit_largest_first() {
    // One long show fills its group alone (dropped); the short ones share one, and the
    // leftover that fits nowhere is alone (dropped).
    let a = |shows: &[(&str, usize)], target| auto_sort(&hv(shows), target, Mode::None, 10);
    let shows = [("d", 2), ("c", 3), ("Long", 10), ("b", 3), ("a", 3)];
    assert_eq!(a(&shows, 10), gv(&[&["a", "b", "c"]]));

    // Nothing fits together.
    assert!(a(&[("X", 6), ("Y", 5)], 10).is_empty());

    // Equal sizes tie-break alphabetically; exactly reaching the target is allowed.
    assert_eq!(a(&[("C", 5), ("B", 5), ("A", 5)], 10), gv(&[&["A", "B"]]));
    // ...ignoring case, like the show list.
    let shows = [("Beta", 5), ("alpha", 5), ("Gamma", 5)];
    assert_eq!(a(&shows, 10), gv(&[&["alpha", "Beta"]]));

    // Later shows fill earlier groups first.
    let shows = [("A", 6), ("B", 5), ("C", 4), ("D", 4), ("E", 1)];
    assert_eq!(a(&shows, 10), gv(&[&["A", "C"], &["B", "D", "E"]]));
}

#[test]
fn auto_sort_measures_a_group_as_a_whole() {
    // Block size 4, target 2 turns. A (5 eps, 2 turns) + B (3 eps, 1 turn) is 8 episodes:
    // 2 turns together, though their own turns add up to 3. C no longer fits.
    let shows = hv(&[("A", 5), ("B", 3), ("C", 3)]);
    assert_eq!(auto_sort(&shows, 2, Mode::Episodes, 4), gv(&[&["A", "B"]]));
}

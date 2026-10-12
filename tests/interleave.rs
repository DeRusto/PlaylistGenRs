//! Ported from test_playlist_gen.py plus the README examples.

use playlistgenrs::interleave::{interleave, interleave_by_season, interleave_in_blocks};

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|x| x.to_string()).collect()
}

#[test]
fn equal_length() {
    assert_eq!(
        interleave(&[vec![1, 2, 3], vec![4, 5, 6]], false),
        [1, 4, 2, 5, 3, 6]
    );
}

#[test]
fn unequal_length() {
    assert_eq!(
        interleave(&[vec![1, 2, 3, 4, 5], vec![6, 7]], false),
        [1, 6, 2, 7, 3, 4, 5]
    );
}

#[test]
fn three_shows() {
    let l = [
        s(&["a1", "a2", "a3"]),
        s(&["b1", "b2"]),
        s(&["c1", "c2", "c3", "c4"]),
    ];
    assert_eq!(
        interleave(&l, false),
        s(&["a1", "b1", "c1", "a2", "b2", "c2", "a3", "c3", "c4"])
    );
}

#[test]
fn single_show() {
    assert_eq!(interleave(&[s(&["x1", "x2"])], false), s(&["x1", "x2"]));
    assert_eq!(interleave(&[s(&["x1", "x2"])], true), s(&["x1", "x2"]));
}

#[test]
fn empty() {
    assert!(interleave::<i32>(&[], false).is_empty());
    assert!(interleave_in_blocks::<i32>(&[], 3, false).is_empty());
    assert!(interleave_by_season::<i32>(&[], false).is_empty());
}

#[test]
fn repeat_unequal_length() {
    assert_eq!(
        interleave(&[vec![1, 2, 3], vec![4, 5]], true),
        [1, 4, 2, 5, 3, 4]
    );
}

#[test]
fn repeat_equal_length() {
    let l = [vec![1, 2], vec![3, 4]];
    assert_eq!(interleave(&l, true), interleave(&l, false));
}

#[test]
fn repeat_three_shows() {
    let l = [
        s(&["a1", "a2", "a3"]),
        s(&["b1", "b2"]),
        s(&["c1", "c2", "c3", "c4"]),
    ];
    assert_eq!(
        interleave(&l, true),
        s(&[
            "a1", "b1", "c1", "a2", "b2", "c2", "a3", "b1", "c3", "a1", "b2", "c4"
        ])
    );
}

#[test]
fn blocks_basic() {
    assert_eq!(
        interleave_in_blocks(&[vec![1, 2, 3, 4, 5], vec![6, 7, 8]], 2, false),
        [1, 2, 6, 7, 3, 4, 8, 5]
    );
}

#[test]
fn blocks_equal_length() {
    assert_eq!(
        interleave_in_blocks(&[vec![1, 2, 3, 4], vec![5, 6, 7, 8]], 2, false),
        [1, 2, 5, 6, 3, 4, 7, 8]
    );
}

#[test]
fn blocks_larger_than_list() {
    assert_eq!(
        interleave_in_blocks(&[vec![1, 2], vec![3, 4, 5, 6]], 5, false),
        [1, 2, 3, 4, 5, 6]
    );
}

#[test]
fn blocks_repeat() {
    assert_eq!(
        interleave_in_blocks(&[vec![1, 2, 3], vec![4, 5]], 2, true),
        [1, 2, 4, 5, 3, 4]
    );
}

#[test]
fn season_total_count() {
    let sa = vec![s(&["a01", "a02"]), s(&["a03"])];
    let sb = vec![s(&["b01"]), s(&["b02", "b03"])];
    assert_eq!(
        interleave_by_season(&[sa, sb], false),
        s(&["a01", "a02", "b01", "a03", "b02", "b03"])
    );
}

#[test]
fn season_shorter_show_skipped() {
    let sa = vec![s(&["a01"]), s(&["a02"])];
    let sb = vec![s(&["b01"])];
    assert_eq!(
        interleave_by_season(&[sa, sb], false),
        s(&["a01", "b01", "a02"])
    );
}

#[test]
fn season_repeat() {
    let sa = vec![s(&["a01"]), s(&["a02"])];
    let sb = vec![s(&["b01"])];
    assert_eq!(
        interleave_by_season(&[sa, sb], true),
        s(&["a01", "b01", "a02", "b01"])
    );
}

#[test]
fn empty_sublist_repeat() {
    assert_eq!(
        interleave(&[s(&["a1", "a2"]), vec![]], true),
        s(&["a1", "a2"])
    );
    assert_eq!(
        interleave_in_blocks(&[s(&["a1", "a2", "a3"]), vec![]], 2, true),
        s(&["a1", "a2", "a3"])
    );
    assert_eq!(
        interleave_by_season(&[vec![s(&["a01"])], vec![]], true),
        s(&["a01"])
    );
}

// README examples

#[test]
fn readme_round_robin() {
    let a = s(&["a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "a9"]);
    let b = s(&["b1", "b2", "b3", "b4", "b5", "b6", "b7"]);
    assert_eq!(
        interleave(&[a, b], false).join(" "),
        "a1 b1 a2 b2 a3 b3 a4 b4 a5 b5 a6 b6 a7 b7 a8 a9"
    );
}

#[test]
fn readme_default_and_repeat() {
    let l = [s(&["a1", "a2", "a3", "a4"]), s(&["b1", "b2"])];
    assert_eq!(interleave(&l, false).join(" "), "a1 b1 a2 b2 a3 a4");
    assert_eq!(interleave(&l, true).join(" "), "a1 b1 a2 b2 a3 b1 a4 b2");
}

#[test]
fn readme_block_two() {
    let l = [s(&["a1", "a2", "a3", "a4", "a5"]), s(&["b1", "b2", "b3"])];
    assert_eq!(
        interleave_in_blocks(&l, 2, false).join(" "),
        "a1 a2 b1 b2 a3 a4 b3 a5"
    );
}

#[test]
fn readme_seasons() {
    let sa = vec![s(&["a01", "a02"]), s(&["a03"])];
    let sb = vec![s(&["b01"]), s(&["b02", "b03"])];
    assert_eq!(
        interleave_by_season(&[sa, sb], false).join(" "),
        "a01 a02 b01 a03 b02 b03"
    );
}

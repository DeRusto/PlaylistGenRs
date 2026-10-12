use playlistgenrs::build::{Mode, ShowOrder};
use playlistgenrs::layout::*;
use playlistgenrs::scan::ShowInfo;
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

fn sv(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn gv(g: &[&[&str]]) -> Vec<Vec<String>> {
    g.iter().map(|g| sv(g)).collect()
}

fn sample() -> State {
    State {
        dirs: sv(&["/tmp/test_dir"]),
        selected: sv(&["ShowA"]),
        groups: gv(&[&["ShowA", "ShowB"]]),
        relative: true,
        interleave_mode: Mode::Episodes,
        block_size: 5,
        ..Default::default()
    }
}

#[test]
fn save_load_delete_round_trip() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("sub/layouts.json");
    let mut store = LayoutStore::new(f.clone());
    assert!(store.layouts.is_empty() && store.active_layout_name.is_none());

    store.save_layout("Default Layout", &sample()).unwrap();
    assert_eq!(store.active_layout_name.as_deref(), Some("Default Layout"));
    assert_eq!(store.get_active_state(), Some(sample()));
    assert!(!t.path().join("sub/layouts.tmp").exists());

    let mut store = LayoutStore::new(f.clone());
    assert_eq!(store.get_layout("Default Layout"), Some(sample()));
    let raw = fs::read_to_string(&f).unwrap();
    assert!(
        raw.contains("\n  \"layouts\": {") && raw.contains("\"block_size\": \"5\""),
        "{raw}"
    );

    assert!(store.delete_layout("Default Layout").unwrap());
    assert!(store.layouts.is_empty() && store.active_layout_name.is_none());
    assert!(!store.delete_layout("nope").unwrap());
}

#[test]
fn reads_python_written_file() {
    let store = LayoutStore::new("tests/data/python_layouts.json".into());
    assert!(store.warnings.is_empty());
    let names: Vec<_> = store.layouts.keys().cloned().collect();
    assert_eq!(names, ["Évening Mix", LAST_SESSION]);
    let s = store.get_active_state().unwrap();
    assert_eq!(s.dirs, ["/media/tv", "/media/tv2"]);
    assert_eq!(s.groups, gv(&[&["ShowA", "ShowB"]]));
    assert_eq!(s.output_path, "/tmp/out.m3u");
    assert!(s.relative && !s.repeat);
    assert_eq!(
        (s.show_order, s.interleave_mode, s.block_size),
        (ShowOrder::Random, Mode::Episodes, 5)
    );
    assert_eq!(store.get_layout(LAST_SESSION), Some(State::default()));
}

#[test]
fn block_size_lenient() {
    for (raw, want) in [
        (r#""7""#, 7),
        ("7", 7),
        (r#"" 8 ""#, 8),
        (r#""""#, 10),
        (r#""x""#, 10),
        (r#""0""#, 10),
        ("-3", 10),
        ("2.5", 10),
        ("null", 10),
    ] {
        let s: State = serde_json::from_str(&format!(r#"{{"block_size": {raw}}}"#)).unwrap();
        assert_eq!(s.block_size, want, "{raw}");
    }
    let s: State = serde_json::from_str("{}").unwrap();
    assert_eq!(s, State::default());
}

#[test]
fn corrupt_file_warns_and_is_not_clobbered_by_autosave() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("layouts.json");
    for bad in ["{not json", "[1, 2]"] {
        fs::write(&f, bad).unwrap();
        let mut store = LayoutStore::new(f.clone());
        assert_eq!(store.warnings.len(), 1);
        assert!(store.warnings[0].starts_with(&format!(
            "Warning: Failed to load layouts from {}: ",
            f.display()
        )));
        assert!(store.layouts.is_empty());
        store.autosave(&sample()).unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), bad);
        // An explicit save does overwrite.
        store.save_layout("Mine", &sample()).unwrap();
        assert_eq!(
            LayoutStore::new(f.clone()).get_layout("Mine"),
            Some(sample())
        );
    }
}

#[test]
fn autosave_targets_active_or_session_slot() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("l.json");
    let mut store = LayoutStore::new(f.clone());
    store.autosave(&sample()).unwrap();
    assert_eq!(
        LayoutStore::new(f.clone()).get_layout(LAST_SESSION),
        Some(sample())
    );
    assert_eq!(store.active_layout_name, None);

    store.save_layout("A", &State::default()).unwrap();
    store.autosave(&sample()).unwrap();
    assert_eq!(store.get_layout("A"), Some(sample()));
}

#[test]
fn delete_active_picks_first_remaining() {
    let t = TempDir::new().unwrap();
    let mut store = LayoutStore::new(t.path().join("l.json"));
    store.save_layout("A", &State::default()).unwrap();
    store.save_layout("B", &State::default()).unwrap();
    store.delete_layout("B").unwrap();
    assert_eq!(store.active_layout_name.as_deref(), Some("A"));
}

#[test]
fn startup_fallbacks() {
    let t = TempDir::new().unwrap();
    let mut store = LayoutStore::new(t.path().join("l.json"));
    assert_eq!(store.startup_state(true), None);
    store
        .layouts
        .insert("First".into(), serde_json::to_value(sample()).unwrap());
    store.active_layout_name = None;
    assert_eq!(store.startup_state(false), None);
    assert_eq!(store.startup_state(true), Some(sample()));
    store.layouts.insert(
        LAST_SESSION.into(),
        serde_json::to_value(State::default()).unwrap(),
    );
    assert_eq!(store.startup_state(false), Some(State::default()));
    store.active_layout_name = Some("First".into());
    assert_eq!(store.startup_state(false), Some(sample()));
}

#[test]
fn unknown_fields_survive_save() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("l.json");
    fs::write(
        &f,
        r#"{"layouts": {"A": {"future": 1}}, "active_layout_name": "A"}"#,
    )
    .unwrap();
    let mut store = LayoutStore::new(f.clone());
    store.autosave(&sample()).unwrap();
    let raw: serde_json::Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
    assert_eq!(raw["layouts"]["A"]["future"], 1);
    assert_eq!(raw["layouts"]["A"]["block_size"], "5");
}

fn info(labels: &[&str]) -> Vec<ShowInfo> {
    labels
        .iter()
        .map(|l| ShowInfo {
            label: l.to_string(),
            paths: vec![],
            n_seasons: 1,
            n_episodes: 1,
        })
        .collect()
}

#[test]
fn rescan_keeps_selection_and_autoselects_new() {
    let old: HashSet<String> = sv(&["A", "B", "C"]).into_iter().collect();
    let mut selected: HashSet<String> = sv(&["A", "C"]).into_iter().collect();
    let mut groups = gv(&[&["A", "C"], &["C"], &["B"]]);
    rescan(&old, &info(&["A", "B", "D"]), &mut selected, &mut groups);
    assert_eq!(selected, sv(&["A", "D"]).into_iter().collect());
    assert_eq!(groups, gv(&[&["A"], &["B"]]));
}

#[test]
fn group_helpers() {
    let mut g = gv(&[&["A", "B"], &["C", "D"]]);
    create_group(&mut g, &sv(&["B", "C", "D"]));
    assert_eq!(g, gv(&[&["A"], &["B", "C", "D"]]));

    // Text menu: index taken after removal, so "A" leaving group 1 makes "group 2" the old group 3.
    let mut t = gv(&[&["A"], &["X"], &["Y"]]);
    add_to_group(&mut t, 1, "A");
    assert_eq!(t, gv(&[&["X"], &["Y", "A"]]));
    add_to_group(&mut g, 0, "A");
    assert_eq!(g, gv(&[&["B", "C", "D", "A"]]));
    add_to_group(&mut g, 5, "E");
    assert_eq!(g, gv(&[&["B", "C", "D", "A"], &["E"]]));

    remove_from_groups(&mut g, &sv(&["E", "B"]));
    assert_eq!(g, gv(&[&["C", "D", "A"]]));
    dissolve_group(&mut g, 3);
    dissolve_group(&mut g, 0);
    assert!(g.is_empty());
}

#[test]
fn gui_add_tracks_target_identity() {
    // Target keeps its position even when an earlier group empties.
    let mut g = gv(&[&["A"], &["B", "C"]]);
    gui_add_to_group(&mut g, 1, &sv(&["A"]));
    assert_eq!(g, gv(&[&["B", "C", "A"]]));

    // Target emptied by the move goes to the end.
    let mut g = gv(&[&["A"], &["B", "C"]]);
    gui_add_to_group(&mut g, 0, &sv(&["A", "B"]));
    assert_eq!(g, gv(&[&["C"], &["A", "B"]]));
}

#[test]
fn group_choice_labels() {
    let g = gv(&[&["a", "b"], &["a", "b", "c", "d"]]);
    assert_eq!(
        group_choices(&g),
        ["Group 1 (a, b)", "Group 2 (a, b, c...)"]
    );
}

#[test]
fn selected_list_in_show_order() {
    let sel: HashSet<String> = sv(&["Z", "B", "A"]).into_iter().collect();
    assert_eq!(selected_list(&sel, &info(&["A", "B"])), ["A", "B", "Z"]);
}

#[test]
fn unreadable_field_keeps_rest_of_layout() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("l.json");
    fs::write(
        &f,
        r#"{"layouts": {"A": {"dirs": ["/tv"], "show_order": "bogus", "relative": true}}, "active_layout_name": "A"}"#,
    )
    .unwrap();
    let s = LayoutStore::new(f).get_active_state().unwrap();
    assert_eq!(
        (s.dirs, s.show_order, s.relative),
        (sv(&["/tv"]), ShowOrder::Alpha, true)
    );
}

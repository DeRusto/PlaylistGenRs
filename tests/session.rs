mod fixtures;

use fixtures::tree;
use playlistgenrs::build::{Mode, ShowOrder};
use playlistgenrs::layout::{LAST_SESSION, LayoutStore, State};
use playlistgenrs::session::{DirError, Session};
use std::collections::HashSet;
use std::fs;
use tempfile::TempDir;

/// A temp dir with `tv/A`, `tv/B`, `tv/C` and a session on `l.json` in it.
fn setup() -> (TempDir, Session) {
    let t = TempDir::new().unwrap();
    tree(
        &t.path().join("tv"),
        &[("A", &["a.mkv"]), ("B", &["b.mkv"]), ("C", &["c.mkv"])],
    );
    let s = Session::open(LayoutStore::new(t.path().join("l.json")));
    (t, s)
}

fn set(v: &[&str]) -> HashSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn apply_and_extract_state() {
    let (t, mut s) = setup();
    let dir = t.path().join("tv").to_string_lossy().into_owned();
    let state = State {
        dirs: vec![dir.clone(), String::new()],
        selected: vec!["B".into(), "Gone".into()],
        groups: vec![vec!["B".into(), "Gone".into()]],
        output_path: "out.m3u".into(),
        relative: true,
        repeat: true,
        show_order: ShowOrder::Random,
        interleave_mode: Mode::Episodes,
        block_size: 7,
    };
    s.apply(&state);
    let cur = s.state();
    // Empty directories are dropped, and so are shows that are no longer on disk.
    assert_eq!(cur.dirs, [dir]);
    assert_eq!(cur.selected, ["B"]);
    assert_eq!(cur.groups, [["B"]]);
    assert_eq!(
        (cur.output_path.as_str(), cur.relative, cur.block_size),
        ("out.m3u", true, 7)
    );

    // Saved layouts are snapshots.
    s.save_as("Saved").unwrap();
    s.groups.push(vec!["X".into()]);
    assert_eq!(s.store.get_layout("Saved").unwrap().groups, [["B"]]);
}

#[test]
fn opens_last_session_but_not_first_layout() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("l.json");
    let mut store = LayoutStore::new(f.clone());
    let other = State {
        repeat: true,
        ..Default::default()
    };
    store
        .layouts
        .insert("Other".into(), serde_json::to_value(other).unwrap());
    store.save_all().unwrap();
    assert!(!Session::open(LayoutStore::new(f.clone())).repeat);

    let mut store = LayoutStore::new(f.clone());
    let last = State {
        output_path: "/tmp/r.m3u".into(),
        block_size: 12,
        ..Default::default()
    };
    store.autosave_session(&last).unwrap();
    let s = Session::open(LayoutStore::new(f));
    assert_eq!(s.store.active_layout_name, None);
    assert_eq!((s.output_path.as_str(), s.block_size), ("/tmp/r.m3u", 12));
}

#[test]
fn load_warnings_are_kept_for_the_frontend() {
    let t = TempDir::new().unwrap();
    let f = t.path().join("l.json");
    fs::write(&f, "not json").unwrap();
    let s = Session::open(LayoutStore::new(f));
    assert_eq!(s.warnings.len(), 1);
    assert!(s.warnings[0].starts_with("Warning: Failed to load layouts"));
}

#[test]
fn add_and_remove_directories() {
    let (t, mut s) = setup();
    let tv = t.path().join("tv");
    let added = s.add_dir(&tv).unwrap();
    assert_eq!(added, tv.canonicalize().unwrap());
    assert_eq!(s.selected, set(&["A", "B", "C"]), "new shows are used");

    // The same directory by another spelling, and something that isn't a directory.
    assert_eq!(
        s.add_dir(&tv.join("../tv")),
        Err(DirError::AlreadyAdded(added.clone()))
    );
    let file = tv.join("A/a.mkv");
    assert_eq!(s.add_dir(&file), Err(DirError::NotADirectory(file)));
    assert_eq!(s.dirs.len(), 1);

    assert_eq!(s.remove_dir(5), None);
    assert_eq!(s.remove_dir(0), Some(added));
    assert!(s.show_info.is_empty() && s.selected.is_empty());
}

#[test]
fn rescan_keeps_selection_and_uses_new_shows() {
    let (t, mut s) = setup();
    let tv = t.path().join("tv");
    s.add_dir(&tv).unwrap();
    s.selected = set(&["A", "C"]);
    s.groups = vec![vec!["A".into(), "C".into()], vec!["B".into()]];

    fs::remove_dir_all(tv.join("C")).unwrap();
    tree(&tv, &[("D", &["d.mkv"])]);
    s.rescan();
    assert_eq!(s.selected, set(&["A", "D"]));
    assert_eq!(s.groups, [vec!["A"], vec!["B"]]);
}

#[test]
fn output_and_build_options() {
    let (t, mut s) = setup();
    assert_eq!(s.output(), None);
    let tv = s.add_dir(&t.path().join("tv")).unwrap();
    assert_eq!(s.output(), Some(tv.join("interleaved.m3u")));
    s.output_path = format!(" {} ", t.path().join("x.m3u").display());
    assert_eq!(
        s.output(),
        Some(t.path().canonicalize().unwrap().join("x.m3u"))
    );

    s.selected.remove("B");
    s.groups = vec![vec!["A".into(), "C".into()]];
    s.block_size = 3;
    let o = s.build_options();
    assert_eq!(o.include, Some(set(&["A", "C"])));
    assert_eq!((o.groups, o.block_size), (s.groups.clone(), 3));
}

#[test]
fn layout_actions() {
    let (t, mut s) = setup();
    s.add_dir(&t.path().join("tv")).unwrap();
    s.save_as("Mine").unwrap();
    assert_eq!(s.store.active_layout_name.as_deref(), Some("Mine"));

    s.new_layout();
    assert!(s.dirs.is_empty() && s.show_info.is_empty());
    assert_eq!(s.store.active_layout_name, None);

    assert!(!s.load("Missing"));
    assert!(s.load("Mine"));
    assert_eq!(s.show_info.len(), 3);

    // Autosave went to the session slot while unnamed; deleting "Mine" makes it active.
    s.autosave();
    s.load("Mine");
    s.store.autosave_session(&State::default()).unwrap();
    s.delete("Mine");
    assert_eq!(s.store.active_layout_name.as_deref(), Some(LAST_SESSION));
    s.delete(LAST_SESSION);
    assert_eq!(s.store.active_layout_name, None);
    assert!(s.dirs.is_empty());
}

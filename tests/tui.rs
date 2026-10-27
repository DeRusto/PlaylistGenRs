mod fixtures;

use fixtures::{playlist_names, tree};
use playlistgenrs::build::{Mode, ShowOrder};
use playlistgenrs::layout::{LAST_SESSION, LayoutStore, State};
use playlistgenrs::tui::run_menu_with;
use tempfile::TempDir;

fn drive(store_file: &std::path::Path, script: &str) -> String {
    let mut out = Vec::new();
    let code = run_menu_with(
        LayoutStore::new(store_file.to_path_buf()),
        &mut script.as_bytes(),
        &mut out,
    );
    assert_eq!(code, 0);
    String::from_utf8(out).unwrap()
}

#[test]
fn add_dir_generate_quit() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("tv");
    tree(
        &tv,
        &[("ShowA", &["a01.mkv", "a02.mkv"]), ("ShowB", &["b01.mkv"])],
    );
    let lf = t.path().join("layouts.json");
    let screen = drive(&lf, &format!("a\n{}\ng\n\nq\n", tv.display()));

    let tv = tv.canonicalize().unwrap();
    let m3u = tv.join("interleaved.m3u");
    assert_eq!(playlist_names(&m3u), ["a01.mkv", "b01.mkv", "a02.mkv"]);
    assert!(
        screen.contains("  Shows       : 2 found, 2 selected\n"),
        "{screen}"
    );
    assert!(screen.contains("  [g] Generate playlist\n"));
    assert!(screen.contains("Found 2 slot(s):\n  ShowA: 2 episode(s)\n  ShowB: 1 episode(s)\n"));
    assert!(screen.contains(&format!(
        "\nPlaylist written: {}  (3 entries)\nPress Enter...",
        m3u.display()
    )));

    // State was autosaved to the session slot.
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!(s.dirs, [tv.to_string_lossy()]);
    assert_eq!(s.selected, ["ShowA", "ShowB"]);
}

#[test]
fn first_screen_matches_python() {
    let t = TempDir::new().unwrap();
    let screen = drive(&t.path().join("l.json"), "q\n");
    let rule = "=".repeat(54);
    assert_eq!(
        screen,
        format!(
            "\x1b[2J\x1b[H{rule}\n         TV Playlist Interleaver\n{rule}\n\nSource directories:\n  (none — add at least one with [a])\n\n\
Settings:\n  Shows       : none\n  Groups      : none\n  Output      : (auto: interleaved.m3u in first source dir)\n  Path style  : absolute\n  Repeat      : off\n  Show order  : alphabetical\n  Mode        : 1 episode per show\n\n\
Commands:\n  [a] Add source directory\n  [o] Set output file\n  [p] Toggle path style  (absolute / relative)\n  [r] Toggle repeat shorter shows\n  [n] Toggle show order  (alphabetical / random)\n  [s] Set interleave mode\n  [q] Quit\n\nChoice: "
        )
    );
}

#[test]
fn state_persists_and_settings_toggle() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("media");
    tree(&tv, &[("ShowA", &["ep1.mkv"])]);
    let lf = t.path().join("l.json");
    let state = State {
        dirs: vec![tv.to_string_lossy().into()],
        selected: vec!["ShowA".into()],
        groups: vec![vec!["ShowA".into()]],
        output_path: t.path().join("tui_out.m3u").to_string_lossy().into(),
        relative: true,
        repeat: true,
        show_order: ShowOrder::Random,
        interleave_mode: Mode::Seasons,
        block_size: 5,
    };
    LayoutStore::new(lf.clone())
        .save_layout(LAST_SESSION, &state)
        .unwrap();

    drive(&lf, "q\n");
    assert_eq!(
        LayoutStore::new(lf.clone()).get_active_state(),
        Some(state.clone())
    );

    // p, r, n toggle; s → 2 → 7 sets block mode.
    let screen = drive(&lf, "p\nr\nn\ns\n2\n7\nq\n");
    assert!(screen.contains("  Mode        : 7 episodes per show\n"));
    let s = LayoutStore::new(lf).get_active_state().unwrap();
    assert_eq!(
        (
            s.relative,
            s.repeat,
            s.show_order,
            s.interleave_mode,
            s.block_size
        ),
        (false, false, ShowOrder::Alpha, Mode::Episodes, 7)
    );
}

#[test]
fn selection_and_groups_screens() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("tv");
    tree(
        &tv,
        &[
            ("A", &["a.mkv"]),
            ("B", &["Season 1/b.mkv", "Season 2/c.mkv"]),
            ("C", &["c.mkv"]),
        ],
    );
    let lf = t.path().join("l.json");
    // Deselect C, then group shows 1 and 2, then add... and back out.
    let screen = drive(
        &lf,
        &format!("a\n{}\nw\n3\nb\nG\ng 1 2\nb\nq\n", tv.display()),
    );
    assert!(
        screen.contains("  [✓]  3. C   1 season     1 ep \n"),
        "{screen}"
    );
    assert!(screen.contains("  [ ]  3. C   1 season     1 ep \n"));
    assert!(screen.contains("  Show Selection  (2/3 selected)\n"));
    assert!(screen.contains("  G1   2. B   2 seasons    2 eps\n"));
    assert!(screen.contains("  Show Groups  (1 group)\n"));
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!(s.selected, ["A", "B"]);
    assert_eq!(s.groups, [["A", "B"]]);
}

#[test]
fn duplicate_directory_refused_and_output_saved_as_typed() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("tv");
    tree(&tv, &[("A", &["a.mkv"])]);
    let lf = t.path().join("l.json");
    let screen = drive(
        &lf,
        &format!("a\n{0}\na\n{0}/../tv\n\no\nout.m3u\nq\n", tv.display()),
    );
    assert!(
        screen.contains("is already added. Press Enter..."),
        "{screen}"
    );
    assert!(screen.contains("  Output      : out.m3u\n"));
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!(s.dirs.len(), 1);
    assert_eq!(s.output_path, "out.m3u");
}

#[test]
fn add_to_group_uses_numbers_as_listed() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("tv");
    tree(
        &tv,
        &[("A", &["a.mkv"]), ("B", &["b.mkv"]), ("C", &["c.mkv"])],
    );
    let lf = t.path().join("l.json");
    // Groups [A] [B] [C]; moving A into group 2 empties group 1, but A still joins B.
    drive(
        &lf,
        &format!("a\n{}\nG\ng 1\ng 2\ng 3\na 2 1\nb\nq\n", tv.display()),
    );
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!(s.groups, [vec!["B", "A"], vec!["C"]]);
}

#[test]
fn eof_exits_cleanly_anywhere() {
    let t = TempDir::new().unwrap();
    for script in ["", "a\n", "s\n", "o"] {
        drive(&t.path().join("l.json"), script);
    }
}

#[test]
fn eof_inside_submenu_keeps_edits() {
    let t = TempDir::new().unwrap();
    let tv = t.path().join("tv");
    tree(&tv, &[("A", &["a.mkv"]), ("B", &["b.mkv"])]);
    let lf = t.path().join("l.json");
    drive(&lf, &format!("a\n{}\nw\n1\n", tv.display()));
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!(s.selected, ["B"]);
}

#[test]
fn eof_at_main_menu_leaves_layout_alone() {
    let t = TempDir::new().unwrap();
    let lf = t.path().join("l.json");
    let saved = State {
        dirs: vec![t.path().join("unplugged").display().to_string()],
        selected: vec!["X".into()],
        groups: vec![vec!["X".into(), "Y".into()]],
        ..State::default()
    };
    LayoutStore::new(lf.clone())
        .autosave_session(&saved)
        .unwrap();
    drive(&lf, "");
    let s = LayoutStore::new(lf).get_layout(LAST_SESSION).unwrap();
    assert_eq!((s.selected, s.groups), (saved.selected, saved.groups));
}

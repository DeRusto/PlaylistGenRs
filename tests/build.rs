mod fixtures;

use fixtures::{playlist_names, playlist_paths, tree};
use playlistgenrs::build::{BuildOptions, BuildReport, Mode, ShowOrder, build_playlist};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn run(dir: &Path, opts: BuildOptions) -> (BuildReport, PathBuf) {
    let out = dir.join("out.m3u");
    let r = build_playlist(
        &[dir.to_path_buf()],
        &out,
        &opts,
        &mut StdRng::seed_from_u64(1),
    );
    (r, out)
}

fn groups(g: &[&[&str]]) -> Vec<Vec<String>> {
    g.iter()
        .map(|g| g.iter().map(|s| s.to_string()).collect())
        .collect()
}

fn abc(t: &TempDir) {
    tree(
        t.path(),
        &[
            ("ShowA", &["a01.mkv", "a02.mkv", "a03.mkv"]),
            ("ShowB", &["b01.mkv", "b02.mkv"]),
            ("ShowC", &["c01.mkv"]),
        ],
    );
}

#[test]
fn two_shows() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            ("ShowA", &["a01.mkv", "a02.mkv", "a03.mkv"]),
            ("ShowB", &["b01.mkv", "b02.mkv"]),
        ],
    );
    let (r, out) = run(
        t.path(),
        BuildOptions {
            relative: true,
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 5);
    assert_eq!(
        playlist_names(&out),
        ["a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv"]
    );
    assert_eq!(
        r.summary_lines(),
        [
            "Found 2 slot(s):",
            "  ShowA: 3 episode(s)",
            "  ShowB: 2 episode(s)"
        ]
    );
    assert_eq!(r.error, None);
}

#[test]
fn repeat_cycles_shorter_show() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            ("ShowA", &["a01.mkv", "a02.mkv", "a03.mkv"]),
            ("ShowB", &["b01.mkv", "b02.mkv"]),
        ],
    );
    let (r, out) = run(
        t.path(),
        BuildOptions {
            repeat: true,
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 6);
    assert_eq!(
        playlist_names(&out),
        [
            "a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv", "b01.mkv"
        ]
    );
}

#[test]
fn skips_empty_show_dirs_with_warning() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            ("ShowA", &["a01.mkv", "cover.jpg"]),
            ("Empty", &["info.nfo"]),
        ],
    );
    let (r, _) = run(t.path(), BuildOptions::default());
    assert_eq!(r.entries, 1);
    assert_eq!(r.warnings, ["  Skipping 'Empty' — no video files found"]);
}

#[test]
fn empty_dir_returns_zero() {
    let t = TempDir::new().unwrap();
    let (r, out) = run(t.path(), BuildOptions::default());
    assert_eq!(
        (r.entries, r.error.as_deref()),
        (0, Some("No show subdirectories found."))
    );
    assert!(!out.exists());

    fs::create_dir(t.path().join("Empty")).unwrap();
    let (r, _) = run(t.path(), BuildOptions::default());
    assert_eq!(r.error.as_deref(), Some("No video files found."));
    assert!(r.slots.is_empty());

    let r = build_playlist(
        &[],
        &out,
        &BuildOptions::default(),
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(r.error.as_deref(), Some("No directories specified."));
}

#[test]
fn relative_and_absolute_paths() {
    let t = TempDir::new().unwrap();
    tree(t.path(), &[("ShowA", &["Season 1/ep1.mkv"])]);
    let (_, out) = run(
        t.path(),
        BuildOptions {
            relative: true,
            ..Default::default()
        },
    );
    assert_eq!(playlist_paths(&out), ["ShowA/Season 1/ep1.mkv"]);

    let sub = t.path().join("lists");
    fs::create_dir(&sub).unwrap();
    let out2 = sub.join("x.m3u");
    let opts = BuildOptions {
        relative: true,
        ..Default::default()
    };
    build_playlist(
        &[t.path().to_path_buf()],
        &out2,
        &opts,
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(playlist_paths(&out2), ["../ShowA/Season 1/ep1.mkv"]);

    let (_, out) = run(t.path(), BuildOptions::default());
    let abs = t.path().join("ShowA/Season 1/ep1.mkv");
    assert_eq!(playlist_paths(&out), [abs.to_string_lossy()]);
}

#[test]
fn absolute_paths_are_normalized_not_canonicalized() {
    let t = TempDir::new().unwrap();
    tree(t.path(), &[("ShowA", &["ep1.mkv"])]);
    let dotted = t.path().join("ShowA/..");
    let out = t.path().join("out.m3u");
    build_playlist(
        &[dotted],
        &out,
        &BuildOptions::default(),
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(
        playlist_paths(&out),
        [t.path().join("ShowA/ep1.mkv").to_string_lossy()]
    );
}

#[test]
fn exact_m3u_bytes() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[("ShowA", &["My.Ep 1.mkv"]), ("ShowB", &["b.MP4"])],
    );
    let (_, out) = run(
        t.path(),
        BuildOptions {
            relative: true,
            ..Default::default()
        },
    );
    assert_eq!(
        fs::read_to_string(out).unwrap(),
        "#EXTM3U\n#EXTINF:-1,My.Ep 1\nShowA/My.Ep 1.mkv\n#EXTINF:-1,b\nShowB/b.MP4\n"
    );
}

#[test]
fn multiple_dirs_and_merged_duplicates() {
    let t = TempDir::new().unwrap();
    tree(
        &t.path().join("dir1"),
        &[("ShowA", &["a01.mkv", "a02.mkv"])],
    );
    tree(
        &t.path().join("dir2"),
        &[("ShowB", &["b01.mkv", "b02.mkv"]), ("showa", &["a03.mkv"])],
    );
    let out = t.path().join("out.m3u");
    let dirs = [t.path().join("dir1"), t.path().join("dir2")];
    let r = build_playlist(
        &dirs,
        &out,
        &BuildOptions::default(),
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(r.entries, 5);
    assert_eq!(
        playlist_names(&out),
        ["a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv"]
    );
}

#[test]
fn episodes_mode_blocks() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            (
                "ShowA",
                &["a01.mkv", "a02.mkv", "a03.mkv", "a04.mkv", "a05.mkv"],
            ),
            ("ShowB", &["b01.mkv", "b02.mkv", "b03.mkv"]),
        ],
    );
    let (r, out) = run(
        t.path(),
        BuildOptions {
            mode: Mode::Episodes,
            block_size: 2,
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 8);
    assert_eq!(
        playlist_names(&out),
        [
            "a01.mkv", "a02.mkv", "b01.mkv", "b02.mkv", "a03.mkv", "a04.mkv", "b03.mkv", "a05.mkv"
        ]
    );
}

#[test]
fn seasons_mode() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            (
                "ShowA",
                &[
                    "Season 1/s01e01.mkv",
                    "Season 1/s01e02.mkv",
                    "Season 2/s02e01.mkv",
                ],
            ),
            (
                "ShowB",
                &[
                    "Season 1/b01e01.mkv",
                    "Season 2/b02e01.mkv",
                    "Season 2/b02e02.mkv",
                ],
            ),
        ],
    );
    let (r, out) = run(
        t.path(),
        BuildOptions {
            mode: Mode::Seasons,
            ..Default::default()
        },
    );
    assert_eq!(
        playlist_names(&out),
        [
            "s01e01.mkv",
            "s01e02.mkv",
            "b01e01.mkv",
            "s02e01.mkv",
            "b02e01.mkv",
            "b02e02.mkv"
        ]
    );
    assert_eq!(
        r.summary_lines()[1],
        "  ShowA: 3 episode(s) in 2 season group(s)"
    );
}

#[test]
fn include_filters_shows() {
    let t = TempDir::new().unwrap();
    abc(&t);
    let include = Some(HashSet::from(["ShowA".to_string(), "ShowC".to_string()]));
    let (r, out) = run(
        t.path(),
        BuildOptions {
            include,
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 4);
    assert_eq!(
        playlist_names(&out),
        ["a01.mkv", "c01.mkv", "a02.mkv", "a03.mkv"]
    );

    let (r, _) = run(
        t.path(),
        BuildOptions {
            include: Some(HashSet::new()),
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 0);
}

#[test]
fn group_is_one_slot_at_first_member_position() {
    let t = TempDir::new().unwrap();
    abc(&t);
    tree(t.path(), &[("ShowD", &["d01.mkv"])]);
    // Group members listed out of scan order: placed where ShowA (first in scan order) is.
    let g = groups(&[&["ShowC", "ShowA"]]);
    let (r, out) = run(
        t.path(),
        BuildOptions {
            groups: g,
            ..Default::default()
        },
    );
    assert_eq!(
        r.summary_lines(),
        [
            "Found 3 slot(s):",
            "  Group 1 (ShowC, ShowA): 4 episode(s)",
            "  ShowB: 2 episode(s)",
            "  ShowD: 1 episode(s)"
        ]
    );
    assert_eq!(
        playlist_names(&out),
        [
            "c01.mkv", "b01.mkv", "d01.mkv", "a01.mkv", "b02.mkv", "a02.mkv", "a03.mkv"
        ]
    );
}

#[test]
fn group_concatenates_members_and_repeats() {
    let t = TempDir::new().unwrap();
    abc(&t);
    let (r, out) = run(
        t.path(),
        BuildOptions {
            groups: groups(&[&["ShowB", "ShowC"]]),
            ..Default::default()
        },
    );
    assert_eq!(r.entries, 6);
    assert_eq!(
        playlist_names(&out),
        [
            "a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv", "c01.mkv"
        ]
    );

    let opts = BuildOptions {
        groups: groups(&[&["ShowA"], &["ShowB", "ShowC"]]),
        repeat: true,
        ..Default::default()
    };
    let (r, out) = run(t.path(), opts);
    assert_eq!(r.entries, 6);
    assert_eq!(
        playlist_names(&out),
        [
            "a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv", "c01.mkv"
        ]
    );
}

#[test]
fn group_filtering_and_first_group_wins() {
    let t = TempDir::new().unwrap();
    abc(&t);
    // "Missing" is dropped but numbering follows the layout, and ShowB plays only in the
    // first group that lists it.
    let g = groups(&[&["Missing"], &["ShowB", "ShowC"], &["ShowB", "ShowA"]]);
    let (r, _) = run(
        t.path(),
        BuildOptions {
            groups: g,
            ..Default::default()
        },
    );
    let names: Vec<_> = r.slots.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Group 3 (ShowA)", "Group 2 (ShowB, ShowC)"]);
}

#[test]
fn group_seasons_mode() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            (
                "ShowA",
                &["Season 1/a01.mkv", "Season 1/a02.mkv", "Season 2/a03.mkv"],
            ),
            ("ShowB", &["Season 1/b01.mkv"]),
            ("ShowC", &["Season 1/c01.mkv", "Season 2/c02.mkv"]),
        ],
    );
    let opts = BuildOptions {
        mode: Mode::Seasons,
        groups: groups(&[&["ShowB", "ShowC"]]),
        ..Default::default()
    };
    let (r, out) = run(t.path(), opts);
    assert_eq!(
        r.summary_lines()[2],
        "  Group 1 (ShowB, ShowC): 3 episode(s) in 3 season group(s)"
    );
    assert_eq!(
        playlist_names(&out),
        [
            "a01.mkv", "a02.mkv", "b01.mkv", "a03.mkv", "c01.mkv", "c02.mkv"
        ]
    );
}

#[test]
fn group_with_no_files_is_skipped() {
    let t = TempDir::new().unwrap();
    abc(&t);
    fs::create_dir(t.path().join("E1")).unwrap();
    fs::create_dir(t.path().join("E2")).unwrap();
    let (r, _) = run(
        t.path(),
        BuildOptions {
            groups: groups(&[&["E1", "E2"]]),
            ..Default::default()
        },
    );
    assert_eq!(
        r.warnings,
        ["  Skipping 'Group 1 (E1, E2)' — no video files found"]
    );
}

#[test]
fn seeded_random_order_is_deterministic_and_complete() {
    let t = TempDir::new().unwrap();
    abc(&t);
    let opts = BuildOptions {
        show_order: ShowOrder::Random,
        groups: groups(&[&["ShowB", "ShowC"]]),
        ..Default::default()
    };
    let (_, out) = run(t.path(), opts.clone());
    let first = fs::read_to_string(&out).unwrap();
    let (_, out) = run(t.path(), opts);
    assert_eq!(fs::read_to_string(&out).unwrap(), first);
    let mut names = playlist_names(&out);
    names.sort();
    assert_eq!(
        names,
        [
            "a01.mkv", "a02.mkv", "a03.mkv", "b01.mkv", "b02.mkv", "c01.mkv"
        ]
    );
}

#[test]
fn write_error_returns_zero() {
    let t = TempDir::new().unwrap();
    tree(t.path(), &[("ShowA", &["ep1.mkv"])]);
    let out = t.path().join("nonexistent_dir/out.m3u");
    let r = build_playlist(
        &[t.path().to_path_buf()],
        &out,
        &BuildOptions::default(),
        &mut StdRng::seed_from_u64(1),
    );
    assert_eq!(r.entries, 0);
    assert_eq!(
        r.error,
        Some(format!(
            "Error writing playlist: [Errno 2] No such file or directory: '{}'",
            out.display()
        ))
    );
}

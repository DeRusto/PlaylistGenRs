mod fixtures;

use fixtures::{names, touch, tree};
use playlistgenrs::scan::{collect_show_files, collect_show_seasons, refresh_shows, scan_dirs};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn show(tmp: &TempDir, files: &[&str]) -> Vec<PathBuf> {
    tree(tmp.path(), &[("Show", files)]);
    vec![tmp.path().join("Show")]
}

fn seasons(paths: &[PathBuf]) -> Vec<Vec<String>> {
    collect_show_seasons(paths, &mut vec![])
        .iter()
        .map(|s| names(s))
        .collect()
}

#[test]
fn flat_show() {
    let t = TempDir::new().unwrap();
    let s = show(&t, &["ep1.mkv", "ep2.mkv", "ep10.mkv"]);
    assert_eq!(
        names(&collect_show_files(&s)),
        ["ep1.mkv", "ep2.mkv", "ep10.mkv"]
    );
    assert_eq!(seasons(&s), [["ep1.mkv", "ep2.mkv", "ep10.mkv"]]);
}

#[test]
fn single_season_dir() {
    let t = TempDir::new().unwrap();
    let s = show(
        &t,
        &[
            "Season 1/s01e01.mkv",
            "Season 1/s01e02.mkv",
            "Season 1/s01e03.mkv",
        ],
    );
    assert_eq!(
        names(&collect_show_files(&s)),
        ["s01e01.mkv", "s01e02.mkv", "s01e03.mkv"]
    );
    assert_eq!(seasons(&s).len(), 1);
}

#[test]
fn season_2_before_10() {
    let t = TempDir::new().unwrap();
    let s = show(
        &t,
        &[
            "Season 2/s02e01.mkv",
            "Season 2/s02e02.mkv",
            "Season 10/s10e01.mkv",
            "Season 1/s01e01.mkv",
            "Season 1/s01e02.mkv",
        ],
    );
    assert_eq!(
        names(&collect_show_files(&s)),
        [
            "s01e01.mkv",
            "s01e02.mkv",
            "s02e01.mkv",
            "s02e02.mkv",
            "s10e01.mkv"
        ]
    );
    assert_eq!(
        seasons(&s),
        vec![
            vec!["s01e01.mkv", "s01e02.mkv"],
            vec!["s02e01.mkv", "s02e02.mkv"],
            vec!["s10e01.mkv"]
        ]
    );
}

#[test]
fn ep9_before_ep10() {
    let t = TempDir::new().unwrap();
    let s = show(
        &t,
        &[
            "Season 1/ep9.mkv",
            "Season 1/ep10.mkv",
            "Season 1/ep2.mkv",
            "Season 1/ep1.mkv",
        ],
    );
    assert_eq!(
        names(&collect_show_files(&s)),
        ["ep1.mkv", "ep2.mkv", "ep9.mkv", "ep10.mkv"]
    );
}

#[test]
fn non_video_ignored_and_uppercase_ext() {
    let t = TempDir::new().unwrap();
    let s = show(
        &t,
        &[
            "Season 1/ep1.mkv",
            "Season 1/ep2.MP4",
            "Season 1/poster.jpg",
            "metadata.nfo",
        ],
    );
    assert_eq!(names(&collect_show_files(&s)), ["ep1.mkv", "ep2.MP4"]);
}

#[test]
fn mixed_root_files_and_season_dirs() {
    let t = TempDir::new().unwrap();
    let s = show(&t, &["Season 1/s01e01.mkv", "special.mkv"]);
    assert_eq!(seasons(&s), [["special.mkv"], ["s01e01.mkv"]]);
    // Flat collection orders by full relative path: "season 1/..." < "special.mkv".
    assert_eq!(
        names(&collect_show_files(&s)),
        ["s01e01.mkv", "special.mkv"]
    );
}

#[test]
fn nested_files_belong_to_their_season() {
    let t = TempDir::new().unwrap();
    let s = show(
        &t,
        &[
            "Season 1/Disc 2/e3.mkv",
            "Season 1/e1.mkv",
            "Season 1/Disc 1/e2.mkv",
        ],
    );
    assert_eq!(seasons(&s), [["e2.mkv", "e3.mkv", "e1.mkv"]]);
}

#[test]
fn empty_show_and_empty_season() {
    let t = TempDir::new().unwrap();
    let s = show(&t, &["Season 2/s02e01.mkv"]);
    fs::create_dir(t.path().join("Show/Season 1")).unwrap();
    assert_eq!(seasons(&s), [["s02e01.mkv"]]);

    fs::create_dir(t.path().join("Empty")).unwrap();
    assert!(collect_show_seasons(&[t.path().join("Empty")], &mut vec![]).is_empty());
}

#[test]
fn scan_merges_case_insensitively_and_sorts() {
    let t = TempDir::new().unwrap();
    let (d1, d2) = (t.path().join("dir1"), t.path().join("dir2"));
    for d in ["dir1/ShowA", "dir1/ShowZ", "dir2/showa", "dir2/ShowM"] {
        fs::create_dir_all(t.path().join(d)).unwrap();
    }
    touch(&t.path().join("dir1/not_a_show.mkv"));
    let r = scan_dirs(&[d1.clone(), d2.clone()], &mut vec![]);
    let labels: Vec<_> = r.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["ShowA", "ShowM", "ShowZ"]);
    assert_eq!(r[0].1, [d1.join("ShowA"), d2.join("showa")]);
}

#[test]
fn scan_sort_is_plain_not_natural() {
    let t = TempDir::new().unwrap();
    for d in ["Show 10", "show 2"] {
        fs::create_dir(t.path().join(d)).unwrap();
    }
    let r = scan_dirs(&[t.path().to_path_buf()], &mut vec![]);
    assert_eq!(r[0].0, "Show 10");
}

#[test]
fn seasons_merge_across_dirs() {
    let t = TempDir::new().unwrap();
    tree(
        &t.path().join("dir1"),
        &[("ShowA", &["Season 1/s01e01.mkv"])],
    );
    tree(
        &t.path().join("dir2"),
        &[("ShowA", &["season 1/s01e02.mkv", "Season 2/s02e01.mkv"])],
    );
    let paths = [t.path().join("dir1/ShowA"), t.path().join("dir2/ShowA")];
    assert_eq!(
        seasons(&paths),
        vec![vec!["s01e01.mkv", "s01e02.mkv"], vec!["s02e01.mkv"]]
    );
    assert_eq!(
        names(&collect_show_files(&paths)),
        ["s01e01.mkv", "s01e02.mkv", "s02e01.mkv"]
    );
}

#[test]
fn refresh_skips_empty_shows() {
    let t = TempDir::new().unwrap();
    tree(
        t.path(),
        &[
            ("ShowA", &["Season 1/a.mkv", "Season 2/b.mkv", "c.mkv"]),
            ("Empty", &["x.txt"]),
        ],
    );
    let info = refresh_shows(&[t.path().to_path_buf()], &mut vec![]);
    assert_eq!(info.len(), 1);
    assert_eq!(
        (
            info[0].label.as_str(),
            info[0].n_seasons,
            info[0].n_episodes
        ),
        ("ShowA", 3, 3)
    );
}

#[test]
fn unreadable_media_dir_warns() {
    let t = TempDir::new().unwrap();
    let missing = t.path().join("missing");
    let mut w = vec![];
    assert!(scan_dirs(std::slice::from_ref(&missing), &mut w).is_empty());
    assert_eq!(
        w,
        [format!(
            "Warning: cannot read '{0}': [Errno 2] No such file or directory: '{0}'",
            missing.display()
        )]
    );
}

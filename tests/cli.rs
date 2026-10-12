mod fixtures;

use fixtures::{playlist_names, tree};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

fn cli(args: &[&str], cwd: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_playlistgenrs"))
        .args(args)
        .current_dir(cwd)
        // Keep the text menu off the user's real layouts.
        .env("XDG_CONFIG_HOME", cwd.join("cfg"))
        .output()
        .unwrap()
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn media(t: &TempDir) {
    tree(
        &t.path().join("tv"),
        &[
            ("ShowA", &["a01.mkv", "a02.mkv", "a03.mkv"]),
            ("ShowB", &["b01.mkv", "b02.mkv"]),
        ],
    );
}

#[test]
fn default_output_and_messages() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(&["tv"], t.path());
    assert_eq!(o.status.code(), Some(0), "{}", text(&o.stderr));
    let out = t.path().canonicalize().unwrap().join("tv/interleaved.m3u");
    assert_eq!(
        text(&o.stdout),
        format!(
            "Found 2 slot(s):\n  ShowA: 3 episode(s)\n  ShowB: 2 episode(s)\n\nPlaylist written to: {}  (5 entries)\n",
            out.display()
        )
    );
    assert_eq!(
        playlist_names(&out),
        ["a01.mkv", "b01.mkv", "a02.mkv", "b02.mkv", "a03.mkv"]
    );
    let first = fs::read_to_string(&out).unwrap();
    assert!(first.lines().nth(2).unwrap().starts_with('/'));
}

#[test]
fn output_relative_repeat() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(
        &["tv", "-o", "list.m3u", "--relative", "--repeat"],
        t.path(),
    );
    assert_eq!(o.status.code(), Some(0));
    let out = t.path().join("list.m3u");
    assert_eq!(
        fs::read_to_string(&out)
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#'))
            .collect::<Vec<_>>(),
        [
            "tv/ShowA/a01.mkv",
            "tv/ShowB/b01.mkv",
            "tv/ShowA/a02.mkv",
            "tv/ShowB/b02.mkv",
            "tv/ShowA/a03.mkv",
            "tv/ShowB/b01.mkv"
        ]
    );
    let o = cli(&["tv", "--output", "long.m3u"], t.path());
    assert_eq!(o.status.code(), Some(0));
    assert!(t.path().join("long.m3u").exists());
}

#[test]
fn interleave_modes() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(
        &[
            "tv",
            "--interleave",
            "episodes",
            "--block-size",
            "2",
            "-o",
            "e.m3u",
        ],
        t.path(),
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        playlist_names(&t.path().join("e.m3u")),
        ["a01.mkv", "a02.mkv", "b01.mkv", "b02.mkv", "a03.mkv"]
    );

    let o = cli(&["tv", "--interleave", "seasons", "-o", "s.m3u"], t.path());
    assert_eq!(o.status.code(), Some(0));
    assert!(text(&o.stdout).contains("  ShowA: 3 episode(s) in 1 season group(s)\n"));
}

#[test]
fn block_size_warning_without_episodes() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(
        &["tv", "--block-size", "3", "--interleave", "seasons"],
        t.path(),
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        text(&o.stderr),
        "Warning: --block-size has no effect when --interleave is 'seasons'.\n"
    );
    let o = cli(&["tv", "--block-size", "3"], t.path());
    assert_eq!(
        text(&o.stderr),
        "Warning: --block-size has no effect when --interleave is 'none'.\n"
    );
}

#[test]
fn bad_block_size_and_mode_are_usage_errors() {
    let t = TempDir::new().unwrap();
    media(&t);
    for bad in ["0", "-2", "abc"] {
        let o = cli(
            &["tv", "--interleave", "episodes", "--block-size", bad],
            t.path(),
        );
        assert_eq!(o.status.code(), Some(2), "{bad}");
        assert!(text(&o.stderr).contains("error"), "{}", text(&o.stderr));
    }
    let o = cli(&["tv", "--block-size", "0"], t.path());
    assert!(text(&o.stderr).contains("must be a positive integer, got 0"));
    let o = cli(&["tv", "--interleave", "weekly"], t.path());
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn missing_media_dir() {
    let t = TempDir::new().unwrap();
    let o = cli(&["--relative"], t.path());
    assert_eq!(o.status.code(), Some(2));
    assert!(text(&o.stderr).contains("the following arguments are required: media_dir"));
}

#[test]
fn not_a_directory() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(&["tv", "nope"], t.path());
    assert_eq!(o.status.code(), Some(1));
    let nope = t.path().canonicalize().unwrap().join("nope");
    assert_eq!(
        text(&o.stderr),
        format!("Error: '{}' is not a directory.\n", nope.display())
    );
}

#[test]
fn nothing_found_exits_1() {
    let t = TempDir::new().unwrap();
    fs::create_dir_all(t.path().join("tv/Empty")).unwrap();
    let o = cli(&["tv"], t.path());
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        text(&o.stderr),
        "  Skipping 'Empty' — no video files found\nNo video files found.\n"
    );
    assert_eq!(text(&o.stdout), "");
}

#[test]
fn write_failure_exits_1() {
    let t = TempDir::new().unwrap();
    media(&t);
    let o = cli(&["tv", "-o", "missing/x.m3u"], t.path());
    assert_eq!(o.status.code(), Some(1));
    assert!(
        text(&o.stderr).starts_with("Error writing playlist: [Errno 2] No such file or directory")
    );
}

#[test]
fn help_mentions_examples() {
    let t = TempDir::new().unwrap();
    let o = cli(&["--help"], t.path());
    assert_eq!(o.status.code(), Some(0));
    assert!(text(&o.stdout).contains("playlistgenrs /media/tv --interleave seasons"));
}

#[test]
fn text_flag_ignores_dirs_and_quits_on_eof() {
    let t = TempDir::new().unwrap();
    let o = cli(&["--text", "does-not-exist"], t.path());
    assert_eq!(o.status.code(), Some(0));
}

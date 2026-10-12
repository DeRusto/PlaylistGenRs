//! Golden tests against the Python implementation: both CLIs must write byte-identical M3U
//! files. Runs only when `python3` and `reference/playlist_gen.py` (or `$PLAYLISTGEN_PY`)
//! exist; otherwise it prints a note and passes.

mod fixtures;

use fixtures::tree;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn python_script() -> Option<PathBuf> {
    let p = std::env::var_os("PLAYLISTGEN_PY")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("reference/playlist_gen.py"));
    let has_python = Command::new("python3")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    (p.exists() && has_python).then_some(p)
}

fn media(base: &Path) -> Vec<Vec<PathBuf>> {
    tree(
        &base.join("flat"),
        &[
            (
                "ShowA",
                &["ep1.mkv", "ep2.mkv", "ep10.mkv", "ep9.MP4", "notes.txt"],
            ),
            ("ShowB", &["b1.avi", "b2.avi"]),
        ],
    );
    tree(
        &base.join("seasons"),
        &[
            (
                "Alpha",
                &[
                    "Season 1/s01e01.mkv",
                    "Season 1/s01e02.mkv",
                    "Season 2/s02e01.mkv",
                    "Season 10/s10e01.mkv",
                ],
            ),
            (
                "Beta",
                &[
                    "Season 1/b1.mkv",
                    "Season 2/b2.mkv",
                    "Season 2/b3.mkv",
                    "Season 2/Extras/x.mkv",
                ],
            ),
            ("Empty", &["cover.jpg"]),
        ],
    );
    tree(
        &base.join("mixed"),
        &[
            (
                "Gamma",
                &[
                    "special.mkv",
                    "Season 1/g1.mkv",
                    "Season 2/g2.mkv",
                    "Season 2/g3.mkv",
                ],
            ),
            (
                "Delta",
                &["d1.webm", "d2.webm", "d3.webm", "d4.webm", "d5.webm"],
            ),
        ],
    );
    tree(
        &base.join("one"),
        &[
            ("Shared", &["Season 1/a1.mkv"]),
            ("Solo", &["s1.mkv", "s2.mkv"]),
        ],
    );
    tree(
        &base.join("two"),
        &[
            ("shared", &["season 1/a2.mkv", "Season 2/a3.mkv"]),
            ("Other", &["o1.ts"]),
        ],
    );
    let d = |n: &str| base.join(n);
    vec![
        vec![d("flat")],
        vec![d("seasons")],
        vec![d("mixed")],
        vec![d("one"), d("two")],
        vec![d("flat"), d("mixed")],
    ]
}

#[test]
fn m3u_matches_python() {
    let Some(py) = python_script() else {
        eprintln!("parity: skipped (python3 or reference/playlist_gen.py not found)");
        return;
    };
    let t = TempDir::new().unwrap();
    let base = t.path().canonicalize().unwrap();
    let sets = media(&base);
    let out = base.join("out");
    fs::create_dir_all(out.join("py")).unwrap();
    fs::create_dir_all(out.join("rs")).unwrap();

    let modes: [&[&str]; 4] = [
        &["--interleave", "none"],
        &["--interleave", "episodes", "--block-size", "2"],
        &["--interleave", "episodes", "--block-size", "10"],
        &["--interleave", "seasons"],
    ];
    let mut checked = 0;
    for (si, dirs) in sets.iter().enumerate() {
        for mode in modes {
            for repeat in [false, true] {
                for relative in [false, true] {
                    let mut args: Vec<String> = dirs
                        .iter()
                        .map(|d| d.to_string_lossy().into_owned())
                        .collect();
                    args.extend(mode.iter().map(|s| s.to_string()));
                    if repeat {
                        args.push("--repeat".into());
                    }
                    if relative {
                        args.push("--relative".into());
                    }
                    let case = format!("set {si} {args:?}");
                    let (py_out, rs_out) = (out.join("py/list.m3u"), out.join("rs/list.m3u"));

                    let p = Command::new("python3")
                        .arg(&py)
                        .args(&args)
                        .arg("-o")
                        .arg(&py_out)
                        .output()
                        .unwrap();
                    let r = Command::new(env!("CARGO_BIN_EXE_playlistgenrs"))
                        .args(&args)
                        .arg("-o")
                        .arg(&rs_out)
                        .output()
                        .unwrap();
                    assert_eq!(p.status.code(), r.status.code(), "{case}");
                    assert_eq!(
                        String::from_utf8_lossy(&p.stdout),
                        String::from_utf8_lossy(&r.stdout).replace("/rs/", "/py/"),
                        "{case}"
                    );
                    assert_eq!(
                        String::from_utf8_lossy(&p.stderr),
                        String::from_utf8_lossy(&r.stderr),
                        "{case}"
                    );
                    assert_eq!(
                        fs::read(&py_out).unwrap(),
                        fs::read(&rs_out).unwrap(),
                        "{case}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 80);
}

//! Helpers to build temporary media trees. Included by other tests via `mod fixtures;`.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// Create `base/<show>/<file>` for every entry; `file` may contain `/` for season subdirs.
pub fn tree(base: &Path, shows: &[(&str, &[&str])]) {
    for (show, files) in shows {
        let dir = base.join(show);
        fs::create_dir_all(&dir).unwrap();
        for f in *files {
            touch(&dir.join(f));
        }
    }
}

pub fn touch(p: &Path) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, "").unwrap();
}

pub fn names(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

/// Non-comment lines of an M3U file.
pub fn playlist_paths(m3u: &Path) -> Vec<String> {
    fs::read_to_string(m3u)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

pub fn playlist_names(m3u: &Path) -> Vec<String> {
    playlist_paths(m3u)
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

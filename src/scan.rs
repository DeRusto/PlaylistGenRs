//! Media directory scanning (Python `_scan_dirs`, `_refresh_shows`, `collect_show_*`).
//!
//! Warnings are pushed onto a caller-supplied `Vec<String>` instead of printed.

use natord::compare_ignore_case;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "wmv", "flv", "m4v", "mpg", "mpeg", "ts", "m2ts", "webm", "ogv",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowInfo {
    pub label: String,
    pub paths: Vec<PathBuf>,
    pub n_seasons: usize,
    pub n_episodes: usize,
}

/// A regular file (following symlinks) with a video extension, matched case-insensitively.
pub fn is_video(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        && p.is_file()
}

/// Format an I/O error like Python's `OSError.__str__`: `[Errno 13] Permission denied: '/x'`.
pub fn py_os_error(e: &io::Error, path: &Path) -> String {
    match e.raw_os_error() {
        Some(code) => {
            let msg = e.to_string();
            let msg = msg
                .strip_suffix(&format!(" (os error {code})"))
                .unwrap_or(&msg);
            format!("[Errno {code}] {msg}: '{}'", path.display())
        }
        None => e.to_string(),
    }
}

fn read_dir_warn(dir: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(e) => {
            warnings.push(format!(
                "Warning: cannot read '{}': {}",
                dir.display(),
                py_os_error(&e, dir)
            ));
            Vec::new()
        }
    }
}

/// Group immediate subdirectories of `dirs` by lowercase name, keeping first-seen casing.
/// Returns groups in first-seen order.
fn subdirs_merged(dirs: &[PathBuf], warnings: &mut Vec<String>) -> Vec<(String, Vec<PathBuf>)> {
    let mut out: Vec<(String, Vec<PathBuf>)> = Vec::new();
    let mut by_lower: HashMap<String, usize> = HashMap::new();
    for d in dirs {
        for p in read_dir_warn(d, warnings) {
            if !p.is_dir() {
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            match by_lower.get(&name.to_lowercase()) {
                Some(&i) => out[i].1.push(p),
                None => {
                    by_lower.insert(name.to_lowercase(), out.len());
                    out.push((name, vec![p]));
                }
            }
        }
    }
    out
}

/// Every immediate subdirectory of each media dir is a show; same-named shows (ignoring
/// case) merge across media dirs. Sorted by lowercase label (plain, not natural).
pub fn scan_dirs(dirs: &[PathBuf], warnings: &mut Vec<String>) -> Vec<(String, Vec<PathBuf>)> {
    let mut shows = subdirs_merged(dirs, warnings);
    shows.sort_by_key(|(label, _)| label.to_lowercase());
    shows
}

fn name_cmp(a: &Path, b: &Path) -> Ordering {
    let name = |p: &Path| {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    };
    compare_ignore_case(&name(a), &name(b))
}

/// Natural order, compared component by component.
fn path_cmp(a: &Path, b: &Path) -> Ordering {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| compare_ignore_case(&x.to_string_lossy(), &y.to_string_lossy()))
        .find(|o| o.is_ne())
        .unwrap_or_else(|| a.iter().count().cmp(&b.iter().count()))
}

/// Recursive video files under `base`, paired with their path relative to `base`.
/// Like Python's `rglob`, unreadable subdirectories are skipped silently and directory
/// symlinks are not followed.
fn videos_under(base: &Path) -> impl Iterator<Item = (PathBuf, PathBuf)> + '_ {
    WalkDir::new(base)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|e| e.into_path())
        .filter(|p| is_video(p))
        .map(move |p| (p.strip_prefix(base).unwrap_or(&p).to_path_buf(), p))
}

fn sorted_videos<'a>(bases: impl IntoIterator<Item = &'a PathBuf>) -> Vec<PathBuf> {
    let mut all: Vec<_> = bases.into_iter().flat_map(|b| videos_under(b)).collect();
    all.sort_by(|a, b| path_cmp(&a.0, &b.0));
    all.into_iter().map(|(_, p)| p).collect()
}

/// All video files under the show dir(s), each ordered by its path relative to its own base.
pub fn collect_show_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    sorted_videos(paths)
}

/// Episodes grouped into seasons: loose files in the show dir(s) first, then one season per
/// immediate subdirectory (merged across dirs ignoring case, natural order). Empty seasons
/// are dropped.
pub fn collect_show_seasons(paths: &[PathBuf], warnings: &mut Vec<String>) -> Vec<Vec<PathBuf>> {
    let mut direct: Vec<PathBuf> = paths
        .iter()
        .flat_map(|d| read_dir_warn(d, warnings))
        .filter(|p| is_video(p))
        .collect();
    direct.sort_by(|a, b| name_cmp(a, b));

    let mut subdirs = subdirs_merged(paths, warnings);
    subdirs.sort_by(|a, b| compare_ignore_case(&a.0, &b.0));

    let mut seasons = Vec::new();
    if !direct.is_empty() {
        seasons.push(direct);
    }
    seasons.extend(
        subdirs
            .iter()
            .map(|(_, dirs)| sorted_videos(dirs))
            .filter(|s| !s.is_empty()),
    );
    seasons
}

/// Shows that contain at least one episode, with season/episode counts.
pub fn refresh_shows(dirs: &[PathBuf], warnings: &mut Vec<String>) -> Vec<ShowInfo> {
    scan_dirs(dirs, warnings)
        .into_iter()
        .filter_map(|(label, paths)| {
            let seasons = collect_show_seasons(&paths, warnings);
            let n_episodes: usize = seasons.iter().map(Vec::len).sum();
            (n_episodes > 0).then_some(ShowInfo {
                label,
                paths,
                n_seasons: seasons.len(),
                n_episodes,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_cmp_is_natural_per_component() {
        let mut v = vec![
            Path::new("Season 10/e1.mkv"),
            Path::new("season 2/e10.mkv"),
            Path::new("Season 2/e9.mkv"),
        ];
        v.sort_by(|a, b| path_cmp(a, b));
        assert_eq!(
            v,
            [
                Path::new("Season 2/e9.mkv"),
                Path::new("season 2/e10.mkv"),
                Path::new("Season 10/e1.mkv"),
            ]
        );
    }
}

//! Playlist building (Python `build_playlist`) and the M3U writer.

use crate::interleave::{interleave, interleave_by_season, interleave_in_blocks};
use crate::rotation::{self, Slot};
use crate::scan::{collect_show_files, collect_show_seasons, py_os_error, scan_dirs};
use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// 1 episode per show
    #[default]
    None,
    /// N consecutive episodes per show
    Episodes,
    /// 1 full season per show
    Seasons,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ShowOrder {
    #[default]
    Alpha,
    Random,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::None, Mode::Episodes, Mode::Seasons];
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::None => "none",
            Mode::Episodes => "episodes",
            Mode::Seasons => "seasons",
        }
    }
}

impl ShowOrder {
    pub const ALL: [ShowOrder; 2] = [ShowOrder::Alpha, ShowOrder::Random];
    pub fn as_str(self) -> &'static str {
        match self {
            ShowOrder::Alpha => "alpha",
            ShowOrder::Random => "random",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub relative: bool,
    pub repeat: bool,
    pub mode: Mode,
    pub block_size: usize,
    pub include: Option<HashSet<String>>,
    pub groups: Vec<Vec<String>>,
    pub show_order: ShowOrder,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions {
            relative: false,
            repeat: false,
            mode: Mode::None,
            block_size: 10,
            include: None,
            groups: Vec::new(),
            show_order: ShowOrder::Alpha,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotSummary {
    pub name: String,
    pub episodes: usize,
    /// Number of season groups; `Some` only in seasons mode.
    pub seasons: Option<usize>,
}

/// Outcome of a build. Frontends print `warnings` (stderr), then `summary_lines` (stdout),
/// then `error` (stderr) — the same order Python prints them in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildReport {
    pub slots: Vec<SlotSummary>,
    pub entries: usize,
    pub warnings: Vec<String>,
    /// Why nothing was written, when `entries == 0`.
    pub error: Option<String>,
}

impl BuildReport {
    pub fn summary_lines(&self) -> Vec<String> {
        if self.slots.is_empty() {
            return Vec::new();
        }
        let mut out = vec![format!("Found {} slot(s):", self.slots.len())];
        out.extend(self.slots.iter().map(|s| match s.seasons {
            Some(k) => format!(
                "  {}: {} episode(s) in {} season group(s)",
                s.name, s.episodes, k
            ),
            None => format!("  {}: {} episode(s)", s.name, s.episodes),
        }));
        out
    }
}

/// Scan `dirs`, interleave the shows' episodes and write an M3U to `output`.
pub fn build_playlist(
    dirs: &[PathBuf],
    output: &Path,
    opts: &BuildOptions,
    rng: &mut impl Rng,
) -> BuildReport {
    let mut report = BuildReport::default();
    let fail = |mut r: BuildReport, msg: String| {
        r.error = Some(msg);
        r
    };
    if dirs.is_empty() {
        return fail(report, "No directories specified.".into());
    }

    let mut entries = scan_dirs(dirs, &mut report.warnings);
    if let Some(inc) = &opts.include {
        entries.retain(|(label, _)| inc.contains(label));
    }
    if entries.is_empty() {
        return fail(report, "No show subdirectories found.".into());
    }
    let random = opts.show_order == ShowOrder::Random;
    if random {
        entries.shuffle(rng);
    }

    let label_to_paths: HashMap<&str, &[PathBuf]> = entries
        .iter()
        .map(|(l, p)| (l.as_str(), p.as_slice()))
        .collect();
    let used: Vec<&str> = entries.iter().map(|(l, _)| l.as_str()).collect();

    let seasons_mode = opts.mode == Mode::Seasons;
    // A slot is a list of seasons; in flat modes it is one "season" holding every episode.
    let collect = |members: &[String], warnings: &mut Vec<String>| -> Vec<Vec<PathBuf>> {
        let paths = members.iter().map(|m| label_to_paths[m.as_str()]);
        if seasons_mode {
            paths
                .flat_map(|p| collect_show_seasons(p, warnings))
                .collect()
        } else {
            let files: Vec<PathBuf> = paths.flat_map(collect_show_files).collect();
            if files.is_empty() {
                vec![]
            } else {
                vec![files]
            }
        }
    };

    let mut slots: Vec<(String, Vec<Vec<PathBuf>>)> = Vec::new();
    for Slot { group, mut members } in rotation::slots(&used, &opts.groups) {
        let name = match group {
            Some(g) => {
                if random {
                    members.shuffle(rng);
                }
                format!("Group {} ({})", g + 1, members.join(", "))
            }
            None => members[0].clone(),
        };
        let slot = collect(&members, &mut report.warnings);
        if slot.is_empty() {
            report
                .warnings
                .push(format!("  Skipping '{name}' — no video files found"));
        } else {
            slots.push((name, slot));
        }
    }
    if slots.is_empty() {
        return fail(report, "No video files found.".into());
    }

    report.slots = slots
        .iter()
        .map(|(name, s)| SlotSummary {
            name: name.clone(),
            episodes: s.iter().map(Vec::len).sum(),
            seasons: seasons_mode.then_some(s.len()),
        })
        .collect();

    let lists: Vec<Vec<Vec<PathBuf>>> = slots.into_iter().map(|(_, s)| s).collect();
    let playlist = match opts.mode {
        Mode::Seasons => interleave_by_season(&lists, opts.repeat),
        Mode::Episodes => interleave_in_blocks(&flat(lists), opts.block_size, opts.repeat),
        Mode::None => interleave(&flat(lists), opts.repeat),
    };

    if let Err(e) = std::fs::write(output, m3u(&playlist, output, opts.relative)) {
        report.slots.clear();
        return fail(
            report,
            format!("Error writing playlist: {}", py_os_error(&e, output)),
        );
    }
    report.entries = playlist.len();
    report
}

fn flat(slots: Vec<Vec<Vec<PathBuf>>>) -> Vec<Vec<PathBuf>> {
    slots.into_iter().map(|s| s.concat()).collect()
}

/// Render the M3U text: absolute (normalized) paths, or relative to `output`'s directory.
pub fn m3u(playlist: &[PathBuf], output: &Path, relative: bool) -> String {
    let base = output.parent().unwrap_or(Path::new(""));
    let mut s = String::from("#EXTM3U\n");
    for ep in playlist {
        let stem = ep.file_stem().unwrap_or_default().to_string_lossy();
        let path = if relative {
            relpath(ep, base)
        } else {
            normpath(ep)
        };
        s += &format!("#EXTINF:-1,{stem}\n{}\n", path.display());
    }
    s
}

/// Lexical normalization like Python's `os.path.normpath` (no symlink resolution).
pub fn normpath(p: &Path) -> PathBuf {
    let mut out: Vec<Component> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(c),
            },
            _ => out.push(c),
        }
    }
    if out.is_empty() {
        return PathBuf::from(".");
    }
    out.iter().collect()
}

/// Python `os.path.relpath(p, start)`: both sides made absolute against the cwd first.
pub fn relpath(p: &Path, start: &Path) -> PathBuf {
    let abs = |x: &Path| normpath(&std::path::absolute(x).unwrap_or_else(|_| x.to_path_buf()));
    pathdiff::diff_paths(abs(p), abs(start))
        .filter(|r| !r.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Python `Path(s).expanduser().resolve()` for user-typed paths.
pub fn resolve_user_path(s: &str) -> PathBuf {
    let p = match s.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => dirs::home_dir()
            .unwrap_or_default()
            .join(rest.trim_start_matches('/')),
        _ => PathBuf::from(s),
    };
    resolve(&p)
}

/// Python `Path.resolve()` (non-strict): walk the components, canonicalizing as long as the
/// path exists so a `..` after a symlink applies to the link's target.
pub fn resolve(p: &Path) -> PathBuf {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            _ => {
                out.push(c);
                if let Ok(real) = out.canonicalize() {
                    out = real;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normpath_like_python() {
        for (i, o) in [
            ("/a/./b/../c", "/a/c"),
            ("/../x", "/x"),
            ("a/../../b", "../b"),
            ("./", "."),
            ("a//b/", "a/b"),
        ] {
            assert_eq!(normpath(Path::new(i)), Path::new(o), "{i}");
        }
    }

    #[test]
    fn resolve_follows_symlink_before_parent() {
        let t = tempfile::TempDir::new().unwrap();
        let base = t.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("mnt/tv/child")).unwrap();
        std::os::unix::fs::symlink(base.join("mnt/tv/child"), base.join("link")).unwrap();
        assert_eq!(resolve(&base.join("link/..")), base.join("mnt/tv"));
        assert_eq!(
            resolve(&base.join("link/new/../x.m3u")),
            base.join("mnt/tv/child/x.m3u")
        );
    }

    #[test]
    fn relpath_like_python() {
        assert_eq!(
            relpath(Path::new("/m/tv/A/e.mkv"), Path::new("/m/tv")),
            Path::new("A/e.mkv")
        );
        assert_eq!(
            relpath(Path::new("/m/tv/A/e.mkv"), Path::new("/m/out")),
            Path::new("../tv/A/e.mkv")
        );
        assert_eq!(relpath(Path::new("/m"), Path::new("/m")), Path::new("."));
    }
}

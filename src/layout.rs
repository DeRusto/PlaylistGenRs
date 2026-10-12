//! Layout persistence, compatible with the Python app's `playlist_layouts.json`,
//! plus the selection/group editing helpers shared by the text menu and GUI.

use crate::build::{Mode, ShowOrder};
use crate::scan::ShowInfo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::PathBuf;

pub const LAST_SESSION: &str = "Last Session State";

/// Same path as the Python app, so existing layouts carry over.
pub fn layout_file() -> PathBuf {
    let base = match std::env::var_os("XDG_CONFIG_HOME") {
        Some(x) if !x.is_empty() => PathBuf::from(x),
        _ => dirs::home_dir().unwrap_or_default().join(".config"),
    };
    base.join("playlist_gen").join("playlist_layouts.json")
}

/// One saved layout. Every field is optional on read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub dirs: Vec<String>,
    pub selected: Vec<String>,
    pub groups: Vec<Vec<String>>,
    pub output_path: String,
    pub relative: bool,
    pub repeat: bool,
    pub show_order: ShowOrder,
    pub interleave_mode: Mode,
    /// Stored as a string in the file; read leniently (string or number, invalid → 10).
    #[serde(deserialize_with = "de_block_size", serialize_with = "ser_block_size")]
    pub block_size: usize,
}

impl Default for State {
    fn default() -> Self {
        State {
            dirs: vec![],
            selected: vec![],
            groups: vec![],
            output_path: String::new(),
            relative: false,
            repeat: false,
            show_order: ShowOrder::Alpha,
            interleave_mode: Mode::None,
            block_size: 10,
        }
    }
}

/// Python: `int(raw)` if `str(raw).strip().isdigit()` and positive, else 10.
pub fn parse_block_size(raw: &str) -> usize {
    let t = raw.trim();
    match t.parse::<usize>() {
        Ok(n) if n > 0 && t.bytes().all(|b| b.is_ascii_digit()) => n,
        _ => 10,
    }
}

fn de_block_size<'de, D: Deserializer<'de>>(d: D) -> Result<usize, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => parse_block_size(&s),
        Value::Number(n) => parse_block_size(&n.to_string()),
        _ => 10,
    })
}

fn ser_block_size<S: Serializer>(n: &usize, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&n.to_string())
}

/// The JSON layout store. Layouts are kept as raw JSON so unknown fields and insertion
/// order survive a round trip.
#[derive(Debug, Clone)]
pub struct LayoutStore {
    pub filename: PathBuf,
    pub layouts: Map<String, Value>,
    pub active_layout_name: Option<String>,
    /// Load warnings, for the frontend to print.
    pub warnings: Vec<String>,
    /// Set when the file existed but couldn't be parsed: autosave won't clobber it.
    load_failed: bool,
}

impl LayoutStore {
    pub fn new(filename: PathBuf) -> Self {
        let mut s = LayoutStore {
            filename,
            layouts: Map::new(),
            active_layout_name: None,
            warnings: vec![],
            load_failed: false,
        };
        s.load_all();
        s
    }

    pub fn open_default() -> Self {
        Self::new(layout_file())
    }

    pub fn load_all(&mut self) {
        self.layouts.clear();
        self.active_layout_name = None;
        self.load_failed = false;
        if !self.filename.exists() {
            return;
        }
        let parsed = std::fs::read_to_string(&self.filename)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Value>(&t).map_err(|e| e.to_string()));
        match parsed {
            Ok(Value::Object(mut root)) => {
                if let Some(Value::Object(l)) = root.remove("layouts") {
                    self.layouts = l;
                }
                if let Some(Value::String(a)) = root.remove("active_layout_name") {
                    self.active_layout_name = Some(a);
                }
            }
            other => {
                let err = other
                    .err()
                    .unwrap_or_else(|| "Root JSON is not a dictionary".into());
                self.warnings.push(format!(
                    "Warning: Failed to load layouts from {}: {err}",
                    self.filename.display()
                ));
                self.load_failed = true;
            }
        }
    }

    /// Atomic write: `playlist_layouts.tmp`, then rename over the real file.
    pub fn save_all(&mut self) -> Result<(), String> {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.filename.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = self.filename.with_extension("tmp");
            let data =
                json!({"layouts": self.layouts, "active_layout_name": self.active_layout_name});
            std::fs::write(&tmp, serde_json::to_string_pretty(&data)?)?;
            std::fs::rename(&tmp, &self.filename)
        };
        write().map_err(|e| {
            format!(
                "Error: Failed to save layouts to {}: {e}",
                self.filename.display()
            )
        })?;
        self.load_failed = false;
        Ok(())
    }

    pub fn get_layout(&self, name: &str) -> Option<State> {
        self.layouts
            .get(name)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Store `state` under `name`, make it active, and write the file.
    pub fn save_layout(&mut self, name: &str, state: &State) -> Result<(), String> {
        self.put(name, state);
        self.active_layout_name = Some(name.to_string());
        self.save_all()
    }

    /// Store `state` in the reserved session slot. Skipped while the file on disk is corrupt.
    pub fn autosave_session(&mut self, state: &State) -> Result<(), String> {
        self.put(LAST_SESSION, state);
        if self.load_failed {
            return Ok(());
        }
        self.save_all()
    }

    /// Save to the active layout if any, else to the session slot (both frontends' autosave).
    pub fn autosave(&mut self, state: &State) -> Result<(), String> {
        match self.active_layout_name.clone() {
            Some(name) => self.save_layout(&name, state),
            None => self.autosave_session(state),
        }
    }

    fn put(&mut self, name: &str, state: &State) {
        let v = serde_json::to_value(state).expect("State serializes");
        match self.layouts.get_mut(name) {
            // Merge so fields this version doesn't know about are kept.
            Some(Value::Object(old)) => {
                if let Value::Object(new) = v {
                    old.extend(new);
                }
            }
            _ => {
                self.layouts.insert(name.to_string(), v);
            }
        }
    }

    /// Remove `name`; if it was active, the first remaining layout (or none) becomes active.
    pub fn delete_layout(&mut self, name: &str) -> Result<bool, String> {
        if self.layouts.shift_remove(name).is_none() {
            return Ok(false);
        }
        if self.active_layout_name.as_deref() == Some(name) {
            self.active_layout_name = self.layouts.keys().next().cloned();
        }
        self.save_all().map(|_| true)
    }

    pub fn get_active_state(&self) -> Option<State> {
        self.active_layout_name
            .as_deref()
            .and_then(|n| self.get_layout(n))
    }

    /// State to restore on startup: active → "Last Session State" → (text menu only) first.
    pub fn startup_state(&self, fall_back_to_first: bool) -> Option<State> {
        self.get_active_state()
            .or_else(|| self.get_layout(LAST_SESSION))
            .or_else(|| {
                fall_back_to_first
                    .then(|| self.layouts.keys().next().and_then(|n| self.get_layout(n)))
                    .flatten()
            })
    }
}

pub fn labels(info: &[ShowInfo]) -> HashSet<String> {
    info.iter().map(|s| s.label.clone()).collect()
}

/// Keep only labels in `known`, dropping groups that end up empty.
pub fn filter_groups(groups: &[Vec<String>], known: &HashSet<String>) -> Vec<Vec<String>> {
    groups
        .iter()
        .map(|g| {
            g.iter()
                .filter(|l| known.contains(*l))
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|g| !g.is_empty())
        .collect()
}

/// After a rescan: keep surviving selections, auto-select newly discovered shows,
/// strip missing labels from groups.
pub fn rescan(
    old_labels: &HashSet<String>,
    new_info: &[ShowInfo],
    selected: &mut HashSet<String>,
    groups: &mut Vec<Vec<String>>,
) {
    let new = labels(new_info);
    selected.retain(|l| new.contains(l));
    selected.extend(new.difference(old_labels).cloned());
    *groups = filter_groups(groups, &new);
}

/// Selected labels in a stable order (show order first, then any others sorted).
pub fn selected_list(selected: &HashSet<String>, info: &[ShowInfo]) -> Vec<String> {
    let mut out: Vec<String> = info
        .iter()
        .map(|s| &s.label)
        .filter(|l| selected.contains(*l))
        .cloned()
        .collect();
    let mut rest: Vec<String> = selected
        .iter()
        .filter(|l| !out.contains(l))
        .cloned()
        .collect();
    rest.sort();
    out.extend(rest);
    out
}

/// Remove `labels` from every group, dropping groups that end up empty.
pub fn remove_from_groups(groups: &mut Vec<Vec<String>>, labels: &[String]) {
    for g in groups.iter_mut() {
        g.retain(|l| !labels.contains(l));
    }
    groups.retain(|g| !g.is_empty());
}

/// New group from `labels` (taken out of any other group first), appended at the end.
pub fn create_group(groups: &mut Vec<Vec<String>>, labels: &[String]) {
    remove_from_groups(groups, labels);
    groups.push(labels.to_vec());
}

/// Text menu `a <g#> <n>`: move `label` into group `g_idx`. Indexes are taken *after* the
/// label is removed from its old group, so they can shift (Python behavior, kept as is);
/// an out-of-range index appends a new group.
pub fn add_to_group(groups: &mut Vec<Vec<String>>, g_idx: usize, label: &str) {
    remove_from_groups(groups, &[label.to_string()]);
    match groups.get_mut(g_idx) {
        Some(g) => g.push(label.to_string()),
        None => groups.push(vec![label.to_string()]),
    }
}

/// GUI "Add Selected to Group": the target group is tracked by identity, so it keeps its
/// position unless removing `labels` empties it, in which case it moves to the end.
pub fn gui_add_to_group(groups: &mut Vec<Vec<String>>, g_idx: usize, labels: &[String]) {
    if g_idx >= groups.len() {
        remove_from_groups(groups, labels);
        groups.push(labels.to_vec());
        return;
    }
    for g in groups.iter_mut() {
        g.retain(|l| !labels.contains(l));
    }
    let mut target = groups[g_idx].clone();
    let keep_position = !target.is_empty();
    for l in labels {
        if !target.contains(l) {
            target.push(l.clone());
        }
    }
    if keep_position {
        groups[g_idx] = target;
        groups.retain(|g| !g.is_empty());
    } else {
        groups.retain(|g| !g.is_empty());
        groups.push(target);
    }
}

pub fn dissolve_group(groups: &mut Vec<Vec<String>>, g_idx: usize) {
    if g_idx < groups.len() {
        groups.remove(g_idx);
    }
}

/// `Group N (a, b, c...)` labels for group pickers: at most 3 names, then `...`.
pub fn group_choices(groups: &[Vec<String>]) -> Vec<String> {
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let names = g.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
            let more = if g.len() > 3 { "..." } else { "" };
            format!("Group {} ({names}{more})", i + 1)
        })
        .collect()
}

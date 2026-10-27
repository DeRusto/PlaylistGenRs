//! The session: the layout being edited in a running app, shared by the text menu and GUI.
//! It owns the layout store, the scanned shows and the layout's settings; each frontend keeps
//! only its own UI state on top.

use crate::build::{BuildOptions, Mode, ShowOrder, resolve, resolve_user_path};
use crate::groups;
use crate::layout::{self, LayoutStore, State};
use crate::scan::{ShowInfo, refresh_shows};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Why a directory couldn't be added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirError {
    NotADirectory(PathBuf),
    AlreadyAdded(PathBuf),
}

/// Fields are public so frontends can bind and edit them directly; call `autosave` after.
pub struct Session {
    pub store: LayoutStore,
    pub dirs: Vec<PathBuf>,
    pub show_info: Vec<ShowInfo>,
    /// Used shows, by label.
    pub selected: HashSet<String>,
    pub groups: Vec<Vec<String>>,
    /// As typed; resolved when building. Empty means `interleaved.m3u` in the first directory.
    pub output_path: String,
    pub relative: bool,
    pub repeat: bool,
    pub show_order: ShowOrder,
    pub mode: Mode,
    pub block_size: usize,
    /// Load, scan and save warnings, for the frontend to show and drain.
    pub warnings: Vec<String>,
}

impl Session {
    /// Open `store`, restoring the active layout, else "Last Session State".
    pub fn open(mut store: LayoutStore) -> Self {
        let warnings = std::mem::take(&mut store.warnings);
        let mut s = Session::blank(store, warnings);
        if let Some(state) = s.store.startup_state() {
            s.apply(&state);
        }
        s
    }

    fn blank(store: LayoutStore, warnings: Vec<String>) -> Self {
        Session {
            store,
            dirs: vec![],
            show_info: vec![],
            selected: HashSet::new(),
            groups: vec![],
            output_path: String::new(),
            relative: false,
            repeat: false,
            show_order: ShowOrder::Alpha,
            mode: Mode::None,
            block_size: 10,
            warnings,
        }
    }

    pub fn state(&self) -> State {
        State {
            dirs: self
                .dirs
                .iter()
                .map(|d| d.to_string_lossy().into_owned())
                .collect(),
            selected: layout::selected_list(&self.selected, &self.show_info),
            groups: self.groups.clone(),
            output_path: self.output_path.clone(),
            relative: self.relative,
            repeat: self.repeat,
            show_order: self.show_order,
            interleave_mode: self.mode,
            block_size: self.block_size,
        }
    }

    /// Replace everything with `state` and scan its directories. Used shows and groups that
    /// are no longer on disk are dropped.
    pub fn apply(&mut self, state: &State) {
        self.dirs = state
            .dirs
            .iter()
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .collect();
        self.show_info = refresh_shows(&self.dirs, &mut self.warnings);
        let all = layout::labels(&self.show_info);
        self.selected = state
            .selected
            .iter()
            .filter(|l| all.contains(*l))
            .cloned()
            .collect();
        self.groups = groups::retain_known(&state.groups, &all);
        self.output_path = state.output_path.clone();
        self.relative = state.relative;
        self.repeat = state.repeat;
        self.show_order = state.show_order;
        self.mode = state.interleave_mode;
        self.block_size = state.block_size;
    }

    /// Save to the active layout, or to the session slot when none is active.
    pub fn autosave(&mut self) {
        let state = self.state();
        if let Err(e) = self.store.autosave(&state) {
            self.warnings.push(e);
        }
    }

    /// Scan the directories again: keep surviving used shows, use newly found ones, and strip
    /// missing shows from groups.
    pub fn rescan(&mut self) {
        let old = layout::labels(&self.show_info);
        self.show_info = refresh_shows(&self.dirs, &mut self.warnings);
        let new = layout::labels(&self.show_info);
        self.selected.retain(|l| new.contains(l));
        self.selected.extend(new.difference(&old).cloned());
        self.groups = groups::retain_known(&self.groups, &new);
    }

    /// Add `path` (resolved through symlinks) and rescan. Returns the path as added.
    pub fn add_dir(&mut self, path: &Path) -> Result<PathBuf, DirError> {
        if !path.is_dir() {
            return Err(DirError::NotADirectory(path.to_path_buf()));
        }
        let p = resolve(path);
        if self.dirs.contains(&p) {
            return Err(DirError::AlreadyAdded(p));
        }
        self.dirs.push(p.clone());
        self.rescan();
        Ok(p)
    }

    /// Remove directory `i` and rescan. Returns the removed path.
    pub fn remove_dir(&mut self, i: usize) -> Option<PathBuf> {
        if i >= self.dirs.len() {
            return None;
        }
        let removed = self.dirs.remove(i);
        self.rescan();
        Some(removed)
    }

    /// Where the playlist goes, or `None` with no output path and no directories.
    pub fn output(&self) -> Option<PathBuf> {
        match self.output_path.trim() {
            "" => self.dirs.first().map(|d| d.join("interleaved.m3u")),
            out => Some(resolve_user_path(out)),
        }
    }

    /// Build options for the used shows and groups.
    pub fn build_options(&self) -> BuildOptions {
        let include = self
            .show_info
            .iter()
            .filter(|s| self.selected.contains(&s.label))
            .map(|s| s.label.clone())
            .collect();
        BuildOptions {
            relative: self.relative,
            repeat: self.repeat,
            mode: self.mode,
            block_size: self.block_size,
            include: Some(include),
            groups: self.groups.clone(),
            show_order: self.show_order,
        }
    }

    // --- Layouts ---

    /// Start an empty, unnamed layout.
    pub fn new_layout(&mut self) {
        let store = std::mem::replace(&mut self.store, LayoutStore::new(PathBuf::new()));
        let warnings = std::mem::take(&mut self.warnings);
        *self = Session::blank(store, warnings);
        self.store.active_layout_name = None;
        // Don't replace an unreadable layout file without an explicit save.
        if !self.store.load_failed()
            && let Err(e) = self.store.save_all()
        {
            self.warnings.push(e);
        }
    }

    /// Save under `name` and make it active.
    pub fn save_as(&mut self, name: &str) -> Result<(), String> {
        let state = self.state();
        self.store.save_layout(name, &state)
    }

    /// Make `name` active and apply it. Returns false when there's no such layout.
    pub fn load(&mut self, name: &str) -> bool {
        let Some(state) = self.store.get_layout(name) else {
            return false;
        };
        self.store.active_layout_name = Some(name.to_string());
        if let Err(e) = self.store.save_all() {
            self.warnings.push(e);
        }
        self.apply(&state);
        true
    }

    /// Delete `name`, then apply whichever layout became active, or start a new one.
    pub fn delete(&mut self, name: &str) {
        if let Err(e) = self.store.delete_layout(name) {
            self.warnings.push(e);
        }
        match self.store.get_active_state() {
            Some(state) => self.apply(&state),
            None => self.new_layout(),
        }
    }
}

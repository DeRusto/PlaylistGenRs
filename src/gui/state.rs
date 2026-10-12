//! GUI state and actions, kept free of egui so they can be unit tested.
//! Mirrors the Python `InterleaverGUI` methods; rendering lives in `app.rs`.

use crate::build::{BuildOptions, BuildReport, Mode, ShowOrder, resolve, resolve_user_path};
use crate::layout::{self, LayoutStore, State};
use crate::scan::{ShowInfo, refresh_shows};
use std::collections::HashSet;
use std::path::PathBuf;

/// A message box: (kind, title, text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msg {
    pub kind: Kind,
    pub title: String,
    pub text: String,
}

pub fn msg(kind: Kind, title: &str, text: impl Into<String>) -> Msg {
    Msg {
        kind,
        title: title.into(),
        text: text.into(),
    }
}

fn need_selection(text: &str) -> Msg {
    msg(Kind::Warning, "Selection Required", text)
}

/// Everything a background generation run needs.
pub struct Job {
    pub dirs: Vec<PathBuf>,
    pub output: PathBuf,
    pub opts: BuildOptions,
}

pub struct GuiState {
    pub store: LayoutStore,
    pub dirs: Vec<PathBuf>,
    pub show_info: Vec<ShowInfo>,
    pub selected: HashSet<String>,
    pub groups: Vec<Vec<String>>,
    pub output_path: String,
    pub relative: bool,
    pub repeat: bool,
    pub show_order: ShowOrder,
    pub mode: Mode,
    /// Raw text of the Block Size field; validated only on generate.
    pub block_size: String,
    /// Highlighted table rows, by label.
    pub rows: HashSet<String>,
    /// Anchor row index for shift-click range selection.
    pub anchor: Option<usize>,
    /// Highlighted directory in the directory list.
    pub dir_sel: Option<usize>,
    pub status: String,
}

impl GuiState {
    /// Restore the active layout, falling back to "Last Session State" only.
    pub fn new(store: LayoutStore) -> Self {
        let mut s = GuiState {
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
            block_size: "10".into(),
            rows: HashSet::new(),
            anchor: None,
            dir_sel: None,
            status: "Ready".into(),
        };
        for w in std::mem::take(&mut s.store.warnings) {
            eprintln!("{w}");
        }
        if let Some(state) = s.store.startup_state(false) {
            s.apply_state(&state);
        }
        s
    }

    pub fn title(&self) -> String {
        let name = self
            .store
            .active_layout_name
            .as_deref()
            .unwrap_or("Unsaved Layout");
        format!("TV Playlist Interleaver — [{name}]")
    }

    pub fn current_state(&self) -> State {
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
            block_size: layout::parse_block_size(&self.block_size),
        }
    }

    pub fn apply_state(&mut self, state: &State) {
        self.dirs = state.dirs.iter().map(PathBuf::from).collect();
        self.dir_sel = None;
        self.rescan_shows();
        let all = layout::labels(&self.show_info);
        self.selected = state
            .selected
            .iter()
            .filter(|l| all.contains(*l))
            .cloned()
            .collect();
        self.groups = layout::filter_groups(&state.groups, &all);
        self.output_path = state.output_path.clone();
        self.relative = state.relative;
        self.repeat = state.repeat;
        self.show_order = state.show_order;
        self.mode = state.interleave_mode;
        self.block_size = state.block_size.to_string();
    }

    /// Save to the active layout, or to the session slot when none is active.
    pub fn auto_save(&mut self) {
        let state = self.current_state();
        if let Err(e) = self.store.autosave(&state) {
            eprintln!("{e}");
        }
    }

    pub fn rescan_shows(&mut self) {
        let old = layout::labels(&self.show_info);
        let mut warnings = vec![];
        self.show_info = refresh_shows(&self.dirs, &mut warnings);
        for w in warnings {
            eprintln!("{w}");
        }
        layout::rescan(&old, &self.show_info, &mut self.selected, &mut self.groups);
        let all = layout::labels(&self.show_info);
        self.rows.retain(|l| all.contains(l));
        self.anchor = None;
    }

    // --- Directories ---

    pub fn add_directory(&mut self, chosen: PathBuf) -> Result<(), Msg> {
        let p = resolve(&chosen);
        if self.dirs.contains(&p) {
            return Err(msg(
                Kind::Info,
                "Directory exists",
                format!("Directory is already added:\n{}", p.display()),
            ));
        }
        self.dirs.push(p.clone());
        self.rescan_shows();
        self.status = format!("Added directory: {}", p.display());
        self.auto_save();
        Ok(())
    }

    pub fn remove_directory(&mut self) -> Result<(), Msg> {
        let Some(i) = self.dir_sel.filter(|&i| i < self.dirs.len()) else {
            return Err(msg(
                Kind::Warning,
                "Selection Required",
                "Please select a directory to remove.",
            ));
        };
        let removed = self.dirs.remove(i);
        self.dir_sel = None;
        self.rescan_shows();
        self.status = format!("Removed directory: {}", removed.display());
        self.auto_save();
        Ok(())
    }

    // --- Show table ---

    /// Highlighted labels in table order.
    pub fn row_labels(&self) -> Vec<String> {
        self.show_info
            .iter()
            .filter(|s| self.rows.contains(&s.label))
            .map(|s| s.label.clone())
            .collect()
    }

    /// Row click: plain selects one, ctrl toggles, shift selects a range from the anchor.
    pub fn click_row(&mut self, idx: usize, ctrl: bool, shift: bool) {
        let Some(label) = self.show_info.get(idx).map(|s| s.label.clone()) else {
            return;
        };
        match (shift, self.anchor) {
            (true, Some(a)) => {
                if !ctrl {
                    self.rows.clear();
                }
                let (lo, hi) = (a.min(idx), a.max(idx));
                self.rows
                    .extend(self.show_info[lo..=hi].iter().map(|s| s.label.clone()));
                return;
            }
            _ if ctrl => {
                if !self.rows.remove(&label) {
                    self.rows.insert(label);
                }
            }
            _ => {
                self.rows = HashSet::from([label]);
            }
        }
        self.anchor = Some(idx);
    }

    /// Text in the Group column: only for used shows; the last group listing a show wins.
    pub fn group_text(&self, label: &str) -> String {
        if !self.selected.contains(label) {
            return String::new();
        }
        match self
            .groups
            .iter()
            .rposition(|g| g.iter().any(|l| l == label))
        {
            Some(g) => format!("Group {}", g + 1),
            None => String::new(),
        }
    }

    /// Space / double-click: flip "Use?" on highlighted rows.
    pub fn toggle_show_selected(&mut self) {
        for l in self.row_labels() {
            if !self.selected.remove(&l) {
                self.selected.insert(l);
            }
        }
        self.auto_save();
    }

    fn check_in_use(&self, labels: &[String]) -> Result<(), Msg> {
        match labels.iter().find(|l| !self.selected.contains(*l)) {
            Some(l) => Err(msg(
                Kind::Warning,
                "Show not in Use",
                format!("Show '{l}' must be checked ('Use?') before grouping."),
            )),
            None => Ok(()),
        }
    }

    // --- Groups ---

    pub fn create_group_from_selected(&mut self) -> Result<(), Msg> {
        let labels = self.row_labels();
        self.check_in_use(&labels)?;
        if labels.len() < 2 {
            return Err(need_selection(
                "Please select 2 or more shows to create a group.",
            ));
        }
        layout::create_group(&mut self.groups, &labels);
        self.status = format!("Created group with {} shows.", labels.len());
        self.auto_save();
        Ok(())
    }

    /// Validation before showing the "Select Target Group" dialog.
    pub fn can_add_to_group(&self) -> Result<(), Msg> {
        let labels = self.row_labels();
        if labels.is_empty() {
            return Err(need_selection(
                "Please select one or more shows to add to a group.",
            ));
        }
        self.check_in_use(&labels)?;
        if self.groups.is_empty() {
            return Err(msg(
                Kind::Info,
                "No Groups",
                "There are no existing groups. Please use 'Create Group' first.",
            ));
        }
        Ok(())
    }

    pub fn add_selected_to_group(&mut self, g_idx: usize) {
        let labels = self.row_labels();
        layout::gui_add_to_group(&mut self.groups, g_idx, &labels);
        self.status = format!("Added {} shows to Group {}.", labels.len(), g_idx + 1);
        self.auto_save();
    }

    pub fn remove_selected_from_group(&mut self) -> Result<(), Msg> {
        let labels = self.row_labels();
        if labels.is_empty() {
            return Err(need_selection(
                "Please select one or more shows to remove from their groups.",
            ));
        }
        layout::remove_from_groups(&mut self.groups, &labels);
        self.status = "Removed selected shows from their groups.".into();
        self.auto_save();
        Ok(())
    }

    pub fn can_dissolve(&self) -> Result<(), Msg> {
        if self.groups.is_empty() {
            return Err(msg(
                Kind::Info,
                "No Groups",
                "No groups are currently defined.",
            ));
        }
        Ok(())
    }

    pub fn dissolve_group(&mut self, g_idx: usize) {
        if g_idx < self.groups.len() {
            layout::dissolve_group(&mut self.groups, g_idx);
            self.status = format!("Dissolved Group {}.", g_idx + 1);
            self.auto_save();
        }
    }

    // --- Generation ---

    pub fn prepare_generation(&mut self) -> Result<Job, Msg> {
        let err = |t: &str| msg(Kind::Error, "Error", t);
        if self.dirs.is_empty() {
            return Err(err("Please add at least one media directory first."));
        }
        if self.selected.is_empty() {
            return Err(err("Please select/check ('Use?') at least one show."));
        }
        let out = self.output_path.trim();
        let output = if out.is_empty() {
            self.dirs[0].join("interleaved.m3u")
        } else {
            resolve_user_path(out)
        };
        let mut block_size = 10;
        if self.mode == Mode::Episodes {
            let raw = self.block_size.trim();
            match raw.parse::<usize>() {
                Ok(n) if n > 0 && raw.bytes().all(|b| b.is_ascii_digit()) => block_size = n,
                _ => return Err(err("Block Size must be a positive integer.")),
            }
        }
        let include = self
            .show_info
            .iter()
            .filter(|s| self.selected.contains(&s.label))
            .map(|s| s.label.clone())
            .collect();
        self.status = "Generating playlist...".into();
        Ok(Job {
            dirs: self.dirs.clone(),
            output,
            opts: BuildOptions {
                relative: self.relative,
                repeat: self.repeat,
                mode: self.mode,
                block_size,
                include: Some(include),
                groups: self.groups.clone(),
                show_order: self.show_order,
            },
        })
    }

    pub fn generation_done(&mut self, output: &std::path::Path, r: &BuildReport) -> Msg {
        let entries = r.entries;
        if entries > 0 {
            let name = output.file_name().unwrap_or_default().to_string_lossy();
            self.status = format!("Generated {entries} entries into: {name}");
            msg(
                Kind::Info,
                "Success",
                format!(
                    "Playlist generated successfully!\nPath: {}\nEntries: {entries}",
                    output.display()
                ),
            )
        } else {
            self.status = "No playlist generated.".into();
            // A write failure is worth naming; everything else means nothing matched.
            let text = match r.error.as_deref() {
                Some(e) if e.starts_with("Error writing playlist") => e,
                _ => "No video files were found matching your parameters.",
            };
            msg(Kind::Warning, "No Playlist Generated", text)
        }
    }

    // --- Layouts ---

    pub fn new_layout(&mut self) {
        self.dirs.clear();
        self.show_info.clear();
        self.selected.clear();
        self.groups.clear();
        self.rows.clear();
        self.anchor = None;
        self.dir_sel = None;
        self.output_path.clear();
        self.relative = false;
        self.repeat = false;
        self.show_order = ShowOrder::Alpha;
        self.mode = Mode::None;
        self.block_size = "10".into();
        self.store.active_layout_name = None;
        // Don't replace an unreadable layout file without an explicit save.
        if !self.store.load_failed()
            && let Err(e) = self.store.save_all()
        {
            eprintln!("{e}");
        }
        self.status = "Created new layout.".into();
    }

    /// Save under `name` (the Save As dialog has already confirmed any overwrite).
    pub fn save_as(&mut self, name: &str) -> Result<(), Msg> {
        let state = self.current_state();
        match self.store.save_layout(name, &state) {
            Ok(()) => {
                self.status = format!("Saved layout: {name}");
                Ok(())
            }
            Err(e) => {
                eprintln!("{e}");
                self.status = "Save failed.".into();
                Err(msg(
                    Kind::Error,
                    "Error",
                    format!("Failed to save layout: {name}"),
                ))
            }
        }
    }

    pub fn layout_names(&self) -> Vec<String> {
        self.store.layouts.keys().cloned().collect()
    }

    pub fn load_layout(&mut self, name: &str) {
        if let Some(state) = self.store.get_layout(name) {
            self.store.active_layout_name = Some(name.to_string());
            if let Err(e) = self.store.save_all() {
                eprintln!("{e}");
            }
            self.apply_state(&state);
            self.status = format!("Loaded layout: {name}");
        }
    }

    pub fn delete_layout(&mut self, name: &str) {
        if let Err(e) = self.store.delete_layout(name) {
            eprintln!("{e}");
        }
        self.status = format!("Deleted layout: {name}");
        match self.store.get_active_state() {
            Some(state) => self.apply_state(&state),
            None => self.new_layout(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, GuiState) {
        let t = TempDir::new().unwrap();
        for f in [
            "BreakingBad/s01e01.mp4",
            "BetterCallSaul/s01e01.mp4",
            "Columbo/a.mkv",
        ] {
            let p = t.path().join("tv").join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        let s = GuiState::new(LayoutStore::new(t.path().join("l.json")));
        (t, s)
    }

    #[test]
    fn apply_and_extract_state() {
        let (t, mut s) = setup();
        let dir = t.path().join("tv").to_string_lossy().into_owned();
        let state = State {
            dirs: vec![dir.clone()],
            selected: vec!["BreakingBad".into(), "Gone".into()],
            groups: vec![vec!["BreakingBad".into(), "Gone".into()]],
            output_path: "/tmp/test.m3u".into(),
            relative: true,
            repeat: true,
            show_order: ShowOrder::Random,
            interleave_mode: Mode::Episodes,
            block_size: 7,
        };
        s.apply_state(&state);
        let cur = s.current_state();
        assert_eq!(cur.dirs, [dir]);
        assert_eq!(cur.selected, ["BreakingBad"]);
        assert_eq!(cur.groups, [["BreakingBad"]]);
        assert_eq!(cur.block_size, 7);
        assert_eq!(
            (cur.output_path.as_str(), cur.relative, cur.repeat),
            ("/tmp/test.m3u", true, true)
        );

        // Saved layouts are snapshots.
        s.store.save_layout("Mutated", &s.current_state()).unwrap();
        s.groups.push(vec!["X".into()]);
        assert_eq!(
            s.store.get_layout("Mutated").unwrap().groups,
            [["BreakingBad"]]
        );
    }

    #[test]
    fn recovers_last_session_but_not_first_layout() {
        let t = TempDir::new().unwrap();
        let f = t.path().join("l.json");
        let mut store = LayoutStore::new(f.clone());
        store.layouts.insert(
            "Other".into(),
            serde_json::to_value(State {
                repeat: true,
                ..Default::default()
            })
            .unwrap(),
        );
        store.save_all().unwrap();
        assert!(!GuiState::new(LayoutStore::new(f.clone())).repeat);

        let mut store = LayoutStore::new(f.clone());
        store
            .autosave_session(&State {
                output_path: "/tmp/r.m3u".into(),
                block_size: 12,
                ..Default::default()
            })
            .unwrap();
        let s = GuiState::new(LayoutStore::new(f));
        assert_eq!(s.store.active_layout_name, None);
        assert_eq!(
            (s.output_path.as_str(), s.block_size.as_str()),
            ("/tmp/r.m3u", "12")
        );
        assert_eq!(s.title(), "TV Playlist Interleaver — [Unsaved Layout]");
    }

    #[test]
    fn directories_and_rows() {
        let (t, mut s) = setup();
        s.add_directory(t.path().join("tv")).unwrap();
        assert_eq!(s.show_info.len(), 3);
        assert_eq!(s.selected.len(), 3, "new shows auto-selected");
        let e = s.add_directory(t.path().join("tv/../tv")).unwrap_err();
        assert_eq!(e.title, "Directory exists");

        // Rows: BetterCallSaul, BreakingBad, Columbo.
        s.click_row(0, false, false);
        s.click_row(2, false, true);
        assert_eq!(s.row_labels().len(), 3);
        s.click_row(1, true, false);
        assert_eq!(s.row_labels(), ["BetterCallSaul", "Columbo"]);
        s.click_row(1, false, false);
        assert_eq!(s.row_labels(), ["BreakingBad"]);

        s.toggle_show_selected();
        assert!(!s.selected.contains("BreakingBad"));
        assert_eq!(s.group_text("BreakingBad"), "");

        assert_eq!(
            s.remove_directory().unwrap_err().title,
            "Selection Required"
        );
        s.dir_sel = Some(0);
        s.remove_directory().unwrap();
        assert!(s.show_info.is_empty() && s.rows.is_empty());
    }

    #[test]
    fn group_actions_validate_like_python() {
        let (t, mut s) = setup();
        s.add_directory(t.path().join("tv")).unwrap();
        assert_eq!(
            s.create_group_from_selected().unwrap_err().text,
            "Please select 2 or more shows to create a group."
        );
        assert_eq!(
            s.can_add_to_group().unwrap_err().title,
            "Selection Required"
        );
        assert_eq!(
            s.can_dissolve().unwrap_err().text,
            "No groups are currently defined."
        );

        s.click_row(0, false, false);
        s.click_row(1, true, false);
        s.create_group_from_selected().unwrap();
        assert_eq!(s.groups, [["BetterCallSaul", "BreakingBad"]]);
        assert_eq!(s.group_text("BreakingBad"), "Group 1");
        assert_eq!(s.status, "Created group with 2 shows.");

        s.click_row(2, false, false);
        s.can_add_to_group().unwrap();
        s.add_selected_to_group(0);
        assert_eq!(s.groups, [["BetterCallSaul", "BreakingBad", "Columbo"]]);

        s.selected.remove("Columbo");
        assert_eq!(
            s.can_add_to_group().unwrap_err().text,
            "Show 'Columbo' must be checked ('Use?') before grouping."
        );
        s.remove_selected_from_group().unwrap();
        s.dissolve_group(0);
        assert!(s.groups.is_empty());
    }

    #[test]
    fn generation_checks_and_job() {
        let (t, mut s) = setup();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Please add at least one media directory first."
        );
        s.add_directory(t.path().join("tv")).unwrap();
        s.selected.clear();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Please select/check ('Use?') at least one show."
        );
        s.selected.insert("Columbo".into());
        s.mode = Mode::Episodes;
        s.block_size = "0".into();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Block Size must be a positive integer."
        );
        s.block_size = " 3 ".into();
        let job = s.prepare_generation().ok().unwrap();
        assert_eq!(job.opts.block_size, 3);
        assert_eq!(job.output, s.dirs[0].join("interleaved.m3u"));
        assert_eq!(
            job.opts.include,
            Some(HashSet::from(["Columbo".to_string()]))
        );
        assert_eq!(s.status, "Generating playlist...");

        let r = crate::build::build_playlist(&job.dirs, &job.output, &job.opts, &mut rand::rng());
        let m = s.generation_done(&job.output, &r);
        assert_eq!(m.title, "Success");
        assert_eq!(s.status, "Generated 1 entries into: interleaved.m3u");
    }

    #[test]
    fn layout_lifecycle() {
        let (t, mut s) = setup();
        s.add_directory(t.path().join("tv")).unwrap();
        s.save_as("Mine").unwrap();
        assert_eq!(s.title(), "TV Playlist Interleaver — [Mine]");
        s.new_layout();
        assert!(s.dirs.is_empty() && s.store.active_layout_name.is_none());
        s.load_layout("Mine");
        assert_eq!(s.show_info.len(), 3);
        assert_eq!(s.status, "Loaded layout: Mine");
        s.delete_layout("Mine");
        // "Last Session State" remains and becomes active.
        assert_eq!(
            s.store.active_layout_name.as_deref(),
            Some(layout::LAST_SESSION)
        );
        s.delete_layout(layout::LAST_SESSION);
        assert_eq!(s.status, "Created new layout.");
        assert_eq!(s.title(), "TV Playlist Interleaver — [Unsaved Layout]");
    }
}

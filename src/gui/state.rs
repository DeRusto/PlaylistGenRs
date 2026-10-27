//! GUI state and actions, kept free of egui so they can be unit tested.
//! Mirrors the Python `InterleaverGUI` methods; rendering lives in `app.rs`.

use crate::build::{BuildOptions, BuildReport, Mode};
use crate::groups;
use crate::layout::{self, LayoutStore};
use crate::rotation::{self, Slot};
use crate::session::{DirError, Session};
use std::collections::{HashMap, HashSet};
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
    pub session: Session,
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
            session: Session::open(store),
            block_size: String::new(),
            rows: HashSet::new(),
            anchor: None,
            dir_sel: None,
            status: "Ready".into(),
        };
        s.reset_ui();
        s
    }

    /// Clear UI state after the session's layout was replaced.
    fn reset_ui(&mut self) {
        self.block_size = self.session.block_size.to_string();
        self.rows.clear();
        self.anchor = None;
        self.dir_sel = None;
    }

    /// After a rescan: forget highlighted rows whose shows are gone.
    fn after_rescan(&mut self) {
        let all = layout::labels(&self.session.show_info);
        self.rows.retain(|l| all.contains(l));
        self.anchor = None;
    }

    /// Print and clear the session's warnings.
    pub fn print_warnings(&mut self) {
        for w in self.session.warnings.drain(..) {
            eprintln!("{w}");
        }
    }

    pub fn title(&self) -> String {
        let name = self
            .session
            .store
            .active_layout_name
            .as_deref()
            .unwrap_or("Unsaved Layout");
        format!("TV Playlist Interleaver — [{name}]")
    }

    /// Save to the active layout, or to the session slot when none is active.
    pub fn auto_save(&mut self) {
        self.session.block_size = layout::parse_block_size(&self.block_size);
        self.session.autosave();
    }

    // --- Directories ---

    pub fn add_directory(&mut self, chosen: PathBuf) -> Result<(), Msg> {
        match self.session.add_dir(&chosen) {
            Ok(p) => {
                self.after_rescan();
                self.status = format!("Added directory: {}", p.display());
                self.auto_save();
                Ok(())
            }
            Err(DirError::AlreadyAdded(p)) => Err(msg(
                Kind::Info,
                "Directory exists",
                format!(
                    "Directory is already added:
{}",
                    p.display()
                ),
            )),
            Err(DirError::NotADirectory(p)) => Err(msg(
                Kind::Error,
                "Error",
                format!(
                    "Not a directory:
{}",
                    p.display()
                ),
            )),
        }
    }

    pub fn remove_directory(&mut self) -> Result<(), Msg> {
        let Some(removed) = self.dir_sel.and_then(|i| self.session.remove_dir(i)) else {
            return Err(msg(
                Kind::Warning,
                "Selection Required",
                "Please select a directory to remove.",
            ));
        };
        self.dir_sel = None;
        self.after_rescan();
        self.status = format!("Removed directory: {}", removed.display());
        self.auto_save();
        Ok(())
    }

    // --- Show table ---

    /// Highlighted labels in table order.
    pub fn row_labels(&self) -> Vec<String> {
        self.session
            .show_info
            .iter()
            .filter(|s| self.rows.contains(&s.label))
            .map(|s| s.label.clone())
            .collect()
    }

    /// Row click: plain selects one, ctrl toggles, shift selects a range from the anchor.
    pub fn click_row(&mut self, idx: usize, ctrl: bool, shift: bool) {
        let Some(label) = self.session.show_info.get(idx).map(|s| s.label.clone()) else {
            return;
        };
        match (shift, self.anchor) {
            (true, Some(a)) => {
                if !ctrl {
                    self.rows.clear();
                }
                let (lo, hi) = (a.min(idx), a.max(idx));
                self.rows.extend(
                    self.session.show_info[lo..=hi]
                        .iter()
                        .map(|s| s.label.clone()),
                );
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

    /// Text in the Group column: only for used shows, naming the group they play in.
    pub fn group_text(&self, label: &str) -> String {
        if !self.session.selected.contains(label) {
            return String::new();
        }
        match self.group_of(label) {
            Some(g) => format!("Group {}", g + 1),
            None => String::new(),
        }
    }

    /// Space / double-click: flip "Use?" on highlighted rows.
    pub fn toggle_show_selected(&mut self) {
        for l in self.row_labels() {
            if !self.session.selected.remove(&l) {
                self.session.selected.insert(l);
            }
        }
        self.auto_save();
    }

    fn check_in_use(&self, labels: &[String]) -> Result<(), Msg> {
        match labels.iter().find(|l| !self.session.selected.contains(*l)) {
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
        groups::create(&mut self.session.groups, &labels);
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
        if self.session.groups.is_empty() {
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
        groups::add(&mut self.session.groups, g_idx, &labels);
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
        groups::remove(&mut self.session.groups, &labels);
        self.status = "Removed selected shows from their groups.".into();
        self.auto_save();
        Ok(())
    }

    pub fn can_dissolve(&self) -> Result<(), Msg> {
        if self.session.groups.is_empty() {
            return Err(msg(
                Kind::Info,
                "No Groups",
                "No groups are currently defined.",
            ));
        }
        Ok(())
    }

    pub fn dissolve_group(&mut self, g_idx: usize) {
        if g_idx < self.session.groups.len() {
            groups::dissolve(&mut self.session.groups, g_idx);
            self.status = format!("Dissolved Group {}.", g_idx + 1);
            self.auto_save();
        }
    }

    // --- Balance view ---

    fn group_of(&self, label: &str) -> Option<usize> {
        groups::group_of(&self.session.groups, label)
    }

    /// Slots of used shows in the order the build places them, each with its members'
    /// sizes (seasons in seasons mode, else episodes) and its turns.
    pub fn balance_slots(&self) -> Vec<(Slot, Vec<usize>, usize)> {
        let sizes: HashMap<&str, usize> = self
            .session
            .show_info
            .iter()
            .filter(|s| self.session.selected.contains(&s.label))
            .map(|s| (s.label.as_str(), rotation::size(s, self.session.mode)))
            .collect();
        let used: Vec<&str> = self
            .session
            .show_info
            .iter()
            .map(|s| s.label.as_str())
            .filter(|l| sizes.contains_key(l))
            .collect();
        let block_size = layout::parse_block_size(&self.block_size);
        rotation::slots(&used, &self.session.groups)
            .into_iter()
            .map(|slot| {
                let member_sizes: Vec<usize> =
                    slot.members.iter().map(|l| sizes[l.as_str()]).collect();
                let turns =
                    rotation::turns(member_sizes.iter().sum(), self.session.mode, block_size);
                (slot, member_sizes, turns)
            })
            .collect()
    }

    /// Balance-view drop of the used show `label` onto the slot holding `onto`, or onto
    /// empty space (`None`). Dropping onto its own slot, or dropping an ungrouped show on
    /// empty space, does nothing.
    pub fn drop_show(&mut self, label: &str, onto: Option<&str>) {
        let from = self.group_of(label);
        match onto {
            None => {
                if from.is_none() {
                    return;
                }
                groups::remove(&mut self.session.groups, &[label.to_string()]);
                self.status = format!("Removed '{label}' from its group.");
            }
            Some(target) if target == label => return,
            Some(target) => match self.group_of(target) {
                Some(g) if from == Some(g) => return,
                Some(g) => {
                    groups::add(&mut self.session.groups, g, &[label.to_string()]);
                    let g = self.group_of(label).expect("just added");
                    self.status = format!("Added '{label}' to Group {}.", g + 1);
                }
                None => {
                    // Show order, like "Create Group from Selected".
                    let pair: Vec<String> = self
                        .session
                        .show_info
                        .iter()
                        .filter(|s| s.label == label || s.label == target)
                        .map(|s| s.label.clone())
                        .collect();
                    groups::create(&mut self.session.groups, &pair);
                    self.status = format!("Created group with '{target}' and '{label}'.");
                }
            },
        }
        self.auto_save();
    }

    /// Pack ungrouped used shows into new groups lasting no more turns than the longest slot.
    /// Existing groups are left alone; new ones are appended.
    pub fn auto_sort(&mut self) {
        let slots = self.balance_slots();
        let target = slots.iter().map(|(_, _, t)| *t).max().unwrap_or(0);
        let shows: Vec<(String, usize)> = slots
            .into_iter()
            .filter(|(s, _, _)| s.group.is_none())
            .filter_map(|(s, h, _)| s.members.into_iter().zip(h).next())
            .collect();
        let block_size = layout::parse_block_size(&self.block_size);
        let new = rotation::auto_sort(&shows, target, self.session.mode, block_size);
        self.status = match new.len() {
            0 => "Auto-sort found no shows that fit together.".into(),
            1 => "Auto-sort created 1 group.".into(),
            n => format!("Auto-sort created {n} groups."),
        };
        self.session.groups.extend(new);
        self.auto_save();
    }

    /// Dissolve every group, then auto-sort all used shows (target: the tallest show).
    /// The app confirms first.
    pub fn wipe_and_auto_sort(&mut self) {
        self.session.groups.clear();
        self.auto_sort();
    }

    // --- Generation ---

    pub fn prepare_generation(&mut self) -> Result<Job, Msg> {
        let err = |t: &str| msg(Kind::Error, "Error", t);
        if self.session.dirs.is_empty() {
            return Err(err("Please add at least one media directory first."));
        }
        if self.session.selected.is_empty() {
            return Err(err("Please select/check ('Use?') at least one show."));
        }
        if self.session.mode == Mode::Episodes {
            let raw = self.block_size.trim();
            match raw.parse::<usize>() {
                Ok(n) if n > 0 && raw.bytes().all(|b| b.is_ascii_digit()) => {
                    self.session.block_size = n
                }
                _ => return Err(err("Block Size must be a positive integer.")),
            }
        }
        self.status = "Generating playlist...".into();
        Ok(Job {
            dirs: self.session.dirs.clone(),
            output: self.session.output().expect("checked dirs above"),
            opts: self.session.build_options(),
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
        self.session.new_layout();
        self.reset_ui();
        self.status = "Created new layout.".into();
    }

    /// Save under `name` (the Save As dialog has already confirmed any overwrite).
    pub fn save_as(&mut self, name: &str) -> Result<(), Msg> {
        self.session.block_size = layout::parse_block_size(&self.block_size);
        match self.session.save_as(name) {
            Ok(()) => {
                self.status = format!("Saved layout: {name}");
                Ok(())
            }
            Err(e) => {
                self.session.warnings.push(e);
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
        self.session.store.layouts.keys().cloned().collect()
    }

    pub fn load_layout(&mut self, name: &str) {
        if self.session.load(name) {
            self.reset_ui();
            self.status = format!("Loaded layout: {name}");
        }
    }

    pub fn delete_layout(&mut self, name: &str) {
        self.session.delete(name);
        self.reset_ui();
        self.status = match self.session.store.active_layout_name {
            Some(_) => format!("Deleted layout: {name}"),
            None => "Created new layout.".into(),
        };
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
    fn directories_and_rows() {
        let (t, mut s) = setup();
        s.add_directory(t.path().join("tv")).unwrap();
        assert_eq!(s.session.show_info.len(), 3);
        assert_eq!(s.session.selected.len(), 3, "new shows auto-selected");
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
        assert!(!s.session.selected.contains("BreakingBad"));
        assert_eq!(s.group_text("BreakingBad"), "");

        assert_eq!(
            s.remove_directory().unwrap_err().title,
            "Selection Required"
        );
        s.dir_sel = Some(0);
        s.remove_directory().unwrap();
        assert!(s.session.show_info.is_empty() && s.rows.is_empty());
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
        assert_eq!(s.session.groups, [["BetterCallSaul", "BreakingBad"]]);
        assert_eq!(s.group_text("BreakingBad"), "Group 1");
        assert_eq!(s.status, "Created group with 2 shows.");

        s.click_row(2, false, false);
        s.can_add_to_group().unwrap();
        s.add_selected_to_group(0);
        assert_eq!(
            s.session.groups,
            [["BetterCallSaul", "BreakingBad", "Columbo"]]
        );

        s.session.selected.remove("Columbo");
        assert_eq!(
            s.can_add_to_group().unwrap_err().text,
            "Show 'Columbo' must be checked ('Use?') before grouping."
        );
        s.remove_selected_from_group().unwrap();
        s.dissolve_group(0);
        assert!(s.session.groups.is_empty());
    }

    /// Alpha: 3 episodes in 2 seasons; Beta: 1 in 1; Gamma: 4 in 1; Delta: 2 in 2.
    fn balance_setup() -> (TempDir, GuiState) {
        let t = TempDir::new().unwrap();
        for f in [
            "Alpha/S1/e1.mkv",
            "Alpha/S1/e2.mkv",
            "Alpha/S2/e1.mkv",
            "Beta/e1.mkv",
            "Gamma/S1/e1.mkv",
            "Gamma/S1/e2.mkv",
            "Gamma/S1/e3.mkv",
            "Gamma/S1/e4.mkv",
            "Delta/S1/e1.mkv",
            "Delta/S2/e1.mkv",
        ] {
            let p = t.path().join("tv").join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "").unwrap();
        }
        let mut s = GuiState::new(LayoutStore::new(t.path().join("l.json")));
        s.add_directory(t.path().join("tv")).unwrap();
        (t, s)
    }

    /// Slots as "label height" members joined by " + ", for compact asserts.
    fn slots(s: &GuiState) -> Vec<String> {
        s.balance_slots()
            .iter()
            .map(|(b, h, _)| {
                let m: Vec<String> = b
                    .members
                    .iter()
                    .zip(h)
                    .map(|(l, h)| format!("{l} {h}"))
                    .collect();
                m.join(" + ")
            })
            .collect()
    }

    fn own(v: &[&str]) -> Vec<String> {
        v.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn balance_slots_follow_build_order_and_mode() {
        let (_t, mut s) = balance_setup();
        s.session.groups = vec![own(&["Gamma", "Beta"])];
        s.session.selected.remove("Delta");
        // The group sits where its first member in show order (Beta) would be.
        assert_eq!(slots(&s), ["Alpha 3", "Gamma 4 + Beta 1"]);
        let b = s.balance_slots();
        assert_eq!((b[0].0.group, b[1].0.group), (None, Some(0)));
        assert_eq!(b[1].2, 5);

        s.session.mode = Mode::Seasons;
        assert_eq!(slots(&s), ["Alpha 2", "Gamma 1 + Beta 1"]);

        // Episodes mode: a slot's turns are its whole episode count in blocks.
        s.session.mode = Mode::Episodes;
        s.block_size = "2".into();
        let turns: Vec<usize> = s.balance_slots().iter().map(|(_, _, t)| *t).collect();
        assert_eq!(turns, [2, 3]);

        // Unused members drop out of their group's slot; all-unused groups vanish.
        s.session.selected.remove("Beta");
        assert_eq!(slots(&s), ["Alpha 3", "Gamma 4"]);
        s.session.selected.remove("Gamma");
        assert_eq!(slots(&s), ["Alpha 3"]);
    }

    #[test]
    fn dropping_shows_edits_groups_and_saves() {
        let (_t, mut s) = balance_setup();
        let saved = |s: &GuiState| {
            s.session
                .store
                .get_layout(layout::LAST_SESSION)
                .unwrap()
                .groups
        };

        // Single onto single: a new group, in show order.
        s.drop_show("Gamma", Some("Alpha"));
        assert_eq!(s.session.groups, [["Alpha", "Gamma"]]);
        assert_eq!(saved(&s), s.session.groups);
        // Onto any member of a group: joins it.
        s.drop_show("Beta", Some("Gamma"));
        assert_eq!(s.session.groups, [["Alpha", "Gamma", "Beta"]]);

        // No-ops: onto its own slot, and an ungrouped show onto empty space.
        s.drop_show("Beta", Some("Alpha"));
        s.drop_show("Beta", Some("Beta"));
        s.drop_show("Delta", None);
        assert_eq!(s.session.groups, [["Alpha", "Gamma", "Beta"]]);

        // Out onto empty space: leaves the group; a lone member stays a 1-show group.
        s.drop_show("Gamma", None);
        s.drop_show("Alpha", None);
        assert_eq!(s.session.groups, [["Beta"]]);
        assert_eq!(s.balance_slots()[1].0.group, Some(0));
        assert_eq!(saved(&s), s.session.groups);

        // Moving between groups keeps the target group's position.
        s.drop_show("Delta", Some("Beta"));
        s.drop_show("Alpha", Some("Gamma"));
        s.drop_show("Delta", Some("Alpha"));
        assert_eq!(
            s.session.groups,
            [own(&["Beta"]), own(&["Alpha", "Gamma", "Delta"])]
        );
        assert_eq!(s.status, "Added 'Delta' to Group 2.");
    }

    #[test]
    fn auto_sort_packs_ungrouped_shows_and_saves() {
        let (_t, mut s) = balance_setup();
        let saved = |s: &GuiState| {
            s.session
                .store
                .get_layout(layout::LAST_SESSION)
                .unwrap()
                .groups
        };

        // Episodes, target 4 (Gamma): Gamma | Alpha+Beta | Delta.
        s.auto_sort();
        assert_eq!(s.session.groups, [["Alpha", "Beta"]]);
        assert_eq!(s.status, "Auto-sort created 1 group.");
        assert_eq!(saved(&s), s.session.groups);

        // Seasons heights, target 2: Alpha | Delta | Beta+Gamma.
        s.session.groups.clear();
        s.session.mode = Mode::Seasons;
        s.auto_sort();
        assert_eq!(s.session.groups, [["Beta", "Gamma"]]);

        // Existing groups are kept, and count toward the target (6 here).
        s.session.mode = Mode::None;
        s.session.groups = vec![own(&["Gamma", "Delta"])];
        s.auto_sort();
        assert_eq!(
            s.session.groups,
            [own(&["Gamma", "Delta"]), own(&["Alpha", "Beta"])]
        );

        // Nothing left to sort.
        s.auto_sort();
        assert_eq!(s.session.groups.len(), 2);
        assert_eq!(s.status, "Auto-sort found no shows that fit together.");

        // Wipe: target drops back to the tallest show (4).
        s.wipe_and_auto_sort();
        assert_eq!(s.session.groups, [["Alpha", "Beta"]]);
        assert_eq!(saved(&s), s.session.groups);

        // Wipe still clears when nothing forms.
        s.session.selected = HashSet::from(["Gamma".to_string()]);
        s.wipe_and_auto_sort();
        assert!(s.session.groups.is_empty() && saved(&s).is_empty());
    }

    #[test]
    fn generation_checks_and_job() {
        let (t, mut s) = setup();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Please add at least one media directory first."
        );
        s.add_directory(t.path().join("tv")).unwrap();
        s.session.selected.clear();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Please select/check ('Use?') at least one show."
        );
        s.session.selected.insert("Columbo".into());
        s.session.mode = Mode::Episodes;
        s.block_size = "0".into();
        assert_eq!(
            s.prepare_generation().err().unwrap().text,
            "Block Size must be a positive integer."
        );
        s.block_size = " 3 ".into();
        let job = s.prepare_generation().ok().unwrap();
        assert_eq!(job.opts.block_size, 3);
        assert_eq!(job.output, s.session.dirs[0].join("interleaved.m3u"));
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
        assert!(s.session.dirs.is_empty() && s.session.store.active_layout_name.is_none());
        s.load_layout("Mine");
        assert_eq!(s.session.show_info.len(), 3);
        assert_eq!(s.status, "Loaded layout: Mine");
        s.delete_layout("Mine");
        // "Last Session State" remains and becomes active.
        assert_eq!(
            s.session.store.active_layout_name.as_deref(),
            Some(layout::LAST_SESSION)
        );
        s.delete_layout(layout::LAST_SESSION);
        assert_eq!(s.status, "Created new layout.");
        assert_eq!(s.title(), "TV Playlist Interleaver — [Unsaved Layout]");
    }
}

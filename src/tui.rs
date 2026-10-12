//! Interactive text menu (Python `interactive_mode`), line for line, over any reader/writer.

use crate::build::{BuildOptions, Mode, ShowOrder, build_playlist};
use crate::layout::{self, LayoutStore, State};
use crate::scan::{ShowInfo, refresh_shows};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::PathBuf;

const CLEAR: &str = "\x1b[2J\x1b[H";

/// Run the menu against the user's layout file. Returns the exit code.
pub fn run_menu(input: &mut impl BufRead, out: &mut impl Write) -> i32 {
    run_menu_with(LayoutStore::open_default(), input, out)
}

/// Run the menu with an explicit store. Quits with 0 on `q` or end of input.
pub fn run_menu_with(store: LayoutStore, input: &mut impl BufRead, out: &mut impl Write) -> i32 {
    Menu::new(store, input, out).main_loop();
    0
}

/// `None` means end of input (or a broken output): unwind and quit.
type Step<T = ()> = Option<T>;

struct Menu<'a, R, W> {
    input: &'a mut R,
    out: &'a mut W,
    store: LayoutStore,
    dirs: Vec<PathBuf>,
    output: Option<PathBuf>,
    relative: bool,
    repeat: bool,
    mode: Mode,
    block_size: usize,
    show_order: ShowOrder,
    show_info: Vec<ShowInfo>,
    selected: HashSet<String>,
    groups: Vec<Vec<String>>,
}

macro_rules! say {
    ($m:expr) => { writeln!($m.out).ok()? };
    ($m:expr, $($arg:tt)*) => { writeln!($m.out, $($arg)*).ok()? };
}

fn plural(n: usize, word: &str, pad: &str) -> String {
    format!("{n} {word}{}", if n != 1 { "s" } else { pad })
}

fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// 1-based digit token → 0-based index below `len`.
fn index(tok: &str, len: usize) -> Option<usize> {
    if !is_digits(tok) {
        return None;
    }
    tok.parse::<usize>()
        .ok()?
        .checked_sub(1)
        .filter(|&i| i < len)
}

impl<'a, R: BufRead, W: Write> Menu<'a, R, W> {
    fn new(store: LayoutStore, input: &'a mut R, out: &'a mut W) -> Self {
        let state = store.startup_state(true);
        let mut m = Menu {
            input,
            out,
            store,
            dirs: vec![],
            output: None,
            relative: false,
            repeat: false,
            mode: Mode::None,
            block_size: 10,
            show_order: ShowOrder::Alpha,
            show_info: vec![],
            selected: HashSet::new(),
            groups: vec![],
        };
        for w in std::mem::take(&mut m.store.warnings) {
            let _ = writeln!(m.out, "{w}");
        }
        if let Some(s) = &state {
            m.dirs = s
                .dirs
                .iter()
                .filter(|d| !d.is_empty())
                .map(PathBuf::from)
                .collect();
            m.output = (!s.output_path.is_empty())
                .then(|| crate::build::resolve_user_path(&s.output_path));
            m.relative = s.relative;
            m.repeat = s.repeat;
            m.mode = s.interleave_mode;
            m.block_size = s.block_size;
            m.show_order = s.show_order;
        }
        m.show_info = m.refresh();
        let all = layout::labels(&m.show_info);
        match state {
            Some(s) => {
                m.selected = s.selected.into_iter().filter(|l| all.contains(l)).collect();
                m.groups = layout::filter_groups(&s.groups, &all);
            }
            None => m.selected = all,
        }
        m
    }

    fn refresh(&mut self) -> Vec<ShowInfo> {
        let mut warnings = vec![];
        let info = refresh_shows(&self.dirs, &mut warnings);
        for w in warnings {
            let _ = writeln!(self.out, "{w}");
        }
        info
    }

    fn input(&mut self, prompt: &str) -> Step<String> {
        write!(self.out, "{prompt}").ok()?;
        self.out.flush().ok()?;
        let mut line = String::new();
        if self.input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        Some(line.trim().to_string())
    }

    fn clear(&mut self) -> Step {
        write!(self.out, "{CLEAR}").ok()
    }

    fn save_state(&mut self) {
        let state = State {
            dirs: self
                .dirs
                .iter()
                .map(|d| d.to_string_lossy().into_owned())
                .collect(),
            selected: layout::selected_list(&self.selected, &self.show_info),
            groups: self.groups.clone(),
            output_path: self
                .output
                .as_ref()
                .map(|o| o.to_string_lossy().into_owned())
                .unwrap_or_default(),
            relative: self.relative,
            repeat: self.repeat,
            show_order: self.show_order,
            interleave_mode: self.mode,
            block_size: self.block_size,
        };
        if let Err(e) = self.store.autosave(&state) {
            let _ = writeln!(self.out, "{e}");
        }
    }

    fn rescan(&mut self) {
        let old = layout::labels(&self.show_info);
        self.show_info = self.refresh();
        layout::rescan(&old, &self.show_info, &mut self.selected, &mut self.groups);
        self.save_state();
    }

    fn print_menu(&mut self) -> Step {
        self.clear()?;
        let rule = "=".repeat(54);
        say!(self, "{rule}");
        say!(self, "         TV Playlist Interleaver");
        say!(self, "{rule}");
        say!(self);
        say!(self, "Source directories:");
        if self.dirs.is_empty() {
            say!(self, "  (none — add at least one with [a])");
        }
        for (i, d) in self.dirs.iter().enumerate() {
            writeln!(self.out, "  [{}] {}", i + 1, d.display()).ok()?;
        }
        say!(self);

        let out_str = match &self.output {
            Some(o) => o.display().to_string(),
            None => "(auto: interleaved.m3u in first source dir)".into(),
        };
        let mode_str = match self.mode {
            Mode::None => "1 episode per show".to_string(),
            Mode::Episodes => format!("{} episodes per show", self.block_size),
            Mode::Seasons => "1 season per show".to_string(),
        };
        let n_total = self.show_info.len();
        let n_sel = self
            .show_info
            .iter()
            .filter(|s| self.selected.contains(&s.label))
            .count();
        let shows_str = if n_total > 0 {
            format!("{n_total} found, {n_sel} selected")
        } else if !self.dirs.is_empty() {
            "scanning...".into()
        } else {
            "none".into()
        };
        let groups_str = match self.groups.len() {
            0 => "none".to_string(),
            n => format!("{n} defined"),
        };
        let order_str = if self.show_order == ShowOrder::Alpha {
            "alphabetical"
        } else {
            "random"
        };

        say!(self, "Settings:");
        say!(self, "  Shows       : {shows_str}");
        say!(self, "  Groups      : {groups_str}");
        say!(self, "  Output      : {out_str}");
        say!(
            self,
            "  Path style  : {}",
            if self.relative {
                "relative"
            } else {
                "absolute"
            }
        );
        say!(
            self,
            "  Repeat      : {}",
            if self.repeat { "on" } else { "off" }
        );
        say!(self, "  Show order  : {order_str}");
        say!(self, "  Mode        : {mode_str}");
        say!(self);
        say!(self, "Commands:");
        say!(self, "  [a] Add source directory");
        if !self.dirs.is_empty() {
            say!(self, "  [x] Remove source directory");
        }
        if !self.show_info.is_empty() {
            say!(self, "  [w] Select shows");
            say!(self, "  [G] Manage groups");
        }
        say!(self, "  [o] Set output file");
        say!(self, "  [p] Toggle path style  (absolute / relative)");
        say!(self, "  [r] Toggle repeat shorter shows");
        say!(self, "  [n] Toggle show order  (alphabetical / random)");
        say!(self, "  [s] Set interleave mode");
        if !self.dirs.is_empty() && !self.selected.is_empty() {
            say!(self, "  [g] Generate playlist");
        }
        say!(self, "  [q] Quit");
        say!(self);
        Some(())
    }

    fn main_loop(&mut self) -> Step {
        loop {
            self.print_menu()?;
            let raw_choice = self.input("Choice: ")?;
            let choice = raw_choice.to_lowercase();

            if choice == "a" {
                let raw = self.input("Directory path: ")?;
                if !raw.is_empty() {
                    let p = crate::build::resolve_user_path(&raw);
                    if p.is_dir() {
                        self.dirs.push(p);
                        write!(self.out, "Scanning...\r").ok()?;
                        self.out.flush().ok()?;
                        self.rescan();
                    } else {
                        self.input(&format!(
                            "'{}' is not a valid directory. Press Enter...",
                            p.display()
                        ))?;
                    }
                }
            } else if choice == "x" && !self.dirs.is_empty() {
                if self.dirs.len() == 1 {
                    self.dirs.clear();
                    self.show_info.clear();
                    self.selected.clear();
                    self.groups.clear();
                    self.save_state();
                } else {
                    self.clear()?;
                    say!(self, "Remove which directory?\n");
                    for (i, d) in self.dirs.iter().enumerate() {
                        writeln!(self.out, "  [{}] {}", i + 1, d.display()).ok()?;
                    }
                    let raw = self.input("\nNumber (Enter to cancel): ")?;
                    if let Some(i) = index(&raw, self.dirs.len()) {
                        self.dirs.remove(i);
                        write!(self.out, "Scanning...\r").ok()?;
                        self.out.flush().ok()?;
                        self.rescan();
                    }
                }
            } else if choice == "w" && !self.show_info.is_empty() {
                // Save even if input ends (Ctrl-D) inside the submenu.
                let step = self.selection_screen();
                self.save_state();
                step?;
            } else if raw_choice == "G" && !self.show_info.is_empty() {
                // Save even if input ends (Ctrl-D) inside the submenu.
                let step = self.groups_screen();
                self.save_state();
                step?;
            } else if choice == "o" {
                let raw = self.input("Output file path (Enter for auto): ")?;
                self.output = (!raw.is_empty()).then(|| crate::build::resolve_user_path(&raw));
                self.save_state();
            } else if choice == "p" {
                self.relative = !self.relative;
                self.save_state();
            } else if choice == "r" {
                self.repeat = !self.repeat;
                self.save_state();
            } else if choice == "n" {
                self.show_order = match self.show_order {
                    ShowOrder::Alpha => ShowOrder::Random,
                    ShowOrder::Random => ShowOrder::Alpha,
                };
                self.save_state();
            } else if choice == "s" {
                // Save even if input ends (Ctrl-D) inside the submenu.
                let step = self.mode_submenu();
                self.save_state();
                step?;
            } else if choice == "g" && !self.dirs.is_empty() && !self.selected.is_empty() {
                self.generate()?;
            } else if choice == "q" {
                return Some(());
            }
        }
    }

    fn generate(&mut self) -> Step {
        let out = self
            .output
            .clone()
            .unwrap_or_else(|| self.dirs[0].join("interleaved.m3u"));
        let include = self
            .show_info
            .iter()
            .filter(|s| self.selected.contains(&s.label))
            .map(|s| s.label.clone())
            .collect();
        say!(self);
        let opts = BuildOptions {
            relative: self.relative,
            repeat: self.repeat,
            mode: self.mode,
            block_size: self.block_size,
            include: Some(include),
            groups: self.groups.clone(),
            show_order: self.show_order,
        };
        let r = build_playlist(&self.dirs, &out, &opts, &mut rand::rng());
        for line in r.warnings.iter().chain(&r.summary_lines()).chain(&r.error) {
            say!(self, "{line}");
        }
        if r.entries > 0 {
            self.input(&format!(
                "\nPlaylist written: {}  ({} entries)\nPress Enter...",
                out.display(),
                r.entries
            ))?;
        } else {
            self.input("\nNo playlist generated. Press Enter...")?;
        }
        Some(())
    }

    fn mode_submenu(&mut self) -> Step {
        self.clear()?;
        let rule = "─".repeat(54);
        say!(self, "{rule}");
        say!(self, "  Interleave mode");
        say!(self, "{rule}");
        say!(
            self,
            "  [1] Default (1 ep)  1 episode per show, round-robin"
        );
        say!(
            self,
            "  [2] Block (N eps)   N consecutive episodes per show before switching"
        );
        say!(
            self,
            "  [3] Season          1 full season per show before switching"
        );
        say!(self, "  [b] Back            keep current setting");
        say!(self);
        let choice = self.input("Choice: ")?.to_lowercase();
        match choice.as_str() {
            "1" => self.mode = Mode::None,
            "2" => {
                let raw = self.input(&format!(
                    "Episodes per show per round [{}]: ",
                    self.block_size
                ))?;
                if is_digits(&raw)
                    && let Ok(n @ 1..) = raw.parse::<usize>()
                {
                    self.block_size = n;
                }
                self.mode = Mode::Episodes;
            }
            "3" => self.mode = Mode::Seasons,
            _ => {}
        }
        Some(())
    }

    fn show_row(&self, s: &ShowInfo, max_name: usize) -> String {
        let s_str = plural(s.n_seasons, "season", " ");
        let e_str = plural(s.n_episodes, "ep", " ");
        format!("{:<max_name$}  {s_str:>10}  {e_str:>7}", s.label)
    }

    fn selection_screen(&mut self) -> Step {
        let max_name = self
            .show_info
            .iter()
            .map(|s| s.label.chars().count())
            .max()
            .unwrap_or(10);
        loop {
            self.clear()?;
            let n_total = self.show_info.len();
            let n_sel = self
                .show_info
                .iter()
                .filter(|s| self.selected.contains(&s.label))
                .count();
            let rule = "─".repeat(54);
            say!(self, "{rule}");
            say!(self, "  Show Selection  ({n_sel}/{n_total} selected)");
            say!(self, "{rule}");
            say!(self);
            for (i, s) in self.show_info.iter().enumerate() {
                let mark = if self.selected.contains(&s.label) {
                    "✓"
                } else {
                    " "
                };
                let row = self.show_row(s, max_name);
                writeln!(self.out, "  [{mark}] {:>2}. {row}", i + 1).ok()?;
            }
            say!(self);
            say!(self, "  Enter number(s) to toggle (e.g. 3  or  1 4 7),");
            say!(self, "  [a] select all,  [n] select none,  [b] back");
            say!(self);
            let raw = self.input("  > ")?.to_lowercase();
            match raw.as_str() {
                "b" => return Some(()),
                "a" => self.selected = layout::labels(&self.show_info),
                "n" => self.selected.clear(),
                _ => {
                    for tok in raw.replace(',', " ").split_whitespace() {
                        if let Some(i) = index(tok, self.show_info.len()) {
                            let label = &self.show_info[i].label;
                            if !self.selected.remove(label) {
                                self.selected.insert(label.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn groups_screen(&mut self) -> Step {
        loop {
            let visible: Vec<ShowInfo> = self
                .show_info
                .iter()
                .filter(|s| self.selected.contains(&s.label))
                .cloned()
                .collect();
            if visible.is_empty() {
                self.input("No shows selected. Press Enter...")?;
                return Some(());
            }
            let label_to_g = |label: &str| {
                self.groups
                    .iter()
                    .position(|g| g.iter().any(|l| l == label))
            };
            let max_name = visible
                .iter()
                .map(|s| s.label.chars().count())
                .max()
                .unwrap_or(10);
            let n_groups = self.groups.len();
            let mut rows = Vec::new();
            for (i, s) in visible.iter().enumerate() {
                let tag = match label_to_g(&s.label) {
                    Some(g) => format!("G{}", g + 1),
                    None => "●".into(),
                };
                rows.push(format!(
                    "  {tag:<3} {:>2}. {}",
                    i + 1,
                    self.show_row(s, max_name)
                ));
            }

            self.clear()?;
            let rule = "─".repeat(54);
            say!(self, "{rule}");
            let g_word = if n_groups > 0 {
                plural(n_groups, "group", "")
            } else {
                "no groups".into()
            };
            say!(self, "  Show Groups  ({g_word})");
            say!(self, "{rule}");
            say!(self);
            for r in rows {
                say!(self, "{r}");
            }
            say!(self);
            say!(
                self,
                "  g <num> [num]...  create new group from listed shows"
            );
            say!(self, "  a <group#> <num>  add show to existing group");
            say!(self, "  r <num>           remove show from its group");
            say!(self, "  d <group#>        delete entire group");
            say!(self, "  [b]               back");
            say!(self);
            let raw = self.input("  > ")?;
            let tokens: Vec<&str> = raw.split_whitespace().collect();
            let Some(cmd) = tokens.first().map(|t| t.to_lowercase()) else {
                continue;
            };

            match (cmd.as_str(), tokens.len()) {
                ("b", _) => return Some(()),
                ("g", 2..) => {
                    let mut new_group: Vec<String> = Vec::new();
                    for tok in &tokens[1..] {
                        if let Some(i) = index(tok, visible.len())
                            && !new_group.contains(&visible[i].label)
                        {
                            new_group.push(visible[i].label.clone());
                        }
                    }
                    if !new_group.is_empty() {
                        layout::create_group(&mut self.groups, &new_group);
                    }
                }
                ("a", 3) => {
                    // Python checks the group index against the list *before* removal.
                    if let (Some(g), Some(i)) = (
                        index(tokens[1], self.groups.len()),
                        index(tokens[2], visible.len()),
                    ) {
                        layout::add_to_group(&mut self.groups, g, &visible[i].label);
                    }
                }
                ("r", 2) => {
                    if let Some(i) = index(tokens[1], visible.len()) {
                        layout::remove_from_groups(
                            &mut self.groups,
                            std::slice::from_ref(&visible[i].label),
                        );
                    }
                }
                ("d", 2) => {
                    if let Some(g) = index(tokens[1], self.groups.len()) {
                        layout::dissolve_group(&mut self.groups, g);
                    }
                }
                _ => {}
            }
        }
    }
}

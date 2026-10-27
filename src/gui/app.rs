//! egui rendering for the GUI; all state changes go through `GuiState`.

use super::dialogs::{self, Dialog, Outcome};
use super::state::{BalanceSlot, GuiState, Kind, Msg, msg};
use crate::build::{BuildReport, Mode, ShowOrder, build_playlist};
use crate::layout;
use eframe::egui::{self, Key, RichText, Ui};
use egui_extras::{Column, TableBuilder};
use egui_file_dialog::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(500);
const BAR_W: f32 = 64.0;
const BAR_GAP: f32 = 12.0;

#[derive(PartialEq)]
enum Picking {
    Dir,
    Output,
}

/// What the "TV Shows Discovered" panel shows.
#[derive(PartialEq)]
enum View {
    Table,
    Balance,
}

type Settings = (String, bool, bool, ShowOrder, Mode, String);

pub struct App {
    s: GuiState,
    dialogs: Vec<Dialog>,
    picker: FileDialog,
    picking: Option<Picking>,
    view: View,
    job: Option<(PathBuf, Receiver<BuildReport>)>,
    autosave_at: Option<Instant>,
    title: String,
}

impl App {
    pub fn new(s: GuiState) -> Self {
        App {
            s,
            dialogs: vec![],
            picker: FileDialog::new(),
            picking: None,
            view: View::Table,
            job: None,
            autosave_at: None,
            title: String::new(),
        }
    }

    fn settings(&self) -> Settings {
        let s = &self.s;
        (
            s.output_path.clone(),
            s.relative,
            s.repeat,
            s.show_order,
            s.mode,
            s.block_size.clone(),
        )
    }

    fn report(&mut self, r: Result<(), Msg>) {
        if let Err(m) = r {
            self.dialogs.push(Dialog::Message(m));
        }
    }

    fn save_layout(&mut self) {
        match self.s.store.active_layout_name.clone() {
            Some(name) => {
                let r = self.s.save_as(&name);
                self.report(r);
            }
            None => self.save_layout_as(),
        }
    }

    fn save_layout_as(&mut self) {
        let name = self.s.store.active_layout_name.clone().unwrap_or_default();
        self.dialogs.push(Dialog::SaveAs { name });
    }

    fn generate(&mut self) {
        match self.s.prepare_generation() {
            Err(m) => self.dialogs.push(Dialog::Message(m)),
            Ok(job) => {
                let (tx, rx) = std::sync::mpsc::channel();
                self.job = Some((job.output.clone(), rx));
                std::thread::spawn(move || {
                    let r = build_playlist(&job.dirs, &job.output, &job.opts, &mut rand::rng());
                    crate::cli::print_report(&r);
                    let _ = tx.send(r);
                });
            }
        }
    }

    fn poll_job(&mut self, ctx: &egui::Context) {
        let Some((output, rx)) = &self.job else {
            return;
        };
        match rx.try_recv() {
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(50)),
            Ok(r) => {
                let m = self.s.generation_done(&output.clone(), &r);
                self.job = None;
                self.dialogs.push(Dialog::Message(m));
            }
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.s.status = "Generation failed.".into();
                self.dialogs.push(Dialog::Message(msg(
                    Kind::Error,
                    "Generation Error",
                    "An error occurred during playlist generation:\nthe worker thread stopped unexpectedly",
                )));
            }
        }
    }

    fn handle_dialog(&mut self, ctx: &egui::Context) {
        let layouts = self.s.layout_names();
        let groups = layout::group_choices(&self.s.groups);
        let Some(top) = self.dialogs.last_mut() else {
            return;
        };
        let outcome = dialogs::show(ctx, top, &layouts, &groups);
        let top = match outcome {
            Outcome::Open => return,
            Outcome::Close => {
                self.dialogs.pop();
                return;
            }
            Outcome::Confirm => self.dialogs.pop().expect("top dialog"),
        };
        match top {
            Dialog::Message(_) => {}
            Dialog::SaveAs { name } => {
                let trimmed = name.trim().to_string();
                if trimmed.is_empty() {
                    self.dialogs.push(Dialog::SaveAs { name });
                    let m = msg(
                        Kind::Warning,
                        "Name Required",
                        "Please enter a valid layout name.",
                    );
                    self.dialogs.push(Dialog::Message(m));
                } else if self.s.store.layouts.contains_key(&trimmed) {
                    self.dialogs.push(Dialog::SaveAs { name });
                    self.dialogs
                        .push(Dialog::ConfirmOverwrite { name: trimmed });
                } else {
                    self.save_as_or_reopen(trimmed);
                }
            }
            Dialog::ConfirmOverwrite { name } => {
                self.dialogs.pop(); // the Save As dialog underneath
                self.save_as_or_reopen(name);
            }
            Dialog::Load { idx } => {
                if let Some(name) = layouts.get(idx) {
                    self.s.load_layout(name);
                }
            }
            Dialog::Delete { idx } => {
                if let Some(name) = layouts.get(idx) {
                    self.dialogs
                        .push(Dialog::ConfirmDelete { name: name.clone() });
                }
            }
            Dialog::ConfirmDelete { name } => self.s.delete_layout(&name),
            Dialog::AddToGroup { idx } => self.s.add_selected_to_group(idx),
            Dialog::Dissolve { idx } => self.s.dissolve_group(idx),
        }
    }

    fn save_as_or_reopen(&mut self, name: String) {
        if let Err(m) = self.s.save_as(&name) {
            self.dialogs.push(Dialog::SaveAs { name });
            self.dialogs.push(Dialog::Message(m));
        }
    }

    fn handle_picker(&mut self, ctx: &egui::Context) {
        self.picker.update(ctx);
        if matches!(
            self.picker.state(),
            egui_file_dialog::DialogState::Cancelled
        ) {
            self.picking = None;
        }
        if let Some(p) = self.picker.take_picked() {
            match self.picking.take() {
                Some(Picking::Dir) => {
                    let r = self.s.add_directory(p);
                    self.report(r);
                }
                Some(Picking::Output) => {
                    let p = if p.extension().is_none() {
                        p.with_extension("m3u")
                    } else {
                        p
                    };
                    self.s.output_path = p.to_string_lossy().into_owned();
                }
                None => {}
            }
        }
    }

    fn menu(&mut self, ui: &mut Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New Layout").clicked() {
                    self.s.new_layout();
                }
                if ui.button("Save Layout").clicked() {
                    self.save_layout();
                }
                if ui.button("Save Layout As...").clicked() {
                    self.save_layout_as();
                }
                if ui.button("Load Layout...").clicked() {
                    let names = self.s.layout_names();
                    if names.is_empty() {
                        let m = msg(
                            Kind::Info,
                            "No Saved Layouts",
                            "There are no saved layouts to load.",
                        );
                        self.dialogs.push(Dialog::Message(m));
                    } else {
                        let active = self.s.store.active_layout_name.as_ref();
                        let idx = names.iter().position(|n| Some(n) == active).unwrap_or(0);
                        self.dialogs.push(Dialog::Load { idx });
                    }
                }
                if ui.button("Delete Layout...").clicked() {
                    if self.s.layout_names().is_empty() {
                        let m = msg(
                            Kind::Info,
                            "No Saved Layouts",
                            "There are no layouts to delete.",
                        );
                        self.dialogs.push(Dialog::Message(m));
                    } else {
                        self.dialogs.push(Dialog::Delete { idx: 0 });
                    }
                }
                ui.separator();
                if ui.button("Exit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        });
    }

    fn left_column(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.strong("Media Source Directories");
            egui::ScrollArea::vertical()
                .id_salt("dirs")
                .max_height(120.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for (i, d) in self.s.dirs.iter().enumerate() {
                        if ui
                            .selectable_label(self.s.dir_sel == Some(i), d.display().to_string())
                            .clicked()
                        {
                            self.s.dir_sel = Some(i);
                        }
                    }
                });
            ui.horizontal(|ui| {
                if ui.button("Add Directory...").clicked() {
                    self.picking = Some(Picking::Dir);
                    self.picker.pick_directory();
                }
                if ui.button("Remove").clicked() {
                    let r = self.s.remove_directory();
                    self.report(r);
                }
            });
        });
        ui.add_space(10.0);
        ui.group(|ui| {
            ui.strong("Playlist Settings");
            egui::Grid::new("settings")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Interleave Mode:");
                    egui::ComboBox::from_id_salt("mode")
                        .selected_text(self.s.mode.as_str())
                        .show_ui(ui, |ui| {
                            for m in Mode::ALL {
                                ui.selectable_value(&mut self.s.mode, m, m.as_str());
                            }
                        });
                    ui.end_row();
                    ui.label("Block Size (N):");
                    let edit =
                        egui::TextEdit::singleline(&mut self.s.block_size).desired_width(60.0);
                    ui.add_enabled(self.s.mode == Mode::Episodes, edit);
                    ui.end_row();
                    ui.label("Show Order:");
                    egui::ComboBox::from_id_salt("order")
                        .selected_text(self.s.show_order.as_str())
                        .show_ui(ui, |ui| {
                            for o in ShowOrder::ALL {
                                ui.selectable_value(&mut self.s.show_order, o, o.as_str());
                            }
                        });
                    ui.end_row();
                });
            ui.checkbox(&mut self.s.relative, "Write Relative Paths");
            ui.checkbox(&mut self.s.repeat, "Repeat Shorter Shows");
            ui.add_space(6.0);
            ui.label("Output Playlist:");
            ui.horizontal(|ui| {
                let w = ui.available_width() - 36.0;
                ui.add(egui::TextEdit::singleline(&mut self.s.output_path).desired_width(w));
                if ui
                    .button("…")
                    .on_hover_text("Save M3U Playlist As")
                    .clicked()
                {
                    self.picking = Some(Picking::Output);
                    self.picker.save_file();
                }
            });
        });
    }

    fn shows_table(&mut self, ui: &mut Ui) {
        let mods = ui.input(|i| i.modifiers);
        let mut clicked: Option<(usize, bool)> = None;
        let rows: Vec<(bool, String, String, String, String, bool)> = self
            .s
            .show_info
            .iter()
            .map(|si| {
                (
                    self.s.selected.contains(&si.label),
                    si.label.clone(),
                    self.s.group_text(&si.label),
                    si.n_seasons.to_string(),
                    si.n_episodes.to_string(),
                    self.s.rows.contains(&si.label),
                )
            })
            .collect();
        TableBuilder::new(ui)
            .striped(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::exact(50.0))
            .column(Column::remainder().at_least(150.0))
            .column(Column::initial(120.0))
            .column(Column::exact(70.0))
            .column(Column::exact(70.0))
            .header(22.0, |mut h| {
                for t in ["Use?", "Show Name", "Group", "Seasons", "Episodes"] {
                    h.col(|ui| {
                        ui.strong(t);
                    });
                }
            })
            .body(|body| {
                body.rows(20.0, rows.len(), |mut row| {
                    let i = row.index();
                    let (used, label, group, seasons, eps, highlighted) = &rows[i];
                    row.set_selected(*highlighted);
                    // Monospace so "[✓]" and "[ ]" line up (the bundled fonts have ✔ but not ✓).
                    let mark = RichText::new(if *used { "[✔]" } else { "[ ]" }).monospace();
                    let cells = [mark, label.into(), group.into(), seasons.into(), eps.into()];
                    for t in cells {
                        row.col(|ui| {
                            ui.add(egui::Label::new(t).selectable(false));
                        });
                    }
                    let r = row.response();
                    if r.double_clicked() {
                        clicked = Some((i, true));
                    } else if r.clicked() {
                        clicked = Some((i, false));
                    }
                });
            });
        if let Some((i, double)) = clicked {
            if double {
                self.s.toggle_show_selected();
            } else {
                self.s.click_row(i, mods.command, mods.shift);
            }
        }
    }

    /// One bar per slot, scaled to the tallest; drag a show's segment onto another bar to
    /// group them, or onto empty space to take it out of its group.
    fn balance_view(&mut self, ui: &mut Ui) {
        let slots = self.s.balance_slots();
        if slots.is_empty() {
            ui.label("Check ('Use?') some shows to see them here.");
            return;
        }
        let tallest = slots.iter().map(BalanceSlot::total).max().unwrap_or(0).max(1);
        let ctx = ui.ctx().clone();
        let payload = egui::DragAndDrop::payload::<String>(&ctx);
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let released = ui.input(|i| i.pointer.any_released());
        let mut dropped: Option<(String, Option<String>)> = None;

        egui::ScrollArea::horizontal().id_salt("balance").show(ui, |ui| {
            let width = slots.len() as f32 * (BAR_W + BAR_GAP) + BAR_GAP;
            let height = ui.available_height().max(150.0);
            let size = egui::vec2(width, height);
            let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter_at(area);
            let v = ui.visuals().clone();
            let font = egui::FontId::proportional(12.0);
            let (top, bottom) = (area.top() + 20.0, area.bottom() - 4.0);
            let scale = (bottom - top) / tallest as f32;
            painter.hline(area.x_range(), top, egui::Stroke::new(1.0, v.warn_fg_color));

            let mut target = None;
            for (i, slot) in slots.iter().enumerate() {
                let x = area.left() + BAR_GAP + i as f32 * (BAR_W + BAR_GAP);
                let fill = if slot.group.is_some() {
                    v.selection.bg_fill
                } else {
                    v.widgets.inactive.weak_bg_fill
                };
                let mut y = bottom;
                for (label, h) in &slot.members {
                    // Keep tiny segments grabbable; the printed total stays exact.
                    let seg_h = (*h as f32 * scale).max(4.0);
                    let (lo, hi) = (egui::pos2(x, y - seg_h), egui::pos2(x + BAR_W, y));
                    let seg = egui::Rect::from_min_max(lo, hi);
                    y -= seg_h;
                    let r = ui
                        .interact(seg, ui.id().with(("bal", label)), egui::Sense::drag())
                        .on_hover_text(format!("{label}: {h}"));
                    r.dnd_set_drag_payload(label.clone());
                    let active = v.widgets.active.bg_fill;
                    painter.rect_filled(seg, 2.0, if r.dragged() { active } else { fill });
                    let edge = egui::StrokeKind::Inside;
                    painter.rect_stroke(seg, 2.0, v.window_stroke, edge);
                    if seg_h >= 14.0 {
                        let clipped = painter.with_clip_rect(seg.shrink(2.0));
                        clipped.text(
                            seg.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            font.clone(),
                            v.strong_text_color(),
                        );
                    }
                    if r.dragged() {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }
                let (lo, hi) = (egui::pos2(x, y), egui::pos2(x + BAR_W, bottom));
                let bar = egui::Rect::from_min_max(lo, hi);
                painter.text(
                    egui::pos2(bar.center().x, y - 2.0),
                    egui::Align2::CENTER_BOTTOM,
                    slot.total().to_string(),
                    font.clone(),
                    v.text_color(),
                );
                if payload.is_some() && pointer.is_some_and(|p| bar.contains(p)) {
                    let edge = egui::StrokeKind::Outside;
                    painter.rect_stroke(bar, 2.0, v.selection.stroke, edge);
                    target = Some(slot.members[0].0.clone());
                }
            }
            if released
                && let Some(p) = &payload
                && pointer.is_some_and(|p| area.contains(p))
            {
                dropped = Some((p.to_string(), target));
            }
        });

        if let Some((label, onto)) = dropped {
            egui::DragAndDrop::clear_payload(&ctx);
            self.s.drop_show(&label, onto.as_deref());
        }
    }

    fn group_buttons(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.strong("Group Management");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Create Group from Selected").clicked() {
                    let r = self.s.create_group_from_selected();
                    self.report(r);
                }
                if ui.button("Add Selected to Group...").clicked() {
                    match self.s.can_add_to_group() {
                        Ok(()) => self.dialogs.push(Dialog::AddToGroup { idx: 0 }),
                        Err(m) => self.dialogs.push(Dialog::Message(m)),
                    }
                }
                if ui.button("Remove Selected from Group").clicked() {
                    let r = self.s.remove_selected_from_group();
                    self.report(r);
                }
                if ui.button("Dissolve Group...").clicked() {
                    match self.s.can_dissolve() {
                        Ok(()) => self.dialogs.push(Dialog::Dissolve { idx: 0 }),
                        Err(m) => self.dialogs.push(Dialog::Message(m)),
                    }
                }
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let before = self.settings();

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::bottom("footer").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.s.status).italics());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = self.job.is_some();
                    if ui
                        .add_enabled(!busy, egui::Button::new("Generate Playlist"))
                        .clicked()
                    {
                        self.generate();
                    }
                    if busy {
                        ui.spinner();
                    }
                });
            });
            ui.add_space(4.0);
        });
        egui::Panel::left("left")
            .exact_size(320.0)
            .show(ui, |ui| self.left_column(ui));
        egui::CentralPanel::default().show(ui, |ui| {
            let table_h = (ui.available_height() - 80.0).max(100.0);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.strong("TV Shows Discovered");
                    ui.add_space(8.0);
                    ui.selectable_value(&mut self.view, View::Table, "Table");
                    ui.selectable_value(&mut self.view, View::Balance, "Balance");
                });
                ui.allocate_ui(egui::vec2(ui.available_width(), table_h), |ui| {
                    match self.view {
                        View::Table => self.shows_table(ui),
                        View::Balance => self.balance_view(ui),
                    }
                });
            });
            ui.add_space(10.0);
            self.group_buttons(ui);
        });

        if self.dialogs.is_empty()
            && self.picking.is_none()
            && !ctx.egui_wants_keyboard_input()
            && ctx.input(|i| i.key_pressed(Key::Space))
        {
            self.s.toggle_show_selected();
        }
        self.handle_picker(&ctx);
        self.handle_dialog(&ctx);
        self.poll_job(&ctx);

        // Debounced autosave for settings; directory/selection/group changes save at once.
        if self.settings() != before {
            self.autosave_at = Some(Instant::now() + AUTOSAVE_DELAY);
        }
        if let Some(at) = self.autosave_at {
            if Instant::now() >= at {
                self.autosave_at = None;
                self.s.auto_save();
            } else {
                ctx.request_repaint_after(at - Instant::now());
            }
        }

        let title = self.s.title();
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Let a running generation finish so the process doesn't exit mid-write.
        if let Some((_, rx)) = self.job.take() {
            let _ = rx.recv();
        }
        self.s.auto_save();
    }
}

//! Modal dialogs. Dialogs stack: a message shown over "Save Layout As" returns to it.

use super::state::{Kind, Msg};
use eframe::egui::{self, Id, Modal, Ui};

pub enum Dialog {
    Message(Msg),
    SaveAs { name: String },
    ConfirmOverwrite { name: String },
    Load { idx: usize },
    Delete { idx: usize },
    ConfirmDelete { name: String },
    AddToGroup { idx: usize },
    Dissolve { idx: usize },
}

/// What the user chose in the top dialog this frame.
pub enum Outcome {
    /// Nothing yet; keep showing.
    Open,
    /// Dismissed (OK, No, Escape or click outside).
    Close,
    /// Primary button pressed. The dialog's fields hold the choice.
    Confirm,
}

fn combo(ui: &mut Ui, id: &str, idx: &mut usize, choices: &[String]) {
    let text = choices.get(*idx).cloned().unwrap_or_default();
    egui::ComboBox::from_id_salt(id)
        .width(260.0)
        .selected_text(text)
        .show_index(ui, idx, choices.len(), |i| choices[i].clone());
}

fn buttons(ui: &mut Ui, primary: &str, secondary: Option<&str>) -> Outcome {
    let mut out = Outcome::Open;
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button(primary).clicked() {
            out = Outcome::Confirm;
        }
        if let Some(s) = secondary
            && ui.button(s).clicked()
        {
            out = Outcome::Close;
        }
    });
    out
}

/// Show `dialog`. `layouts` and `groups` are the picker choices.
pub fn show(
    ctx: &egui::Context,
    dialog: &mut Dialog,
    layouts: &[String],
    groups: &[String],
) -> Outcome {
    let resp = Modal::new(Id::new("dialog")).show(ctx, |ui| {
        ui.set_min_width(300.0);
        match dialog {
            Dialog::Message(m) => {
                let icon = match m.kind {
                    Kind::Info => "ℹ",
                    Kind::Warning => "⚠",
                    Kind::Error => "⛔",
                };
                ui.heading(format!("{icon} {}", m.title));
                ui.label(&m.text);
                buttons(ui, "OK", None)
            }
            Dialog::SaveAs { name } => {
                ui.heading("Save Layout As");
                ui.label("Enter Layout Name:");
                let r = ui.text_edit_singleline(name);
                r.request_focus();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                match buttons(ui, "Save", Some("Cancel")) {
                    Outcome::Open if enter => Outcome::Confirm,
                    o => o,
                }
            }
            Dialog::ConfirmOverwrite { name } => {
                ui.heading("Confirm Overwrite");
                ui.label(format!("Layout '{name}' already exists. Overwrite?"));
                buttons(ui, "Yes", Some("No"))
            }
            Dialog::Load { idx } => {
                ui.heading("Load Layout");
                ui.label("Select Layout:");
                combo(ui, "load", idx, layouts);
                buttons(ui, "Load", Some("Cancel"))
            }
            Dialog::Delete { idx } => {
                ui.heading("Delete Layout");
                ui.label("Select Layout to Delete:");
                combo(ui, "delete", idx, layouts);
                buttons(ui, "Delete", Some("Cancel"))
            }
            Dialog::ConfirmDelete { name } => {
                ui.heading("Confirm Delete");
                ui.label(format!("Are you sure you want to delete layout: '{name}'?"));
                buttons(ui, "Yes", Some("No"))
            }
            Dialog::AddToGroup { idx } => {
                ui.heading("Select Target Group");
                ui.label("Select existing group:");
                combo(ui, "add", idx, groups);
                buttons(ui, "Add to Group", Some("Cancel"))
            }
            Dialog::Dissolve { idx } => {
                ui.heading("Dissolve Group");
                ui.label("Select group to dissolve:");
                combo(ui, "dissolve", idx, groups);
                buttons(ui, "Dissolve", Some("Cancel"))
            }
        }
    });
    match resp.inner {
        Outcome::Open if resp.should_close() => Outcome::Close,
        o => o,
    }
}

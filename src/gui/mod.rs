//! Settings window (FileConverter's "Configure presets…"): edit which presets appear in the
//! context menu, for which file types, and their conversion settings. Saving re-installs the menu.

mod editor;

use crate::preset::{self, Config, Options, Output, Preset};
use editor::preset_editor;
use eframe::egui;
use std::thread::JoinHandle;

type Missing = Vec<(&'static str, &'static str)>;

const SAVE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);

pub fn run() -> Result<(), String> {
    let cfg = preset::load()?;
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../packaging/icons/file-converter.png"))
        .map_err(|e| e.to_string())?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("File Converter — Settings")
            .with_icon(icon)
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "file-converter",
        options,
        Box::new(|cc| {
            let ctx = cc.egui_ctx.clone();
            let presets = cfg.presets.clone();
            // Off the UI thread so the window shows at once: menu files and an `ffmpeg -encoders` probe.
            let startup = std::thread::spawn(move || {
                // Opening the app is the "install" step on macOS and for per-user setups: register the menu now.
                let status = match crate::integrate::install(&presets) {
                    Ok(_) => String::new(),
                    Err(e) => format!("Could not add the context menu: {e}"),
                };
                let missing = crate::tools::missing();
                ctx.request_repaint();
                (status, missing)
            });
            Ok(Box::new(App {
                saved: cfg.clone(),
                cfg,
                selected: 0,
                reveal: false,
                filter: String::new(),
                new_ext: String::new(),
                status: String::new(),
                confirm_close: false,
                missing: Vec::new(),
                startup: Some(startup),
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

struct App {
    cfg: Config,
    saved: Config,
    selected: usize,
    /// Scroll the preset list to the selection on the next frame (after add/duplicate/move).
    reveal: bool,
    filter: String,
    new_ext: String,
    status: String,
    confirm_close: bool,
    missing: Missing,
    startup: Option<JoinHandle<(String, Missing)>>,
}

impl App {
    /// Takes the startup results; `wait` blocks so saving never races the startup menu install.
    fn finish_startup(&mut self, wait: bool) {
        if self.startup.as_ref().is_some_and(|h| wait || h.is_finished()) {
            let (status, missing) = self.startup.take().unwrap().join().unwrap_or_default();
            if self.status.is_empty() {
                self.status = status;
            }
            self.missing = missing;
        }
    }

    fn validate(&self) -> Result<(), String> {
        for (i, p) in self.cfg.presets.iter().enumerate() {
            if p.name.trim().is_empty() || p.name.split('/').any(|s| s.trim().is_empty()) {
                return Err(format!("Preset #{} has an empty name (or name part).", i + 1));
            }
            if self.cfg.presets[..i].iter().any(|q| q.name == p.name) {
                return Err(format!("Two presets are named '{}'.", p.name));
            }
            if p.inputs.is_empty() {
                return Err(format!("'{}' has no input file types, so it would never show up.", p.name));
            }
            if !p.o.ffmpeg_custom.trim().is_empty() && shlex::split(&p.o.ffmpeg_custom).is_none() {
                return Err(format!("'{}': unbalanced quotes in the custom ffmpeg arguments.", p.name));
            }
        }
        Ok(())
    }

    fn save(&mut self) -> bool {
        self.finish_startup(true);
        let res = self
            .validate()
            .and_then(|_| preset::save(&self.cfg))
            .and_then(|_| crate::integrate::install(&self.cfg.presets));
        match res {
            Ok(msg) => {
                self.saved = self.cfg.clone();
                self.status = format!("Saved. {}", msg.lines().next().unwrap_or_default());
                true
            }
            Err(e) => {
                self.status = format!("Not saved: {e}");
                false
            }
        }
    }

    fn select(&mut self, i: usize) {
        self.selected = i;
        self.reveal = true;
    }

    fn presets_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ New").clicked() {
                let name = (1..)
                    .map(|i| if i == 1 { "New preset".to_string() } else { format!("New preset {i}") })
                    .find(|n| !self.cfg.presets.iter().any(|p| &p.name == n))
                    .unwrap();
                self.cfg.presets.push(Preset { name, output: Output::Mp4, inputs: vec![], o: Options::default() });
                self.filter.clear();
                self.select(self.cfg.presets.len() - 1);
            }
            let has = self.selected < self.cfg.presets.len();
            if ui.add_enabled(has, egui::Button::new("⎘ Duplicate")).clicked() {
                let mut p = self.cfg.presets[self.selected].clone();
                p.name += " (copy)";
                self.cfg.presets.insert(self.selected + 1, p);
                self.select(self.selected + 1);
            }
            if ui
                .add_enabled(has, egui::Button::new("🗑 Delete"))
                .on_hover_text("Revert undoes it until you save")
                .clicked()
            {
                self.cfg.presets.remove(self.selected);
                self.select(self.selected.min(self.cfg.presets.len().saturating_sub(1)));
            }
            if ui.add_enabled(has && self.selected > 0, egui::Button::new("⏶")).on_hover_text("Move up").clicked() {
                self.cfg.presets.swap(self.selected, self.selected - 1);
                self.select(self.selected - 1);
            }
            if ui
                .add_enabled(self.selected + 1 < self.cfg.presets.len(), egui::Button::new("⏷"))
                .on_hover_text("Move down")
                .clicked()
            {
                self.cfg.presets.swap(self.selected, self.selected + 1);
                self.select(self.selected + 1);
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.filter).hint_text("🔍 Search presets").desired_width(f32::INFINITY),
        );
        ui.separator();
        let needle = self.filter.to_lowercase();
        let reveal = std::mem::take(&mut self.reveal);
        let mut clicked = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let shown = self.cfg.presets.iter().enumerate().filter(|(_, p)| p.name.to_lowercase().contains(&needle));
            for (i, p) in shown {
                let label = format!("{}   → {}", p.name.replace('/', " › "), p.output.ext());
                let r = ui.selectable_label(i == self.selected, label);
                if r.clicked() {
                    clicked = Some(i);
                }
                if reveal && i == self.selected {
                    r.scroll_to_me(None);
                }
            }
        });
        if let Some(i) = clicked {
            self.selected = i;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.finish_startup(false);
        let mut dirty = self.cfg != self.saved;
        if ui.ctx().input(|i| i.viewport().close_requested()) && dirty {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
        }
        if dirty && ui.ctx().input_mut(|i| i.consume_shortcut(&SAVE)) {
            self.save();
            dirty = self.cfg != self.saved;
        }

        if !self.missing.is_empty() {
            egui::Panel::top("missing").show(ui, |ui| {
                ui.add_space(4.0);
                let list: Vec<String> = self.missing.iter().map(|(t, why)| format!("{t} ({why})")).collect();
                ui.colored_label(ui.visuals().warn_fg_color, format!("Not installed: {}", list.join(", ")));
                ui.horizontal(|ui| {
                    let cmd = crate::tools::install_hint();
                    ui.label("Install them with:");
                    ui.code(cmd);
                    if ui.small_button("Copy").clicked() {
                        ui.ctx().copy_text(cmd.to_string());
                    }
                    if ui.small_button("Check again").clicked() {
                        self.missing = crate::tools::missing();
                    }
                });
                ui.add_space(4.0);
            });
        }

        egui::Panel::bottom("actions").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Simultaneous conversions:");
                ui.add(egui::DragValue::new(&mut self.cfg.max_simultaneous).range(1..=32))
                    .on_hover_text("How many files convert at the same time");
                ui.separator();
                let save =
                    egui::Button::new("💾 Save and update context menu").shortcut_text(ui.ctx().format_shortcut(&SAVE));
                if ui.add_enabled(dirty, save).clicked() {
                    self.save();
                }
                if ui
                    .add_enabled(dirty, egui::Button::new("Revert"))
                    .on_hover_text("Undo all unsaved changes")
                    .clicked()
                {
                    self.cfg = self.saved.clone();
                }
                // Away from Save: replacing every preset is not a routine action.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Restore defaults").on_hover_text("Replace all presets; Revert undoes it").clicked() {
                        self.cfg = preset::defaults();
                        self.select(0);
                    }
                });
            });
            let status = if dirty && self.status.is_empty() { "Unsaved changes." } else { &self.status };
            if status.starts_with("Not saved") || status.starts_with("Could not") {
                ui.colored_label(ui.visuals().error_fg_color, status);
            } else {
                ui.label(status);
            }
            ui.add_space(4.0);
        });

        egui::Panel::left("presets").resizable(true).default_size(340.0).show(ui, |ui| {
            ui.heading("Presets");
            ui.label("Shown in the right-click menu for their input types. Use '/' in a name for a sub-menu.");
            self.presets_list(ui);
        });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.cfg.presets.get_mut(self.selected) {
                Some(p) => preset_editor(ui, p, &mut self.new_ext),
                None => {
                    ui.label("No preset selected.");
                }
            });
        });

        if self.confirm_close {
            let mut close = false;
            egui::Modal::new(egui::Id::unique("confirm-close")).show(ui.ctx(), |ui| {
                ui.label("Save changes before closing?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() && self.save() {
                        close = true;
                    }
                    if ui.button("Discard").clicked() {
                        self.cfg = self.saved.clone();
                        close = true;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_close = false;
                    }
                });
            });
            if close {
                self.confirm_close = false;
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}

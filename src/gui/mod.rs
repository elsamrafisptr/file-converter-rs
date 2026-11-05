//! Settings window (FileConverter's "Configure presets…"): edit which presets appear in the
//! context menu, for which file types, and their conversion settings. Saving re-installs the menu.

mod editor;

use crate::preset::{self, Config, Options, Output, Preset};
use editor::preset_editor;
use eframe::egui;

pub fn run() -> Result<(), String> {
    let cfg = preset::load()?;
    // Opening the app is the "install" step on macOS and for per-user setups: register the menu now.
    let status = match crate::integrate::install(&cfg.presets) {
        Ok(_) => String::new(),
        Err(e) => format!("Could not add the context menu: {e}"),
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("File Converter — Settings")
            .with_inner_size([980.0, 680.0]),
        ..Default::default()
    };
    let app = App {
        saved: cfg.clone(),
        cfg,
        selected: 0,
        new_ext: String::new(),
        status,
        confirm_close: false,
        missing: crate::tools::missing(),
    };
    eframe::run_native("file-converter", options, Box::new(|_| Ok(Box::new(app)))).map_err(|e| e.to_string())
}

struct App {
    cfg: Config,
    saved: Config,
    selected: usize,
    new_ext: String,
    status: String,
    confirm_close: bool,
    missing: Vec<(&'static str, &'static str)>,
}

impl App {
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

    fn presets_list(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("➕ New").clicked() {
                let name = (1..)
                    .map(|i| if i == 1 { "New preset".to_string() } else { format!("New preset {i}") })
                    .find(|n| !self.cfg.presets.iter().any(|p| &p.name == n))
                    .unwrap();
                self.cfg.presets.push(Preset { name, output: Output::Mp4, inputs: vec![], o: Options::default() });
                self.selected = self.cfg.presets.len() - 1;
            }
            let has = self.selected < self.cfg.presets.len();
            if ui.add_enabled(has, egui::Button::new("⎘ Duplicate")).clicked() {
                let mut p = self.cfg.presets[self.selected].clone();
                p.name += " (copy)";
                self.cfg.presets.insert(self.selected + 1, p);
                self.selected += 1;
            }
            if ui.add_enabled(has, egui::Button::new("🗑 Delete")).clicked() {
                self.cfg.presets.remove(self.selected);
                self.selected = self.selected.min(self.cfg.presets.len().saturating_sub(1));
            }
            if ui.add_enabled(has && self.selected > 0, egui::Button::new("⏶")).on_hover_text("Move up").clicked() {
                self.cfg.presets.swap(self.selected, self.selected - 1);
                self.selected -= 1;
            }
            if ui
                .add_enabled(self.selected + 1 < self.cfg.presets.len(), egui::Button::new("⏷"))
                .on_hover_text("Move down")
                .clicked()
            {
                self.cfg.presets.swap(self.selected, self.selected + 1);
                self.selected += 1;
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, p) in self.cfg.presets.iter().enumerate() {
                let label = format!("{}   → {}", p.name.replace('/', " › "), p.output.ext());
                if ui.selectable_label(i == self.selected, label).clicked() {
                    self.selected = i;
                }
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dirty = self.cfg != self.saved;
        if ui.ctx().input(|i| i.viewport().close_requested()) && dirty {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.confirm_close = true;
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
                ui.add(egui::DragValue::new(&mut self.cfg.max_simultaneous).range(1..=32));
                ui.separator();
                if ui.add_enabled(dirty, egui::Button::new("💾 Save and update context menu")).clicked() {
                    self.save();
                }
                if ui.add_enabled(dirty, egui::Button::new("Revert")).clicked() {
                    self.cfg = self.saved.clone();
                }
                if ui.button("Restore defaults").clicked() {
                    self.cfg = preset::defaults();
                    self.selected = 0;
                }
            });
            ui.label(if dirty && self.status.is_empty() { "Unsaved changes." } else { &self.status });
            ui.add_space(4.0);
        });

        egui::Panel::left("presets").resizable(true).default_size(300.0).show(ui, |ui| {
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

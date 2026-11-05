//! Settings window (FileConverter's "Configure presets…"): edit which presets appear in the
//! context menu, for which file types, and their conversion settings. Saving re-installs the menu.

use crate::convert::{AAC_VBR, MP3_VBR, OGG_VBR};
use crate::preset::{self, AudioMode, Config, Options, Output, PostAction, Preset, VideoSpeed};
use eframe::egui;
use std::path::Path;

const INPUT_GROUPS: &[(&str, &[&str])] = &[
    ("Video", &["3gp", "3gpp", "avi", "bik", "flv", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "ogv", "rm", "ts", "vob", "webm", "wmv"]),
    ("Audio", &["aac", "aiff", "ape", "flac", "m4a", "m4b", "mp3", "oga", "ogg", "opus", "wav", "wma"]),
    ("Image", &[
        "arw", "avif", "bmp", "cr2", "dds", "dng", "exr", "gif", "heic", "ico", "jfif", "jpeg", "jpg", "nef", "png", "psd",
        "raf", "svg", "tga", "tif", "tiff", "webp", "xcf",
    ]),
    ("Document", &["doc", "docx", "odp", "ods", "odt", "pdf", "ppt", "pptx", "xls", "xlsx"]),
];
const CBR: &[u32] = &[32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];

pub fn run() -> Result<(), String> {
    let cfg = preset::load()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("File Converter — Settings")
            .with_inner_size([980.0, 680.0]),
        ..Default::default()
    };
    let app = App { saved: cfg.clone(), cfg, selected: 0, new_ext: String::new(), status: String::new(), confirm_close: false };
    eframe::run_native("file-converter", options, Box::new(|_| Ok(Box::new(app)))).map_err(|e| e.to_string())
}

struct App {
    cfg: Config,
    saved: Config,
    selected: usize,
    new_ext: String,
    status: String,
    confirm_close: bool,
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

fn preset_editor(ui: &mut egui::Ui, p: &mut Preset, new_ext: &mut String) {
    egui::Grid::new("general").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(360.0));
        ui.end_row();

        ui.label("Output type");
        egui::ComboBox::from_id_salt("output").selected_text(p.output.ext()).show_ui(ui, |ui| {
            for o in Output::ALL {
                ui.selectable_value(&mut p.output, o, o.ext());
            }
        });
        ui.end_row();

        ui.label("Output file");
        ui.vertical(|ui| {
            ui.add(egui::TextEdit::singleline(&mut p.o.output_template).desired_width(360.0));
            ui.small("(p) folder · (f) name · (F) NAME · (i) input ext · (d0) folder name · (p:v) Videos · (p:m) Music · (p:p) Pictures · (p:d) Documents");
            let sample = dirs::video_dir().or_else(dirs::home_dir).unwrap_or_default().join(format!("clip.{}", p.inputs.first().map_or("mp4", |s| s)));
            ui.small(format!("e.g. {} → {}", sample.display(), preset::output_path(Path::new(&sample), p).display()));
        });
        ui.end_row();

        ui.label("Afterwards");
        ui.horizontal(|ui| {
            ui.radio_value(&mut p.o.post_action, PostAction::None, "Keep input");
            ui.radio_value(&mut p.o.post_action, PostAction::Archive, "Move input to \"Conversion Archive\"");
            ui.radio_value(&mut p.o.post_action, PostAction::Delete, "Delete input");
        });
        ui.end_row();
    });

    ui.add_space(8.0);
    ui.heading("Input types");
    ui.label("The preset appears in the right-click menu of these files.");
    for (group, exts) in INPUT_GROUPS {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("{group}:"));
            if ui.small_button("all").clicked() {
                for e in *exts {
                    if !p.inputs.iter().any(|i| i == e) {
                        p.inputs.push(e.to_string());
                    }
                }
            }
            if ui.small_button("none").clicked() {
                p.inputs.retain(|i| !exts.contains(&i.as_str()));
            }
            for e in *exts {
                let mut on = p.inputs.iter().any(|i| i.eq_ignore_ascii_case(e));
                if ui.checkbox(&mut on, *e).changed() {
                    if on {
                        p.inputs.push(e.to_string());
                    } else {
                        p.inputs.retain(|i| !i.eq_ignore_ascii_case(e));
                    }
                }
            }
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.strong("Other:");
        let known = |e: &str| INPUT_GROUPS.iter().any(|(_, g)| g.contains(&e));
        let mut remove = None;
        for e in p.inputs.iter().filter(|e| !known(e)) {
            if ui.small_button(format!("{e} ✖")).clicked() {
                remove = Some(e.clone());
            }
        }
        if let Some(r) = remove {
            p.inputs.retain(|e| *e != r);
        }
        let edit = ui.add(egui::TextEdit::singleline(new_ext).hint_text("extension").desired_width(80.0));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui.button("Add").clicked() || enter {
            let e = new_ext.trim().trim_start_matches('.').to_lowercase();
            if !e.is_empty() && !p.inputs.contains(&e) {
                p.inputs.push(e);
            }
            new_ext.clear();
        }
    });

    ui.add_space(8.0);
    ui.heading("Conversion settings");
    conversion_settings(ui, p);
}

fn conversion_settings(ui: &mut egui::Ui, p: &mut Preset) {
    use Output::*;
    let ffmpeg = matches!(p.output, Aac | Avi | Flac | Gif | Mkv | Mp3 | Mp4 | Ogg | Ogv | Wav | Webm);
    if ffmpeg {
        let mut custom = !p.o.ffmpeg_custom.trim().is_empty();
        if ui.checkbox(&mut custom, "Custom ffmpeg arguments (replaces the settings below)").changed() {
            p.o.ffmpeg_custom = if custom {
                // Start from what the current settings generate.
                crate::convert::ffmpeg_args(p).ok().and_then(|a| shlex::try_join(a.iter().map(String::as_str)).ok()).unwrap_or_default()
            } else {
                String::new()
            };
            if custom && p.o.ffmpeg_custom.is_empty() {
                p.o.ffmpeg_custom = "-c copy".into();
            }
        }
        if custom {
            ui.label(format!("ffmpeg -i INPUT  …  OUTPUT.{}", p.output.ext()));
            ui.add(egui::TextEdit::multiline(&mut p.o.ffmpeg_custom).code_editor().desired_rows(3).desired_width(f32::INFINITY));
            return;
        }
    }

    let o = &mut p.o;
    egui::Grid::new("settings").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        let mut video = |ui: &mut egui::Ui, max: u32, hint: &str| {
            ui.label("Video quality");
            ui.add(egui::Slider::new(&mut o.video_quality, 0..=max).text(hint));
            ui.end_row();
        };
        match p.output {
            Mkv | Mp4 => {
                video(ui, 51, "higher = better, bigger");
                ui.label("Encoding speed");
                egui::ComboBox::from_id_salt("speed").selected_text(format!("{:?}", o.video_speed)).show_ui(ui, |ui| {
                    for s in VideoSpeed::ALL {
                        ui.selectable_value(&mut o.video_speed, s, format!("{s:?}"));
                    }
                });
                ui.end_row();
            }
            Webm => video(ui, 63, "63 = lossless"),
            Ogv => video(ui, 10, ""),
            Avi => video(ui, 30, ""),
            _ => {}
        }
        if matches!(p.output, Mkv | Mp4 | Webm | Ogv | Avi) {
            ui.label("Audio");
            ui.horizontal(|ui| {
                ui.checkbox(&mut o.enable_audio, "keep audio");
                let table = match p.output {
                    Mkv | Mp4 => AAC_VBR,
                    Avi => MP3_VBR,
                    _ => OGG_VBR,
                };
                ui.add_enabled_ui(o.enable_audio, |ui| bitrate(ui, &mut o.audio_bitrate, table, "≈"));
            });
            ui.end_row();
        }
        if matches!(p.output, Mkv | Mp4 | Webm | Ogv | Avi | Gif) {
            if p.output == Gif {
                ui.label("Frames per second");
                ui.add(egui::Slider::new(&mut o.fps, 1..=50));
                ui.end_row();
            }
            scale_rotation(ui, &mut o.video_scale, &mut o.video_rotation);
        }

        match p.output {
            Mp3 => {
                ui.label("Encoding");
                ui.horizontal(|ui| {
                    if o.audio_mode != AudioMode::Mp3Cbr {
                        o.audio_mode = AudioMode::Mp3Vbr;
                    }
                    ui.radio_value(&mut o.audio_mode, AudioMode::Mp3Vbr, "Variable bitrate");
                    ui.radio_value(&mut o.audio_mode, AudioMode::Mp3Cbr, "Constant bitrate");
                });
                ui.end_row();
                ui.label("Bitrate");
                if o.audio_mode == AudioMode::Mp3Cbr {
                    bitrate(ui, &mut o.audio_bitrate, &CBR.iter().map(|b| (*b, "")).collect::<Vec<_>>(), "");
                } else {
                    bitrate(ui, &mut o.audio_bitrate, MP3_VBR, "≈");
                }
                ui.end_row();
            }
            Ogg | Aac => {
                ui.label("Bitrate");
                bitrate(ui, &mut o.audio_bitrate, if p.output == Ogg { OGG_VBR } else { AAC_VBR }, "≈");
                ui.end_row();
            }
            Wav => {
                ui.label("Bit depth");
                if !matches!(o.audio_mode, AudioMode::Wav8 | AudioMode::Wav16 | AudioMode::Wav24 | AudioMode::Wav32) {
                    o.audio_mode = AudioMode::Wav16;
                }
                ui.horizontal(|ui| {
                    for (m, l) in [(AudioMode::Wav8, "8 bit"), (AudioMode::Wav16, "16 bit"), (AudioMode::Wav24, "24 bit"), (AudioMode::Wav32, "32 bit")] {
                        ui.radio_value(&mut o.audio_mode, m, l);
                    }
                });
                ui.end_row();
            }
            _ => {}
        }
        if matches!(p.output, Mp3 | Ogg | Aac | Wav | Flac) {
            ui.label("Channels");
            ui.horizontal(|ui| {
                ui.radio_value(&mut o.audio_channels, 0, "Keep");
                ui.radio_value(&mut o.audio_channels, 1, "Mono");
                ui.radio_value(&mut o.audio_channels, 2, "Stereo");
            });
            ui.end_row();
        }

        if matches!(p.output, Jpg | Webp | Avif | Png) {
            if p.output != Png {
                ui.label("Image quality");
                ui.add(egui::Slider::new(&mut o.image_quality, 1..=100));
                ui.end_row();
            }
            scale_rotation(ui, &mut o.image_scale, &mut o.image_rotation);
            ui.label("Maximum size");
            ui.add(egui::DragValue::new(&mut o.image_max_size).range(0..=20000).suffix(" px (0 = no limit)"));
            ui.end_row();
            ui.label("Power of 2");
            ui.checkbox(&mut o.image_pow2, "Clamp to a square power-of-two size (textures)");
            ui.end_row();
        }
        if matches!(p.output, Ico | Pdf) {
            ui.label("No settings for this output type.");
            ui.end_row();
        }
    });
}

/// Bitrate picker; VBR encoders map it to the nearest quality step.
fn bitrate(ui: &mut egui::Ui, value: &mut u32, table: &[(u32, &str)], prefix: &str) {
    egui::ComboBox::from_id_salt(("bitrate", table.len())).selected_text(format!("{prefix}{value} kbit/s")).show_ui(ui, |ui| {
        for (b, _) in table {
            ui.selectable_value(value, *b, format!("{prefix}{b} kbit/s"));
        }
    });
}

fn scale_rotation(ui: &mut egui::Ui, scale: &mut f64, rotation: &mut f64) {
    ui.label("Scale");
    let mut pct = (*scale * 100.0).round() as u32;
    if ui.add(egui::Slider::new(&mut pct, 5..=200).suffix(" %")).changed() {
        *scale = pct as f64 / 100.0;
    }
    ui.end_row();
    ui.label("Rotation");
    ui.horizontal(|ui| {
        for (deg, l) in [(0.0, "None"), (90.0, "Left 90°"), (180.0, "180°"), (270.0, "Right 90°")] {
            ui.radio_value(rotation, deg, l);
        }
    });
    ui.end_row();
}

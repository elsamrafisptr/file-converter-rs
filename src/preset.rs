use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_PRESETS: &str = include_str!("presets.toml");

#[derive(Deserialize, Clone, PartialEq)]
pub struct Config {
    #[serde(default = "two")]
    pub max_simultaneous: usize,
    #[serde(rename = "preset")]
    pub presets: Vec<Preset>,
}

fn two() -> usize {
    2
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Aac, Avi, Avif, Flac, Gif, Ico, Jpg, Mkv, Mp3, Mp4, Ogg, Ogv, Pdf, Png, Wav, Webm, Webp,
}

impl Output {
    pub const ALL: [Output; 17] = [
        Output::Aac, Output::Avi, Output::Avif, Output::Flac, Output::Gif, Output::Ico, Output::Jpg, Output::Mkv,
        Output::Mp3, Output::Mp4, Output::Ogg, Output::Ogv, Output::Pdf, Output::Png, Output::Wav, Output::Webm,
        Output::Webp,
    ];

    pub fn ext(self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PostAction {
    #[default]
    None,
    Archive,
    Delete,
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum VideoSpeed {
    UltraFast, SuperFast, VeryFast, Faster, Fast,
    #[default]
    Medium,
    Slow, Slower, VerySlow,
}

impl VideoSpeed {
    pub const ALL: [VideoSpeed; 9] = [
        VideoSpeed::UltraFast, VideoSpeed::SuperFast, VideoSpeed::VeryFast, VideoSpeed::Faster, VideoSpeed::Fast,
        VideoSpeed::Medium, VideoSpeed::Slow, VideoSpeed::Slower, VideoSpeed::VerySlow,
    ];
}

#[derive(Deserialize, Serialize, Clone, Copy, Default, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    #[default]
    Mp3Vbr, Mp3Cbr, Wav8, Wav16, Wav24, Wav32,
}

#[derive(Deserialize, Serialize, Clone, PartialEq)]
pub struct Preset {
    /// `Folder/Sub/Name`: slashes create context sub-menus.
    pub name: String,
    pub output: Output,
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub o: Options,
}

#[derive(Deserialize, Serialize, Clone, PartialEq)]
#[serde(default)]
pub struct Options {
    pub output_template: String,
    pub post_action: PostAction,
    pub enable_audio: bool,
    pub video_quality: u32,
    pub video_speed: VideoSpeed,
    pub video_scale: f64,
    /// Degrees counter-clockwise: 90, 180 or 270.
    pub video_rotation: f64,
    pub fps: u32,
    pub audio_bitrate: u32,
    pub audio_channels: u32,
    pub audio_mode: AudioMode,
    pub image_quality: u32,
    pub image_scale: f64,
    /// Degrees counter-clockwise.
    pub image_rotation: f64,
    pub image_pow2: bool,
    pub image_max_size: u32,
    /// When set, replaces every generated ffmpeg encoder argument.
    pub ffmpeg_custom: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            output_template: "(p)(f)".into(),
            post_action: PostAction::None,
            enable_audio: true,
            video_quality: 30,
            video_speed: VideoSpeed::Medium,
            video_scale: 1.0,
            video_rotation: 0.0,
            fps: 15,
            audio_bitrate: 160,
            audio_channels: 0,
            audio_mode: AudioMode::Mp3Vbr,
            image_quality: 80,
            image_scale: 1.0,
            image_rotation: 0.0,
            image_pow2: false,
            image_max_size: 0,
            ffmpeg_custom: String::new(),
        }
    }
}

pub fn ext_of(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

impl Preset {
    pub fn accepts(&self, path: &Path) -> bool {
        let ext = ext_of(path);
        self.inputs.iter().any(|i| i.eq_ignore_ascii_case(&ext))
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("file-converter").join("presets.toml")
}

/// Loads the user's presets, writing the defaults on first use.
pub fn load() -> Result<Config, String> {
    let path = config_path();
    if !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, DEFAULT_PRESETS).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let cfg: Config = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(cfg)
}

pub fn defaults() -> Config {
    toml::from_str(DEFAULT_PRESETS).expect("built-in presets are valid")
}

/// TOML text for the presets, omitting options left at their default value.
fn to_toml(cfg: &Config) -> Result<String, String> {
    let defaults = toml::Table::try_from(Options::default()).map_err(|e| e.to_string())?;
    let mut presets = toml::value::Array::new();
    for p in &cfg.presets {
        let mut t = toml::Table::try_from(p).map_err(|e| e.to_string())?;
        t.retain(|k, v| defaults.get(k) != Some(v));
        presets.push(t.into());
    }
    let mut root = toml::Table::new();
    root.insert("max_simultaneous".into(), (cfg.max_simultaneous as i64).into());
    root.insert("preset".into(), presets.into());
    Ok("# File Converter presets (edited by `file-converter settings`). Delete to restore defaults.\n\n".to_string()
        + &toml::to_string(&root).map_err(|e| e.to_string())?)
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let text = to_toml(cfg)?;
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Output path from a FileConverter-style template, e.g. `(p)(f)` or `(p:v)(d0)/(f)_small`.
pub fn output_path(input: &Path, preset: &Preset) -> PathBuf {
    let ext = preset.output.ext();
    let in_ext = input.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let parent = input.parent().unwrap_or(Path::new(""));
    let sep = std::path::MAIN_SEPARATOR_STR;
    let dir = |d: Option<PathBuf>| d.map(|p| format!("{}{sep}", p.display())).unwrap_or_default();

    let mut t = preset.o.output_template.clone();
    for (k, v) in [
        ("(path)", dir(Some(parent.to_path_buf()))),
        ("(p:documents)", dir(dirs::document_dir())),
        ("(p:music)", dir(dirs::audio_dir())),
        ("(p:videos)", dir(dirs::video_dir())),
        ("(p:pictures)", dir(dirs::picture_dir())),
        ("(p:d)", dir(dirs::document_dir())),
        ("(p:m)", dir(dirs::audio_dir())),
        ("(p:v)", dir(dirs::video_dir())),
        ("(p:p)", dir(dirs::picture_dir())),
        ("(p)", dir(Some(parent.to_path_buf()))),
        ("(filename)", stem.clone()),
        ("(f)", stem.clone()),
        ("(F)", stem.to_uppercase()),
        ("(outputext)", ext.clone()),
        ("(o)", ext.clone()),
        ("(O)", ext.to_uppercase()),
        ("(inputext)", in_ext.clone()),
        ("(i)", in_ext.clone()),
        ("(I)", in_ext.to_uppercase()),
    ] {
        t = t.replace(k, &v);
    }
    // (d0) is the input's parent folder name, (d1) its parent, ...
    let names: Vec<String> = parent.iter().map(|c| c.to_string_lossy().into_owned()).collect();
    for (i, name) in names.iter().rev().enumerate() {
        t = t.replace(&format!("(d{i})"), name).replace(&format!("(D{i})"), &name.to_uppercase());
    }
    PathBuf::from(format!("{t}.{ext}"))
}

/// `a.mp4` -> `a (2).mp4` -> `a (3).mp4` until free.
pub fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..)
        .map(|i| path.with_file_name(format!("{stem} ({i}){ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn defaults_parse_and_template() {
        let cfg: Config = toml::from_str(DEFAULT_PRESETS).unwrap();
        assert!(cfg.presets.len() > 60);
        let mp4 = cfg.presets.iter().find(|p| p.name == "To Mp4").unwrap();
        assert!(mp4.accepts(Path::new("/x/Clip.MOV")));
        assert!(!mp4.accepts(Path::new("/x/song.mp3")));
        let out = output_path(Path::new("/home/u/Videos/clip.mov"), mp4);
        assert_eq!(out, PathBuf::from("/home/u/Videos/clip.mp4"));

        let mut p = mp4.clone();
        p.o.output_template = "(p)(d0)-(F)_(i)".into();
        assert_eq!(output_path(Path::new("/a/Vids/clip.mov"), &p), PathBuf::from("/a/Vids/Vids-CLIP_mov.mp4"));
    }

    #[test]
    fn saved_presets_round_trip() {
        let mut cfg = defaults();
        cfg.presets[0].o.video_scale = 0.8;
        cfg.presets[0].o.ffmpeg_custom = r#"-vf "scale=1280:720""#.into();
        let back: Config = toml::from_str(&to_toml(&cfg).unwrap()).unwrap();
        assert_eq!(back.presets.len(), cfg.presets.len());
        for (a, b) in cfg.presets.iter().zip(&back.presets) {
            assert_eq!((&a.name, a.output, &a.inputs), (&b.name, b.output, &b.inputs));
            assert!(a.o == b.o, "{}", a.name);
        }
    }
}

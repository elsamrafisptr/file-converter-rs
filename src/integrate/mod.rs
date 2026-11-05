//! File manager context menus. FileConverter uses a Windows shell extension; here each platform's
//! native, no-plugin mechanism is used instead:
//! - Linux: KDE Dolphin service menus (filtered by MIME type) and GNOME Nautilus scripts.
//! - Windows: per-extension cascading verbs under HKCU\Software\Classes\SystemFileAssociations.
//! - macOS: one Finder Quick Action that asks for the preset (`file-converter pick`).

use crate::preset::Preset;
use std::path::PathBuf;

type R<T> = Result<T, String>;

/// Last context menu entry; opens the settings window.
pub const SETTINGS_LABEL: &str = "Configure presets…";

fn exe() -> R<PathBuf> {
    std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| e.to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn choose(_names: &[&str], _count: usize) -> R<Option<String>> {
    Err("`pick` is only used on macOS; use `convert --preset NAME`".into())
}

#[cfg(target_os = "macos")]
pub fn choose(names: &[&str], count: usize) -> R<Option<String>> {
    let q = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let list = names.iter().map(|n| q(n)).collect::<Vec<_>>().join(", ");
    let script = format!(
        "choose from list {{{list}}} with title \"File Converter\" with prompt \"Convert {count} file(s) with:\""
    );
    let out = std::process::Command::new("osascript").args(["-e", &script]).output().map_err(|e| e.to_string())?;
    let pick = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok((out.status.success() && pick != "false" && !pick.is_empty()).then_some(pick))
}

#[cfg(all(unix, not(target_os = "macos")))]
#[path = "linux.rs"]
mod imp;
#[cfg(windows)]
#[path = "windows.rs"]
mod imp;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod imp;

pub use imp::{install, uninstall};
#[cfg(all(unix, not(target_os = "macos")))]
pub use imp::{install_system, uninstall_system};

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn install_system() -> R<String> {
    Err("--system is only used by the Linux packages".into())
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn uninstall_system() -> R<String> {
    install_system()
}

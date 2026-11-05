//! Locating the external converters (ffmpeg, ImageMagick, Ghostscript, LibreOffice).

use std::path::{Path, PathBuf};

type R<T> = Result<T, String>;

/// Folders to search, in order. Finder/Explorer launch us with a minimal PATH, so well-known
/// install locations are added, and installers may bundle tools in `tools/` next to the binary.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("tools"));
        dirs.push(exe_dir);
    }
    dirs.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    if cfg!(target_os = "macos") {
        dirs.extend(
            ["/opt/homebrew/bin", "/usr/local/bin", "/Applications/LibreOffice.app/Contents/MacOS"].map(PathBuf::from),
        );
    } else if cfg!(windows) {
        let pf = PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()));
        dirs.push(pf.join(r"LibreOffice\program"));
        // Versioned install folders: newest first.
        dirs.extend(versioned(&pf.join("gs"), "gs").into_iter().map(|d| d.join("bin")));
        dirs.extend(versioned(&pf, "ImageMagick-"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
        }
    } else {
        dirs.extend(["/usr/local/bin", "/usr/bin", "/snap/bin"].map(PathBuf::from));
    }
    dirs
}

/// Subfolders of `parent` starting with `prefix`, highest version first.
fn versioned(parent: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .map(|e| e.path())
        .collect();
    // shortcut: lexical order, fine while version components have the same digit counts.
    v.sort_by(|a, b| b.cmp(a));
    v
}

/// First of `names` found in the search folders.
pub fn tool(names: &[&str]) -> R<PathBuf> {
    let dirs = search_dirs();
    for name in names {
        for d in &dirs {
            let p = d.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    Err(format!("'{}' not found; install it or put it next to file-converter", names[0]))
}

pub fn ffmpeg() -> R<PathBuf> {
    tool(&["ffmpeg"])
}

pub fn magick() -> R<PathBuf> {
    // ImageMagick 7 ships `magick`; v6 (older Linux distros) only `convert` (which is a system tool on Windows).
    if cfg!(windows) {
        tool(&["magick"])
    } else {
        tool(&["magick", "convert"])
    }
}

pub fn ghostscript() -> R<PathBuf> {
    tool(&["gs", "gswin64c", "gswin32c"])
}

pub fn libreoffice() -> R<PathBuf> {
    tool(&["soffice", "libreoffice", "soffice.com"])
}

/// Converters that are not installed, with what they are needed for.
pub fn missing() -> Vec<(&'static str, &'static str)> {
    let checks: [(&str, &str, fn() -> R<PathBuf>); 4] = [
        ("ffmpeg", "audio and video", ffmpeg),
        ("ImageMagick", "images", magick),
        ("Ghostscript", "PDF pages", ghostscript),
        ("LibreOffice", "Office documents (optional)", libreoffice),
    ];
    checks.into_iter().filter(|(_, _, find)| find().is_err()).map(|(name, purpose, _)| (name, purpose)).collect()
}

/// One command that installs every converter on this OS.
pub fn install_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew install ffmpeg imagemagick ghostscript && brew install --cask libreoffice"
    } else if cfg!(windows) {
        "winget install Gyan.FFmpeg ImageMagick.ImageMagick ArtifexSoftware.GhostScript TheDocumentFoundation.LibreOffice"
    } else if Path::new("/usr/bin/apt").exists() {
        "sudo apt install ffmpeg imagemagick ghostscript libreoffice"
    } else if Path::new("/usr/bin/dnf").exists() {
        "sudo dnf install ffmpeg ImageMagick ghostscript libreoffice"
    } else if Path::new("/usr/bin/pacman").exists() {
        "sudo pacman -S ffmpeg imagemagick ghostscript libreoffice-fresh"
    } else {
        "install ffmpeg, ImageMagick, Ghostscript and LibreOffice with your package manager"
    }
}

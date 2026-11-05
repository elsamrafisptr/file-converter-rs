use super::*;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn data_dirs() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = dirs::data_dir().into_iter().collect();
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    v.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    v
}

/// Extension -> MIME types, from the same shared-mime-info database Dolphin uses.
fn mime_types() -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for d in data_dirs() {
        let Ok(text) = std::fs::read_to_string(d.join("mime/globs2")) else { continue };
        for line in text.lines().filter(|l| !l.starts_with('#')) {
            let mut it = line.split(':');
            let (Some(_weight), Some(mime), Some(glob)) = (it.next(), it.next(), it.next()) else { continue };
            if let Some(ext) = glob.strip_prefix("*.").filter(|e| !e.contains(['*', '?', '['])) {
                let e = map.entry(ext.to_lowercase()).or_default();
                if !e.iter().any(|m| m == mime) {
                    e.push(mime.to_string());
                }
            }
        }
    }
    map
}

fn servicemenu_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("kio/servicemenus")
}

fn nautilus_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("nautilus/scripts/File Converter")
}

/// Desktop Entry `Exec` argument: quoted per the spec, then escaped as a string value.
fn exec_arg(s: &str) -> String {
    let quoted =
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$"));
    quoted.replace('\\', "\\\\").replace('%', "%%")
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Where the Linux packages put the default menus for every user.
const SYSTEM_DIR: &str = "/usr/share/kio/servicemenus";

fn is_ours(name: &str) -> bool {
    name.starts_with("file-converter-") && name.ends_with(".desktop")
}

fn our_menus(dir: &Path) -> Vec<String> {
    let entries = std::fs::read_dir(dir).into_iter().flatten().flatten();
    entries.map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| is_ours(n)).collect()
}

fn remove_menus(dir: &Path) -> R<()> {
    our_menus(dir).iter().try_for_each(|n| std::fs::remove_file(dir.join(n)).map_err(|e| e.to_string()))
}

/// Writes one Dolphin service menu per preset plus the settings entry; returns the file names.
fn write_menus(dir: &Path, presets: &[Preset], exe_s: &str) -> R<(HashSet<String>, String)> {
    let mimes = mime_types();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut written = HashSet::new();
    let mut unknown = std::collections::BTreeSet::new();
    let mut all_types: Vec<&str> = Vec::new();
    // Dolphin sorts a submenu's entries by action key: `p000…` keeps preset order, `zz-settings` goes last.
    let mut write = |name: &str, types: &[&str], actions: &str, key: &str, label: &str, icon: &str, exec: &str| {
        let body = format!(
            "[Desktop Entry]\nType=Service\nMimeType={};\nActions={actions};\nX-KDE-Submenu=File Converter\n\
                 X-KDE-Priority=TopLevel\n\n[Desktop Action {key}]\nName={label}\nIcon={icon}\nExec={exec}\n",
            types.join(";")
        );
        let path = dir.join(name);
        written.insert(name.to_string());
        std::fs::write(&path, body).map_err(|e| e.to_string())?;
        // KF6 ignores local service menus that are not executable.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
    };
    for (i, p) in presets.iter().enumerate() {
        let mut types: Vec<&str> = Vec::new();
        for ext in &p.inputs {
            match mimes.get(&ext.to_lowercase()) {
                // `.ts` is also Qt Linguist/TypeScript: never offer media presets on text files.
                Some(m) => types.extend(m.iter().map(String::as_str).filter(|m| !m.starts_with("text/"))),
                None => {
                    unknown.insert(ext.clone());
                }
            }
        }
        types.sort();
        types.dedup();
        if types.is_empty() {
            continue;
        }
        all_types.extend(&types);
        let key = format!("p{i:03}");
        let exec = format!("{} convert --preset {} %F", exec_arg(&exe_s), exec_arg(&p.name));
        write(
            &format!("file-converter-{i:03}.desktop"),
            &types,
            &key,
            &key,
            &p.name.replace('/', " › "),
            "document-export",
            &exec,
        )?;
    }
    all_types.sort();
    all_types.dedup();
    if !all_types.is_empty() {
        let exec = format!("{} settings", exec_arg(&exe_s));
        let actions = "_SEPARATOR_;zz-settings";
        write(
            "file-converter-settings.desktop",
            &all_types,
            actions,
            "zz-settings",
            SETTINGS_LABEL,
            "configure",
            &exec,
        )?;
    }
    let mut msg = format!("Dolphin: {} service menus in {}", presets.len(), dir.display());
    if !unknown.is_empty() {
        let list: Vec<_> = unknown.into_iter().collect();
        msg += &format!("\n  (no MIME type known for: {})", list.join(", "));
    }
    Ok((written, msg))
}

pub fn install(presets: &[Preset]) -> R<String> {
    uninstall()?;
    let exe = exe()?;
    let exe_s = exe.to_string_lossy();
    let dir = servicemenu_dir();
    let (written, mut msg) = write_menus(&dir, presets, &exe_s)?;
    // Same-named user files shadow the package's system-wide menus; an entry without actions hides one.
    for sys in data_dirs().iter().skip(1) {
        for name in our_menus(&sys.join("kio/servicemenus")).into_iter().filter(|n| !written.contains(n)) {
            std::fs::write(dir.join(name), "[Desktop Entry]\nType=Service\nActions=\n").map_err(|e| e.to_string())?;
        }
    }

    if crate::tools::tool(&["nautilus"]).is_ok() {
        let root = nautilus_dir();
        for p in presets {
            let path = root.join(&p.name);
            std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
            let script =
                format!("#!/bin/sh\nexec {} convert --preset {} \"$@\"\n", sh_quote(&exe_s), sh_quote(&p.name));
            std::fs::write(&path, script).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        }
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let settings = root.join(SETTINGS_LABEL);
        std::fs::write(&settings, format!("#!/bin/sh\nexec {} settings\n", sh_quote(&exe_s)))
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
        msg += &format!("\nNautilus: scripts in {}", root.display());
    }
    Ok(msg)
}

pub fn uninstall() -> R<String> {
    remove_menus(&servicemenu_dir())?;
    let n = nautilus_dir();
    if n.exists() {
        std::fs::remove_dir_all(&n).map_err(|e| e.to_string())?;
    }
    Ok("Context menu entries removed.".into())
}

/// Default-preset menus for all users (run as root by the .deb/.rpm scripts).
pub fn install_system() -> R<String> {
    let dir = Path::new(SYSTEM_DIR);
    remove_menus(dir)?;
    let exe = exe()?;
    write_menus(dir, &crate::preset::defaults().presets, &exe.to_string_lossy()).map(|(_, msg)| msg)
}

pub fn uninstall_system() -> R<String> {
    remove_menus(Path::new(SYSTEM_DIR)).map(|_| "System-wide context menu entries removed.".into())
}

#[cfg(test)]
#[test]
fn exec_quoting() {
    assert_eq!(exec_arg("Scale 75%/To Mp4"), "\"Scale 75%%/To Mp4\"");
    assert_eq!(exec_arg(r#"a"b$c\d"#), r#""a\\"b\\$c\\\\d""#);
}

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

// ---------------------------------------------------------------- Linux / BSD

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

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
        let quoted = format!(
            "\"{}\"",
            s.replace('\\', "\\\\").replace('"', "\\\"").replace('`', "\\`").replace('$', "\\$")
        );
        quoted.replace('\\', "\\\\").replace('%', "%%")
    }

    fn sh_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    pub fn install(presets: &[Preset]) -> R<String> {
        uninstall()?;
        let exe = exe()?;
        let exe_s = exe.to_string_lossy();
        let mimes = mime_types();

        let dir = servicemenu_dir();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut unknown = std::collections::BTreeSet::new();
        let mut all_types: Vec<&str> = Vec::new();
        // Dolphin sorts a submenu's entries by action key: `p000…` keeps preset order, `zz-settings` goes last.
        let write = |name: &str, types: &[&str], actions: &str, key: &str, label: &str, icon: &str, exec: &str| {
            let body = format!(
                "[Desktop Entry]\nType=Service\nMimeType={};\nActions={actions};\nX-KDE-Submenu=File Converter\n\
                 X-KDE-Priority=TopLevel\n\n[Desktop Action {key}]\nName={label}\nIcon={icon}\nExec={exec}\n",
                types.join(";")
            );
            let path = dir.join(name);
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
            write(&format!("file-converter-{i:03}.desktop"), &types, &key, &key, &p.name.replace('/', " › "), "document-export", &exec)?;
        }
        all_types.sort();
        all_types.dedup();
        if !all_types.is_empty() {
            let exec = format!("{} settings", exec_arg(&exe_s));
            let actions = "_SEPARATOR_;zz-settings";
            write("file-converter-settings.desktop", &all_types, actions, "zz-settings", SETTINGS_LABEL, "configure", &exec)?;
        }
        let mut msg = format!("Dolphin: {} service menus in {}", presets.len(), dir.display());
        if !unknown.is_empty() {
            let list: Vec<_> = unknown.into_iter().collect();
            msg += &format!("\n  (no MIME type known for: {})", list.join(", "));
        }

        if crate::convert::tool(&["nautilus"]).is_ok() {
            let root = nautilus_dir();
            for p in presets {
                let path = root.join(&p.name);
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                let script = format!(
                    "#!/bin/sh\nexec {} convert --preset {} \"$@\"\n",
                    sh_quote(&exe_s),
                    sh_quote(&p.name)
                );
                std::fs::write(&path, script).map_err(|e| e.to_string())?;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            }
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let settings = root.join(SETTINGS_LABEL);
            std::fs::write(&settings, format!("#!/bin/sh\nexec {} settings\n", sh_quote(&exe_s))).map_err(|e| e.to_string())?;
            std::fs::set_permissions(&settings, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            msg += &format!("\nNautilus: scripts in {}", root.display());
        }
        Ok(msg)
    }

    pub fn uninstall() -> R<String> {
        if let Ok(entries) = std::fs::read_dir(servicemenu_dir()) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("file-converter-") && name.ends_with(".desktop") {
                    std::fs::remove_file(e.path()).map_err(|e| e.to_string())?;
                }
            }
        }
        let n = nautilus_dir();
        if n.exists() {
            std::fs::remove_dir_all(&n).map_err(|e| e.to_string())?;
        }
        Ok("Context menu entries removed.".into())
    }

    #[cfg(test)]
    #[test]
    fn exec_quoting() {
        assert_eq!(exec_arg("Scale 75%/To Mp4"), "\"Scale 75%%/To Mp4\"");
        assert_eq!(exec_arg(r#"a"b$c\d"#), r#""a\\"b\\$c\\\\d""#);
    }
}

// ---------------------------------------------------------------- Windows

#[cfg(windows)]
mod imp {
    use super::*;
    use winreg::{enums::*, RegKey};

    const VERB: &str = "FileConverter";

    #[derive(Default)]
    struct Node {
        label: String,
        preset: Option<String>,
        children: Vec<Node>,
    }

    fn classes() -> R<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS)
            .map_err(|e| e.to_string())
    }

    fn write(key: &RegKey, nodes: &[Node], exe: &str) -> std::io::Result<()> {
        let (shell, _) = key.create_subkey("shell")?;
        for (i, n) in nodes.iter().enumerate() {
            let (k, _) = shell.create_subkey(format!("{i:03}"))?;
            k.set_value("MUIVerb", &n.label)?;
            match &n.preset {
                Some(p) => {
                    let (c, _) = k.create_subkey("command")?;
                    c.set_value("", &format!("\"{exe}\" convert --preset \"{p}\" \"%1\""))?;
                }
                None => {
                    k.set_value("SubCommands", &"")?;
                    write(&k, &n.children, exe)?;
                }
            }
        }
        Ok(())
    }

    pub fn install(presets: &[Preset]) -> R<String> {
        uninstall()?;
        let exe = exe()?.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        let classes = classes()?;
        let mut exts: Vec<String> = presets.iter().flat_map(|p| p.inputs.iter().map(|e| e.to_lowercase())).collect();
        exts.sort();
        exts.dedup();
        for ext in &exts {
            // Menu tree: "Scale 75%/To Mp4" -> folder "Scale 75%" -> "To Mp4".
            let mut root = Node::default();
            for p in presets.iter().filter(|p| p.inputs.iter().any(|i| i.eq_ignore_ascii_case(ext))) {
                let parts: Vec<&str> = p.name.split('/').collect();
                let mut node = &mut root;
                for part in &parts[..parts.len() - 1] {
                    let idx = match node.children.iter().position(|c| c.preset.is_none() && c.label == *part) {
                        Some(i) => i,
                        None => {
                            node.children.push(Node { label: part.to_string(), ..Default::default() });
                            node.children.len() - 1
                        }
                    };
                    node = &mut node.children[idx];
                }
                node.children.push(Node {
                    label: parts[parts.len() - 1].to_string(),
                    preset: Some(p.name.clone()),
                    children: vec![],
                });
            }
            let (k, _) = classes
                .create_subkey(format!(r"SystemFileAssociations\.{ext}\shell\{VERB}"))
                .map_err(|e| e.to_string())?;
            k.set_value("MUIVerb", &"File Converter").map_err(|e| e.to_string())?;
            k.set_value("SubCommands", &"").map_err(|e| e.to_string())?;
            k.set_value("Icon", &exe).map_err(|e| e.to_string())?;
            write(&k, &root.children, &exe).map_err(|e| e.to_string())?;
            // Last entry, separated, like FileConverter's own menu.
            let (s, _) = k.create_subkey(r"shell\zzz-settings").map_err(|e| e.to_string())?;
            s.set_value("MUIVerb", &SETTINGS_LABEL).map_err(|e| e.to_string())?;
            s.set_value("CommandFlags", &0x20u32).map_err(|e| e.to_string())?; // ECF_SEPARATORBEFORE
            let (c, _) = s.create_subkey("command").map_err(|e| e.to_string())?;
            c.set_value("", &format!("\"{exe}\" settings")).map_err(|e| e.to_string())?;
        }
        Ok(format!("Explorer: context menu added for {} extensions (Windows 11: under \"Show more options\").", exts.len()))
    }

    pub fn uninstall() -> R<String> {
        let classes = classes()?;
        if let Ok(sfa) = classes.open_subkey_with_flags("SystemFileAssociations", KEY_ALL_ACCESS) {
            let keys: Vec<String> = sfa.enum_keys().flatten().filter(|k| k.starts_with('.')).collect();
            for k in keys {
                let _ = sfa.delete_subkey_all(format!(r"{k}\shell\{VERB}"));
            }
        }
        Ok("Context menu entries removed.".into())
    }
}

// ---------------------------------------------------------------- macOS

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    fn workflow_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_default().join("Library/Services/File Converter.workflow")
    }

    fn xml(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>NSServices</key>
	<array>
		<dict>
			<key>NSMenuItem</key>
			<dict><key>default</key><string>File Converter…</string></dict>
			<key>NSMessage</key><string>runWorkflowAsService</string>
			<key>NSRequiredContext</key>
			<dict><key>NSApplicationIdentifier</key><string>com.apple.finder</string></dict>
			<key>NSSendFileTypes</key>
			<array><string>public.item</string></array>
		</dict>
	</array>
</dict>
</plist>
"#;

    fn document(command: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>AMApplicationBuild</key><string>523</string>
	<key>AMApplicationVersion</key><string>2.10</string>
	<key>AMDocumentVersion</key><string>2</string>
	<key>actions</key>
	<array>
		<dict>
			<key>action</key>
			<dict>
				<key>AMAccepts</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Optional</key><true/>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>AMActionVersion</key><string>2.0.3</string>
				<key>AMApplication</key><array><string>Automator</string></array>
				<key>AMParameterProperties</key>
				<dict>
					<key>COMMAND_STRING</key><dict/>
					<key>CheckedForUserDefaultShell</key><dict/>
					<key>inputMethod</key><dict/>
					<key>shell</key><dict/>
					<key>source</key><dict/>
				</dict>
				<key>AMProvides</key>
				<dict>
					<key>Container</key><string>List</string>
					<key>Types</key><array><string>com.apple.cocoa.string</string></array>
				</dict>
				<key>ActionBundlePath</key><string>/System/Library/Automator/Run Shell Script.action</string>
				<key>ActionName</key><string>Run Shell Script</string>
				<key>ActionParameters</key>
				<dict>
					<key>COMMAND_STRING</key><string>{command}</string>
					<key>CheckedForUserDefaultShell</key><true/>
					<key>inputMethod</key><integer>1</integer>
					<key>shell</key><string>/bin/sh</string>
					<key>source</key><string></string>
				</dict>
				<key>BundleIdentifier</key><string>com.apple.RunShellScript</string>
				<key>CFBundleVersion</key><string>2.0.3</string>
				<key>CanShowSelectedItemsWhenRun</key><false/>
				<key>CanShowWhenRun</key><true/>
				<key>Category</key><array><string>AMCategoryUtilities</string></array>
				<key>Class Name</key><string>RunShellScriptAction</string>
				<key>InputUUID</key><string>6B1C2D4E-1F00-4C59-9E0A-2C1A0F5E0001</string>
				<key>OutputUUID</key><string>6B1C2D4E-1F00-4C59-9E0A-2C1A0F5E0002</string>
				<key>UUID</key><string>6B1C2D4E-1F00-4C59-9E0A-2C1A0F5E0003</string>
				<key>UnlocalizedApplications</key><array><string>Automator</string></array>
				<key>isViewVisible</key><integer>1</integer>
			</dict>
			<key>isViewVisible</key><integer>1</integer>
		</dict>
	</array>
	<key>connectors</key><dict/>
	<key>workflowMetaData</key>
	<dict>
		<key>applicationBundleIDsByPath</key><dict/>
		<key>applicationPaths</key><array/>
		<key>inputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>outputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>presentationMode</key><integer>15</integer>
		<key>processesInput</key><integer>0</integer>
		<key>serviceInputTypeIdentifier</key><string>com.apple.Automator.fileSystemObject</string>
		<key>serviceOutputTypeIdentifier</key><string>com.apple.Automator.nothing</string>
		<key>serviceProcessesInput</key><integer>0</integer>
		<key>systemImageName</key><string>NSActionTemplate</string>
		<key>useAutomaticInputType</key><integer>0</integer>
		<key>workflowTypeIdentifier</key><string>com.apple.Automator.servicesMenu</string>
	</dict>
</dict>
</plist>
"#
        )
    }

    pub fn install(_presets: &[Preset]) -> R<String> {
        let exe = exe()?;
        let contents = workflow_dir().join("Contents");
        std::fs::create_dir_all(&contents).map_err(|e| e.to_string())?;
        let cmd = format!("'{}' pick \"$@\"", exe.to_string_lossy().replace('\'', r"'\''"));
        std::fs::write(contents.join("Info.plist"), INFO_PLIST).map_err(|e| e.to_string())?;
        std::fs::write(contents.join("document.wflow"), document(&xml(&cmd))).map_err(|e| e.to_string())?;
        // Refresh the Services menu cache.
        let _ = std::process::Command::new("/System/Library/CoreServices/pbs").arg("-update").output();
        Ok(format!(
            "Finder: Quick Action \"File Converter…\" installed in {} (right-click › Quick Actions).",
            workflow_dir().display()
        ))
    }

    pub fn uninstall() -> R<String> {
        let d = workflow_dir();
        if d.exists() {
            std::fs::remove_dir_all(&d).map_err(|e| e.to_string())?;
        }
        Ok("Context menu entries removed.".into())
    }
}

pub use imp::{install, uninstall};

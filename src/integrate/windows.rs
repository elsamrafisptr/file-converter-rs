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
        let (k, _) =
            classes.create_subkey(format!(r"SystemFileAssociations\.{ext}\shell\{VERB}")).map_err(|e| e.to_string())?;
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

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

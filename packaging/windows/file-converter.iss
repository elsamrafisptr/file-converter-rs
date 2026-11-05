; Inno Setup script. Built by .github/workflows/release.yml:
;   iscc /DAppVersion=x.y.z packaging\windows\file-converter.iss
; Expects target\release\file-converter.exe and dist\tools\ (ffmpeg.exe + portable ImageMagick).

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6E1B7C3A-2F4D-4B8E-9A51-0C7D3E2F9B14}
AppName=File Converter
AppVersion={#AppVersion}
AppPublisher=Elsam Rafi Saputra
AppPublisherURL=https://github.com/elsamrafisptr/file-converter-rs
; Per-user install: no admin rights, and the context menu is registered for this user.
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\File Converter
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
LicenseFile=..\..\LICENSE
SetupIconFile=..\icons\file-converter.ico
UninstallDisplayIcon={app}\file-converter.exe
OutputDir=..\..\dist
OutputBaseFilename=file-converter-setup-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: ghostscript; Description: "Install Ghostscript with winget (needed to turn PDF pages into images)"
Name: libreoffice; Description: "Install LibreOffice with winget (needed for Word, Excel and PowerPoint files, ~350 MB)"; Flags: unchecked

[Files]
Source: "..\..\target\release\file-converter.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\dist\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\File Converter"; Filename: "{app}\file-converter.exe"; Parameters: "settings"

[Run]
Filename: "{localappdata}\Microsoft\WindowsApps\winget.exe"; Parameters: "install -e --id ArtifexSoftware.GhostScript --silent --accept-package-agreements --accept-source-agreements"; StatusMsg: "Installing Ghostscript..."; Tasks: ghostscript; Flags: runhidden waituntilterminated skipifdoesntexist
Filename: "{localappdata}\Microsoft\WindowsApps\winget.exe"; Parameters: "install -e --id TheDocumentFoundation.LibreOffice --silent --accept-package-agreements --accept-source-agreements"; StatusMsg: "Installing LibreOffice..."; Tasks: libreoffice; Flags: runhidden waituntilterminated skipifdoesntexist
Filename: "{app}\file-converter.exe"; Parameters: "install"; StatusMsg: "Adding the right-click menu..."; Flags: runhidden waituntilterminated
Filename: "{app}\file-converter.exe"; Parameters: "settings"; Description: "Open File Converter settings"; Flags: postinstall nowait skipifsilent unchecked

[UninstallRun]
Filename: "{app}\file-converter.exe"; Parameters: "uninstall"; Flags: runhidden waituntilterminated; RunOnceId: "RemoveContextMenu"

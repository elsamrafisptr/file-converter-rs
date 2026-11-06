# Changelog

All notable changes to File Converter. Also on the website: https://elsamrafisptr.github.io/file-converter-rs/changelog.html

## 0.1.1 — 2026-11-05

### Changed
- Windows installer is half the size: 61 MB instead of 124 MB. It now only includes the ImageMagick program File Converter uses.
- macOS: install `ffmpeg-full` from Homebrew. The plain `ffmpeg` package cannot make WebM, OGG and OGV files.

### Added
- The app now warns you when your ffmpeg is missing something the presets need.
- Every installer is tested automatically before a release: install, add the menu, convert files, uninstall.

### Fixed
- PDF to PNG: pages are always named `name-1.png`, `name-2.png`, … even when `name.png` already exists.

## 0.1.0 — 2026-11-05

First release.

- Right-click menu on Windows (Explorer), Linux (Dolphin and Files/Nautilus) and macOS (Finder Quick Action).
- 73 ready-made presets from the original FileConverter: video, audio, images, PDF and Office documents, plus scale, rotate and speed/pitch presets.
- Settings window to add, change and reorder presets, choose file types and conversion quality.
- Several files are converted at the same time, with a notification when done.
- Never overwrites a file: `video.mp4` becomes `video (2).mp4`.
- Optional: move the original file to a "Conversion Archive" folder, or delete it, after conversion.
- Installers: Windows setup, macOS app (.dmg), Linux .deb and .tar.gz.

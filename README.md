# file-converter

A Rust port of [Tichau/FileConverter](https://github.com/Tichau/FileConverter) ([file-converter.io](https://file-converter.io/)):
right-click files in your file manager → **File Converter** → pick a preset (To Mp4, To Mp3, To Jpg, Scale 75% › To Webm, …).

**Website and user guide: https://elsamrafisptr.github.io/file-converter-rs/** (source in `docs/`).

Same design as the original: a small app that runs the real converters with tuned arguments.

| Input → output | Engine (original) | Engine (here) |
|---|---|---|
| audio / video / gif from video | ffmpeg | ffmpeg (same arguments, ported) |
| images → jpg/png/webp/avif/ico/pdf/gif | Magick.NET | ImageMagick CLI (`magick`, or `convert` on IM6) |
| pdf → png pages | Magick.NET + Ghostscript | Ghostscript |
| doc/docx/odt/xls/xlsx/ods/ppt/pptx/odp → pdf/png | Microsoft Office interop | LibreOffice (`soffice --headless`) |
| context menu | SharpShell COM extension | Dolphin service menus, Nautilus scripts, Explorer registry verbs, Finder Quick Action |

The 70+ default presets come from the original `Settings.default.xml` (CD-audio extraction is left out).

## Install

Download the file for your system from the [latest release](https://github.com/elsamrafisptr/file-converter-rs/releases/latest), then:

| System | Download | Install |
|---|---|---|
| **Windows 10/11** | `file-converter-setup-x64.exe` | Run it. ffmpeg and ImageMagick are included; the installer can also fetch Ghostscript (PDF) and LibreOffice (Office files) with winget. No admin rights needed. |
| **Ubuntu, Kubuntu, Debian, Mint, Pop!_OS** | `file-converter_<version>_amd64.deb` | Double-click it, or `sudo apt install ./file-converter_*_amd64.deb`. ffmpeg, ImageMagick and Ghostscript are installed automatically. |
| **Other Linux** | `file-converter-linux-x86_64.tar.gz` | Extract it and run `./install.sh`, then install the converters it lists (e.g. `sudo dnf install ffmpeg ImageMagick ghostscript`). |
| **macOS 11+** | `file-converter-macos.dmg` | Drag **File Converter** to Applications and open it once (first time: right-click › **Open**, since the app is not notarized). Then install the converters: `brew install ffmpeg-full imagemagick ghostscript` ([Homebrew](https://brew.sh); the plain `ffmpeg` formula lacks the Vorbis/Theora encoders used by WebM/OGG/OGV). |

Then right-click any supported file:

- **Windows**: **File Converter** › preset (Windows 11: under *Show more options*).
- **Linux, Dolphin**: **File Converter** › preset. **Nautilus**: open *File Converter* from the app menu once, then right-click › **Scripts** › **File Converter**.
- **macOS**: **Quick Actions** › **File Converter…**, then pick the preset.

Converted files appear next to the originals. The **File Converter** app (Start menu, application menu or Applications) opens the settings and lists any missing converter with the command that installs it. LibreOffice is only needed for Word/Excel/PowerPoint files.

Uninstall: Windows › *Installed apps* › File Converter; Debian/Ubuntu › `sudo apt remove file-converter`; tarball › `./install.sh --uninstall`; macOS › `"/Applications/File Converter.app/Contents/MacOS/file-converter" uninstall`, then delete the app.

### Building from source

```sh
cargo install --path .
file-converter install      # adds the context menu for the current user; `uninstall` removes it
```

Installers are built by `.github/workflows/release.yml` when a `v*` tag is pushed (`packaging/` holds the .deb scripts, the Inno Setup script and the macOS app bundle script).

What the context menu consists of:

- **Linux (KDE Dolphin)**: one service menu per preset, filtered by MIME type, under *File Converter* (`~/.local/share/kio/servicemenus/`; the .deb also installs the defaults for all users in `/usr/share/kio/servicemenus/`).
  **GNOME Nautilus** (if installed): scripts in `~/.local/share/nautilus/scripts/File Converter/`.
- **Windows**: a cascading *File Converter* menu for each supported extension under `HKCU\Software\Classes\SystemFileAssociations`.
- **macOS**: a Finder Quick Action *File Converter…* (`~/Library/Services`) that asks which preset to use.

Every menu ends with **Configure presets…**, which opens the settings window (also: `file-converter settings`, or `file-converter` with no arguments):

- add, duplicate, delete and reorder presets (a `/` in the name makes a sub-menu);
- choose the output type and which input file types the preset appears on;
- output file name template and what to do with the input afterwards (keep / archive / delete);
- conversion settings for the output type: video quality, encoding speed, audio bitrate, scale, rotation, FPS, MP3 VBR/CBR, WAV bit depth, channels, image quality, max size, …, or fully custom ffmpeg arguments;
- number of simultaneous conversions.

**Save** writes `presets.toml` and refreshes the context menu.

## Command line

```sh
file-converter list [FILES…]                  # presets (accepting all FILES)
file-converter convert --preset "To Mp4" a.mov b.avi
file-converter config                          # path of presets.toml
```

Jobs run in parallel (`max_simultaneous`), with progress bars in a terminal and a desktop notification when done.

## Presets

Presets live in `presets.toml` (`file-converter config` prints its path; `~/.config/file-converter/` on Linux), written with the defaults on first run. The settings window edits it; hand-editing works too (then run `file-converter install`). Delete it to restore the defaults.

```toml
max_simultaneous = 2

[[preset]]
name = "Scale 75%/To Mp4"     # "/" makes a sub-menu
output = "mp4"                # aac avi avif flac gif ico jpg mkv mp3 mp4 ogg ogv pdf png wav webm webp
inputs = ["mov", "avi", "mkv"]
video_scale = 0.75
# Optional, with defaults:
# output_template = "(p)(f)"  # (p) folder, (f)/(F) name, (i)/(I) input ext, (o)/(O) output ext,
#                             # (d0) parent folder name, (p:d)/(p:m)/(p:v)/(p:p) Documents/Music/Videos/Pictures
# post_action = "none"        # none | archive (moves the input to "Conversion Archive/") | delete
# enable_audio = true
# video_quality = 30          # x264 crf = 51 - q, vp9 crf = 63 - q, xvid q = 31 - q, theora 0-10
# video_speed = "medium"      # ultrafast … veryslow
# video_rotation = 0          # 90 = left, 180, 270 = right
# fps = 15                    # gif
# audio_bitrate = 160         # kbit/s, mapped to the nearest VBR quality
# audio_channels = 0          # 0 = keep
# audio_mode = "mp3vbr"       # mp3vbr | mp3cbr | wav8 | wav16 | wav24 | wav32
# image_quality = 80
# image_scale = 1.0
# image_rotation = 0          # 90 = left
# image_pow2 = false          # square power-of-two size (textures)
# image_max_size = 0          # longest side limit, 0 = none
# ffmpeg_custom = ""          # replaces all generated ffmpeg encoder arguments
```

Existing files are never overwritten: outputs become `name (2).ext`, `name (3).ext`, …

## Limitations

- **iOS / iPadOS**: not supported. iOS apps cannot launch external programs such as ffmpeg and Files has no context-menu extension point for this; it would need a separate Swift share extension with embedded codecs.
- Windows: Explorer starts one process per selected file, so `max_simultaneous` only applies within a single call.
- Debian/Ubuntu ImageMagick builds may block PDF writing by policy (`/etc/ImageMagick-*/policy.xml`, `pattern="PDF"`); "To Pdf" from images needs that rule relaxed.
- No hardware video encoding (CUDA/AMF).

## Contributing

Enable the repository hooks once per clone:

```sh
git config core.hooksPath .githooks
```

`pre-commit` checks staged files only and rejects any file over 5,000 tokens (tokens = bytes / 4, i.e. 20,000 bytes), with step-by-step instructions for splitting it. Files not written by hand or by agents are skipped: lockfiles, generated code (`*.generated.*`, `*.pb.go`, `*.min.js`, …), vendored/build directories, license texts and binary files. Add other such patterns to `is_ignored` in `.githooks/pre-commit`.

## License

GPL-3.0-or-later, like the original project.

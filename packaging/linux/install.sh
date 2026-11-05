#!/bin/sh
# Installs File Converter for the current user (no root needed). Uninstall: ./install.sh --uninstall
set -e
cd "$(dirname "$0")"
BIN="$HOME/.local/bin"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"

if [ "$1" = "--uninstall" ]; then
	[ -x "$BIN/file-converter" ] && "$BIN/file-converter" uninstall
	rm -f "$BIN/file-converter" "$DATA/applications/file-converter.desktop" \
		"$DATA/icons/hicolor/256x256/apps/file-converter.png"
	echo "File Converter removed."
	exit 0
fi

install -Dm755 file-converter "$BIN/file-converter"
install -Dm644 file-converter.png "$DATA/icons/hicolor/256x256/apps/file-converter.png"
mkdir -p "$DATA/applications"
# Absolute path: ~/.local/bin is not always in the desktop session's PATH.
sed "s#^Exec=file-converter#Exec=\"$BIN/file-converter\"#" file-converter.desktop >"$DATA/applications/file-converter.desktop"
"$BIN/file-converter" install
echo "Done: right-click a file › File Converter. Settings: 'File Converter' in the application menu."

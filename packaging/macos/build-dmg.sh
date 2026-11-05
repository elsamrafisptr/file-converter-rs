#!/bin/sh
# Builds dist/file-converter-macos.dmg with a universal "File Converter.app". Run on macOS:
#   packaging/macos/build-dmg.sh 0.1.0
set -e
VERSION="$1"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

STAGE="$ROOT/dist/dmg"
APP="$STAGE/File Converter.app"
rm -rf "$STAGE" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/file-converter" \
	target/aarch64-apple-darwin/release/file-converter target/x86_64-apple-darwin/release/file-converter
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist >"$APP/Contents/Info.plist"

ICONSET="$ROOT/dist/file-converter.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256; do
	sips -z $s $s packaging/icons/file-converter.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
	sips -z $((s * 2)) $((s * 2)) packaging/icons/file-converter.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/file-converter.icns"

# Ad-hoc signature: required for Apple Silicon; not notarized (needs a paid Apple Developer ID).
codesign --force --deep --sign - "$APP"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "File Converter" -srcfolder "$STAGE" -ov -format UDZO "$ROOT/dist/file-converter-macos.dmg"

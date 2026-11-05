#!/usr/bin/env bash
# End-to-end check of a built/installed File Converter: converts generated samples with
# representative presets and fails on any missing output. Used by the release workflow.
#   packaging/smoke-test.sh path/to/file-converter
# Needs ffmpeg in PATH (to create the samples). Optional checks: SMOKE_AVIF=1, SMOKE_PDF=1.
set -euo pipefail
BIN="$1"
WORK="$(mktemp -d)"
cd "$WORK"

ffmpeg -v error -f lavfi -i testsrc=size=321x241:rate=25 -f lavfi -i sine=frequency=440 -t 2 -c:v mpeg4 -c:a aac clip.mov
ffmpeg -v error -f lavfi -i testsrc=size=320x240 -frames:v 1 photo.png
ffmpeg -v error -f lavfi -i sine=frequency=300 -t 2 song.flac

check() { # preset input expected-output
	"$BIN" convert --preset "$1" "$2"
	[ -s "$3" ] || { echo "FAIL: '$1' on $2 did not produce $3" >&2; exit 1; }
	echo "ok: $1 -> $3"
}

check "To Mp4" clip.mov clip.mp4
check "To Webm" clip.mov clip.webm
check "To Avi" clip.mov clip.avi
check "To Ogv" clip.mov clip.ogv
check "To Gif" clip.mov clip.gif
check "To Ogg" clip.mov clip.ogg
check "To Mp3" song.flac song.mp3
check "To Wav" song.flac song.wav
check "To Aac" song.flac song.aac
check "To Jpg" photo.png photo.jpg
check "To Webp" photo.png photo.webp
check "To Ico" photo.png photo.ico
check "Rotate left/To Png" photo.png "photo (2).png"
if [ -n "${SMOKE_AVIF:-}" ]; then check "To Avif" photo.png photo.avif; fi
if [ -n "${SMOKE_PDF:-}" ]; then
	check "To Pdf" photo.png photo.pdf
	check "To Png (paged)" photo.pdf photo-1.png
fi

# Never overwrite: a second run must create a numbered copy.
check "To Mp4" clip.mov "clip (2).mp4"
# Unsupported input must fail without output.
if "$BIN" convert --preset "To Mp4" photo.png 2>/dev/null; then
	echo "FAIL: 'To Mp4' accepted a PNG" >&2
	exit 1
fi

cd /
rm -rf "$WORK"
echo "smoke test passed"

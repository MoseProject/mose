#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 8 ]; then
    echo "usage: $0 BUILD_DIR FFMPEG_DIR GSTREAMER_DIR PYTHON_RUNTIME OUTPUT_APP INVENTORY VERSION ARCH" >&2
    exit 2
fi

BUILD_DIR="$(cd "$1" && pwd)"
FFMPEG_DIR="$(cd "$2" && pwd)"
GSTREAMER_DIR="$(cd "$3" && pwd)"
PYTHON_RUNTIME="$(cd "$4" && pwd)"
OUTPUT_APP="$5"
INVENTORY="$6"
VERSION="$7"
ARCH="$8"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

case "$OUTPUT_APP" in
    *.app) ;;
    *) echo "OUTPUT_APP must end in .app" >&2; exit 2 ;;
esac

rm -rf -- "$OUTPUT_APP"
CONTENTS="$OUTPUT_APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
RUNTIME="$RESOURCES/runtime"
GST_ROOT="$RUNTIME/gstreamer"
mkdir -p "$MACOS" "$RESOURCES" "$RUNTIME/bin" \
    "$GST_ROOT/lib" "$GST_ROOT/libexec/gstreamer-1.0" "$RESOURCES/python"

install -m 0755 "$BUILD_DIR/oca-desktop" "$MACOS/oca"
install -m 0755 "$BUILD_DIR/ytbridge" "$MACOS/ytbridge"
install -m 0755 "$PYTHON_RUNTIME/tools/deno" "$MACOS/deno"
cp -a "$BUILD_DIR/espeak-ng-data" "$RESOURCES/"
cp -a "$PYTHON_RUNTIME/lib" "$RESOURCES/python/"
install -m 0755 "$FFMPEG_DIR/bin/ffmpeg" "$RUNTIME/bin/ffmpeg"
python3 "$SCRIPT_DIR/verify_ffmpeg_runtime.py" "$FFMPEG_DIR/bin/ffmpeg" --platform macos
python3 "$SCRIPT_DIR/collect_gstreamer_plugins.py" \
    "$GSTREAMER_DIR/lib/gstreamer-1.0" "$GST_ROOT/lib/gstreamer-1.0" --platform macos
install -m 0755 "$GSTREAMER_DIR/libexec/gstreamer-1.0/gst-plugin-scanner" \
    "$GST_ROOT/libexec/gstreamer-1.0/gst-plugin-scanner"

sed "s/__OCA_VERSION__/$VERSION/g" "$SCRIPT_DIR/macos-Info.plist" > "$CONTENTS/Info.plist"
ICONSET="$(mktemp -d)/oca.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" "$REPO_ROOT/logo.png" \
        --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" "$REPO_ROOT/logo.png" \
        --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$RESOURCES/oca.icns"
rm -rf -- "$(dirname "$ICONSET")"

python3 "$SCRIPT_DIR/fetch_models.py" "$RESOURCES"
cp "$SCRIPT_DIR/bundle-manifest.json" "$RESOURCES/"
python3 "$SCRIPT_DIR/generate_third_party_notices.py" "$RESOURCES/licenses"
mkdir -p "$RESOURCES/licenses/fonts"
cp "$REPO_ROOT/LICENSE" "$RESOURCES/licenses/oca.txt"
find "$REPO_ROOT/crates/core/assets/fonts" -name OFL.txt -print0 \
    | while IFS= read -r -d '' license; do
        family="$(basename "$(dirname "$license")")"
        cp "$license" "$RESOURCES/licenses/fonts/${family}-OFL.txt"
    done

{
    "$FFMPEG_DIR/bin/ffmpeg" -version | head -1
    "$GSTREAMER_DIR/bin/gst-launch-1.0" --version | head -2
    "$PYTHON_RUNTIME/bin/python3.10" --version
    "$MACOS/deno" --version | head -1
    PYTHONDONTWRITEBYTECODE=1 "$PYTHON_RUNTIME/bin/python3.10" -c \
        "import importlib.metadata, yt_dlp.version; print('yt-dlp ' + yt_dlp.version.__version__ + '; yt-dlp-ejs ' + importlib.metadata.version('yt-dlp-ejs'))"
} > "$RESOURCES/RUNTIME_VERSIONS.txt"

# Homebrew bottles commonly mark dylibs read-only. The files in this private app copy must be
# writable so install_name_tool, xattr and codesign can rewrite/sign them.
chmod -R u+w "$OUTPUT_APP"

BREW_PREFIX="$(brew --prefix)"
OPENH264_DIR="$(brew --prefix openh264)"
python3 "$SCRIPT_DIR/bundle_macos_dylibs.py" "$OUTPUT_APP" \
    --architecture "$ARCH" \
    --search-root "$BUILD_DIR" \
    --search-root "$FFMPEG_DIR/lib" \
    --search-root "$GSTREAMER_DIR/lib" \
    --search-root "$PYTHON_RUNTIME/lib" \
    --search-root "$BREW_PREFIX/lib" \
    --search-root "$OPENH264_DIR/lib"

python3 "$SCRIPT_DIR/verify_ffmpeg_runtime.py" "$RUNTIME/bin/ffmpeg" --platform macos

bash "$SCRIPT_DIR/sign_macos_app.sh" "$OUTPUT_APP"
python3 "$SCRIPT_DIR/generate_dependency_inventory.py" "$INVENTORY" \
    --bundle-root "$OUTPUT_APP"
python3 "$SCRIPT_DIR/validate_bundle.py" "$OUTPUT_APP" --platform macos \
    --inventory "$INVENTORY"

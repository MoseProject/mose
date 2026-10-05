#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "usage: $0 APP INVENTORY OUTPUT_DMG" >&2
    exit 2
fi

APP="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
INVENTORY="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
OUTPUT_PARENT="$(dirname "$3")"
mkdir -p "$OUTPUT_PARENT"
OUTPUT_DMG="$(cd "$OUTPUT_PARENT" && pwd)/$(basename "$3")"
case "$OUTPUT_DMG" in
    *.dmg) ;;
    *) echo "OUTPUT_DMG must end in .dmg" >&2; exit 2 ;;
esac

WORK_DIR="$(mktemp -d)"
STAGING="$WORK_DIR/staging"
MOUNT_POINT="$WORK_DIR/mount"
READ_WRITE_DMG="$WORK_DIR/oca-read-write.dmg"
DEVICE=""
cleanup() {
    if [ -n "$DEVICE" ]; then
        hdiutil detach "$DEVICE" -quiet || true
    fi
    rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT
mkdir -p "$STAGING" "$MOUNT_POINT"
cp -a "$APP" "$STAGING/Oca.app"
cp "$INVENTORY" "$STAGING/DEPENDENCIES.json"
ln -s /Applications "$STAGING/Applications"

# `hdiutil create -srcfolder` uses copy-helper, which can stall for minutes on the thousands of
# small Python files in this bundle. Populate a writable image directly, then convert it
# to the compressed read-only release image.
CONTENT_KIB="$(du -sk "$STAGING" | awk '{print $1}')"
IMAGE_MIB=$(((CONTENT_KIB * 12 / 10 + 65536 + 1023) / 1024))
hdiutil create -quiet -size "${IMAGE_MIB}m" -fs "Journaled HFS+" \
    -volname oca -type UDIF "$READ_WRITE_DMG"
DEVICE="$(hdiutil attach -nobrowse -mountpoint "$MOUNT_POINT" "$READ_WRITE_DMG" \
    | awk '/Apple_HFS/ {print $1; exit}')"
[ -n "$DEVICE" ] || { echo "failed to mount writable DMG" >&2; exit 1; }
cp -a "$STAGING/." "$MOUNT_POINT/"
hdiutil detach "$DEVICE" -quiet
DEVICE=""

rm -f -- "$OUTPUT_DMG"
hdiutil convert -quiet -format UDZO -ov -o "$OUTPUT_DMG" "$READ_WRITE_DMG"
hdiutil imageinfo "$OUTPUT_DMG" >/dev/null

#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 BUNDLE_DIR VERSION OUTPUT_DEB [MANIFEST]" >&2
    exit 2
fi

BUNDLE_DIR="$(cd "$1" && pwd)"
VERSION="$2"
OUTPUT_PARENT="$(dirname "$3")"
mkdir -p "$OUTPUT_PARENT"
OUTPUT_DEB="$(cd "$OUTPUT_PARENT" && pwd)/$(basename "$3")"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST="${4:-$SCRIPT_DIR/bundle-manifest.json}"

command -v dpkg-deb >/dev/null
python3 "$SCRIPT_DIR/validate_bundle.py" "$BUNDLE_DIR" \
    --platform linux --manifest "$MANIFEST"

PACKAGE_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$PACKAGE_ROOT"' EXIT

mkdir -p \
    "$PACKAGE_ROOT/DEBIAN" \
    "$PACKAGE_ROOT/opt/oca" \
    "$PACKAGE_ROOT/usr/bin" \
    "$PACKAGE_ROOT/usr/share/applications" \
    "$PACKAGE_ROOT/usr/share/doc/oca" \
    "$PACKAGE_ROOT/usr/share/icons/hicolor/1024x1024/apps"

cp -a "$BUNDLE_DIR/." "$PACKAGE_ROOT/opt/oca/"
ln -s /opt/oca/oca-desktop "$PACKAGE_ROOT/usr/bin/oca"
install -m 0644 "$SCRIPT_DIR/oca-deb.desktop" \
    "$PACKAGE_ROOT/usr/share/applications/oca.desktop"
install -m 0644 "$REPO_ROOT/logo.png" \
    "$PACKAGE_ROOT/usr/share/icons/hicolor/1024x1024/apps/oca.png"
install -m 0644 "$REPO_ROOT/LICENSE" "$PACKAGE_ROOT/usr/share/doc/oca/copyright"

cat > "$PACKAGE_ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
fi
EOF

cat > "$PACKAGE_ROOT/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
fi
EOF

chmod 0755 "$PACKAGE_ROOT/DEBIAN/postinst" "$PACKAGE_ROOT/DEBIAN/postrm"
(cd "$PACKAGE_ROOT" && find opt usr -type f -print0 | sort -z | xargs -0 md5sum) \
    > "$PACKAGE_ROOT/DEBIAN/md5sums"

INSTALLED_SIZE="$(du -sk "$PACKAGE_ROOT" | cut -f1)"
cat > "$PACKAGE_ROOT/DEBIAN/control" <<EOF
Package: oca
Version: $VERSION
Section: video
Priority: optional
Architecture: amd64
Depends: libc6 (>= 2.28)
Installed-Size: $INSTALLED_SIZE
Maintainer: Lucas Gomes <lucasgsm88@gmail.com>
Homepage: https://github.com/lucasgmagalhaes/oca
Description: Native video editor for gameplay footage
 oca provides timeline editing, local AI-assisted tools, preview and export
 in a self-contained package with its media engines and models included.
EOF

chmod 0755 "$PACKAGE_ROOT/DEBIAN"
chmod 0644 "$PACKAGE_ROOT/DEBIAN/control" "$PACKAGE_ROOT/DEBIAN/md5sums"
dpkg-deb -Zxz -z6 --root-owner-group --build "$PACKAGE_ROOT" "$OUTPUT_DEB"

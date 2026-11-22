#!/usr/bin/env bash
# Builds desktop/target/Aqua-<arch>.AppImage: the desktop app in one file.
#
# Uses appimagetool from $APPIMAGETOOL or the PATH; if there is none it is
# fetched from the AppImage project's GitHub releases. The AppImage uses the
# system's Vulkan (or GL) driver, Wayland or X11 libraries, as any app does.
set -euo pipefail
cd "$(dirname "$0")/../.."
ARCH="${ARCH:-$(uname -m)}"

(cd desktop && cargo build --release)

APPDIR=desktop/target/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/512x512/apps"
cp desktop/target/release/aqua "$APPDIR/usr/bin/"
cp desktop/package/aqua.desktop "$APPDIR/aqua.desktop"
cp desktop/package/aqua.desktop "$APPDIR/usr/share/applications/"
cp desktop/package/aqua.png "$APPDIR/aqua.png"
cp desktop/package/aqua.png "$APPDIR/usr/share/icons/hicolor/512x512/apps/"
cat > "$APPDIR/AppRun" <<'RUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/aqua" "$@"
RUN
chmod +x "$APPDIR/AppRun"

TOOL="${APPIMAGETOOL:-}"
if [ -z "$TOOL" ]; then
  TOOL="$(command -v appimagetool || true)"
fi
if [ -z "$TOOL" ]; then
  TOOL="desktop/target/appimagetool-$ARCH.AppImage"
  if [ ! -x "$TOOL" ]; then
    echo "Fetching appimagetool…"
    curl -fL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOL"
  fi
fi
# Without FUSE (containers, CI) the tool runs from its own extracted copy.
APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" "$TOOL" "$APPDIR" "desktop/target/Aqua-$ARCH.AppImage"
echo "Built desktop/target/Aqua-$ARCH.AppImage"

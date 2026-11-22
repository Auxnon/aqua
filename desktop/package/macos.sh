#!/usr/bin/env bash
# Builds desktop/target/Aqua.app (Apple silicon and Intel in one binary) and
# Aqua.dmg. Run on a Mac with Xcode's command line tools and rustup:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin
# The app is signed ad hoc (needed to run on Apple silicon); for anyone else to
# open it without a warning it needs a Developer ID signature and notarizing.
set -euo pipefail
cd "$(dirname "$0")/../.."

(cd desktop && cargo build --release --target aarch64-apple-darwin && cargo build --release --target x86_64-apple-darwin)

OUT=desktop/target
APP="$OUT/Aqua.app"
rm -rf "$APP" "$OUT/aqua.iconset"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/aqua" \
  "$OUT/aarch64-apple-darwin/release/aqua" "$OUT/x86_64-apple-darwin/release/aqua"

# The icon, from the 512 px PNG.
mkdir "$OUT/aqua.iconset"
for s in 16 32 128 256 512; do
  sips -z $s $s desktop/package/aqua.png --out "$OUT/aqua.iconset/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); [ $d -le 512 ] && sips -z $d $d desktop/package/aqua.png --out "$OUT/aqua.iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$OUT/aqua.iconset" -o "$APP/Contents/Resources/aqua.icns"

VERSION="$(grep -m1 '^version' desktop/Cargo.toml | cut -d'"' -f2)"
sed "s/@VERSION@/$VERSION/g" desktop/package/Info.plist > "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP"

rm -f "$OUT/Aqua.dmg"
hdiutil create -volname Aqua -srcfolder "$APP" -ov -format UDZO "$OUT/Aqua.dmg"
echo "Built $APP and $OUT/Aqua.dmg"

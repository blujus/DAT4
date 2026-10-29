#!/usr/bin/env bash
# Build a native Apple Silicon Mindmap.app (and optionally a .dmg).
# Run on macOS (Xcode command line tools + Rust installed):  scripts/bundle-macos.sh [--dmg]
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET=aarch64-apple-darwin
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="mm-app"))')
rustup target add "$TARGET" >/dev/null
cargo build --release --target "$TARGET" -p mm-app -p mm-cli

APP=dist/Mindmap.app
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "target/$TARGET/release/mindmap" "$APP/Contents/MacOS/Mindmap"
cp "target/$TARGET/release/mm-cli" "$APP/Contents/Resources/mm-cli"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Mindmap</string>
  <key>CFBundleDisplayName</key><string>Mindmap</string>
  <key>CFBundleIdentifier</key><string>app.mindmap.desktop</string>
  <key>CFBundleExecutable</key><string>Mindmap</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>Mindmap document</string>
    <key>CFBundleTypeRole</key><string>Editor</string>
    <key>LSItemContentTypes</key><array><string>public.json</string></array>
  </dict></array>
</dict></plist>
PLIST

# Ad-hoc sign so Apple Silicon will run it locally. Replace "-" with a Developer ID for distribution.
codesign --force --deep --sign "${CODESIGN_IDENTITY:--}" "$APP"
echo "Built $APP"

if [[ "${1:-}" == "--dmg" ]]; then
  rm -f "dist/Mindmap-${VERSION}-arm64.dmg"
  hdiutil create -volname Mindmap -srcfolder "$APP" -ov -format UDZO "dist/Mindmap-${VERSION}-arm64.dmg"
  echo "Built dist/Mindmap-${VERSION}-arm64.dmg"
fi

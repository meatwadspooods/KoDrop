#!/bin/sh
set -e
cd "$(dirname "$0")"
cargo build --release

app=target/release/KoDrop.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/kodrop "$app/Contents/MacOS/kodrop"
# The icon files are optional and not in the repo
for icon in KoDrop.icns KoDrop-icon.png; do
  if [ -f "assets/$icon" ]; then cp "assets/$icon" "$app/Contents/Resources/"; fi
done
cat > "$app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key><string>kodrop</string>
  <key>CFBundleIconFile</key><string>KoDrop</string>
  <key>CFBundleIdentifier</key><string>app.kodrop</string>
  <key>CFBundleName</key><string>KoDrop</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key><string>KoDrop sends books to your Kindle over your local network.</string>
</dict>
</plist>
EOF
# Gives macOS a bundle identity to attach the Local Network permission to
codesign --force --sign - --identifier app.kodrop "$app"
echo "Built $app"

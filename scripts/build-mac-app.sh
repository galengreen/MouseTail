#!/bin/bash
# Build MouseTail.app: the SwiftUI menu bar app with the Rust daemon inside it.
#   scripts/build-mac-app.sh [debug|release|universal]   → dist/MouseTail.app
# `universal` runs natively on Apple Silicon and Intel (what releases ship).
# Signs with the "MouseTail Release" certificate when it's in the keychain (as releases are, so
# macOS keeps MouseTail's permissions across updates), else a Developer ID / Apple Development
# identity, else ad hoc.
set -euo pipefail
cd "$(dirname "$0")/.."
profile=${1:-release}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)

if [[ $profile == universal ]]; then
  # Separate target dirs per architecture: sharing one lets their host-side macro builds
  # overwrite each other.
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target add "$t" >/dev/null 2>&1 || true
    CARGO_TARGET_DIR="target/mac-$t" \
      cargo build --release --locked -p mousetail --target "$t"
  done
  mkdir -p target/universal
  lipo -create -output target/universal/mousetail \
    target/mac-aarch64-apple-darwin/aarch64-apple-darwin/release/mousetail \
    target/mac-x86_64-apple-darwin/x86_64-apple-darwin/release/mousetail
  (cd apps/macos && swift build -c release --arch arm64 --arch x86_64)
  daemon=target/universal/mousetail
  bin=$(cd apps/macos && swift build -c release --arch arm64 --arch x86_64 --show-bin-path)
elif [[ $profile == release ]]; then
  cargo build --release -p mousetail
  (cd apps/macos && swift build -c release)
  daemon=target/release/mousetail
  bin=$(cd apps/macos && swift build -c release --show-bin-path)
else
  cargo build -p mousetail
  (cd apps/macos && swift build)
  daemon=target/debug/mousetail
  bin=$(cd apps/macos && swift build --show-bin-path)
fi

app=dist/MouseTail.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp "$bin/MouseTail" "$app/Contents/MacOS/MouseTail"
ditto "$bin/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"
# Lower-case "mousetail" would clash with "MouseTail" on a case-insensitive disk.
cp "$daemon" "$app/Contents/MacOS/mousetaild"
cp apps/macos/Resources/AppIcon.icns apps/macos/Resources/MenuBarIcon*.png apps/macos/Resources/LogoMouse.svg \
  "$app/Contents/Resources/"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>MouseTail</string>
  <key>CFBundleDisplayName</key><string>MouseTail</string>
  <key>CFBundleIdentifier</key><string>nz.galengreen.MouseTail</string>
  <key>CFBundleExecutable</key><string>MouseTail</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHumanReadableCopyright</key><string>© Galen Green</string>
  <key>NSLocalNetworkUsageDescription</key>
  <string>MouseTail finds and connects to your other computers on the local network.</string>
  <key>NSAudioCaptureUsageDescription</key>
  <string>MouseTail sends your Mac's sound to the computer you're using.</string>
  <key>SUFeedURL</key>
  <string>https://github.com/galengreen/MouseTail/releases/latest/download/appcast.xml</string>
  <key>SUPublicEDKey</key><string>$(sed -n 's/^pub const PUBLIC_KEY: &str = "\(.*\)";/\1/p' crates/core/src/update.rs)</string>
  <key>SUEnableAutomaticChecks</key><true/>
  <key>SUAutomaticallyUpdate</key><true/>
  <key>SUScheduledCheckInterval</key><integer>21600</integer>
  <key>SUVerifyUpdateBeforeExtraction</key><true/>
  <key>NSBonjourServices</key>
  <array><string>_mousetail._udp</string></array>
</dict>
</plist>
PLIST

if security find-identity -p codesigning | grep -q '"MouseTail Release"'; then
  ident="MouseTail Release"
else
  ident=$(security find-identity -v -p codesigning | awk '/Developer ID Application|Apple Development/ {print $2; exit}')
  ident=${ident:--}
fi
codesign --force --deep --sign "$ident" "$app/Contents/Frameworks/Sparkle.framework"
codesign --force --sign "$ident" --identifier nz.galengreen.MouseTail.daemon "$app/Contents/MacOS/mousetaild"
codesign --force --sign "$ident" "$app"
echo "built $app ($profile, signed with ${ident:0:8}…)"

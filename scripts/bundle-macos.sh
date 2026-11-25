#!/bin/sh
# Builds Maestro.app for macOS, for both Apple Silicon and Intel, and zips it.
# Run on a Mac from the repository root: sh scripts/bundle-macos.sh
# The result is dist/maestro-<version>-macos-universal.zip.
#
# The app is signed "ad hoc": enough for Apple Silicon to run it, not enough
# for Gatekeeper to open it without the user allowing it. Proper signing takes
# an Apple developer identity. See docs/releasing.md.
set -eu

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app=dist/Maestro.app
zip=dist/maestro-$version-macos-universal.zip

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

rm -rf "$app" "$zip" dist/icon.iconset
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" dist/icon.iconset

# One program that runs on both kinds of Mac.
lipo -create -output "$app/Contents/MacOS/maestro" \
    target/aarch64-apple-darwin/release/maestro \
    target/x86_64-apple-darwin/release/maestro

# The icon, in the sizes macOS asks for.
for size in 16 32 128 256 512; do
    double=$((size * 2))
    sips -z "$size" "$size" assets/icon.png --out "dist/icon.iconset/icon_${size}x${size}.png" >/dev/null
    sips -z "$double" "$double" assets/icon.png --out "dist/icon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns dist/icon.iconset -o "$app/Contents/Resources/Maestro.icns"
rm -rf dist/icon.iconset

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Maestro</string>
    <key>CFBundleDisplayName</key><string>Maestro</string>
    <key>CFBundleIdentifier</key><string>io.github.pryme8.maestro</string>
    <key>CFBundleExecutable</key><string>maestro</string>
    <key>CFBundleIconFile</key><string>Maestro</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSAppleEventsUsageDescription</key><string>Maestro asks Terminal to run an agent tool's own sign-in command, and asks for your password to switch a network adapter.</string>
</dict>
</plist>
EOF

codesign --force --deep --sign - "$app"
ditto -c -k --keepParent "$app" "$zip"
echo "made $zip"

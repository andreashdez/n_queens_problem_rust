#!/usr/bin/env bash
# Packages the desktop GUI as a universal macOS app and zips it for a release.
#
# Needs macOS with Xcode 26 or later, whose `actool` compiles the Icon Composer icon.
# Writes `target/macos/N-Queens Solver.app` and prints the path of the zip next to it.
set -euo pipefail

cd "$(dirname "$0")/.."

app_name="N-Queens Solver"
bundle_id="io.github.andreashdez.n-queens-solver"
binary="n_queens_gui"
icon="assets/app-icon.icon"
minimum_macos="11.0"
targets=(aarch64-apple-darwin x86_64-apple-darwin)

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
out="target/macos"
app="$out/$app_name.app"
zip="$out/N-Queens-Solver-$version-macos.zip"

rustup target add "${targets[@]}" >&2
binaries=()
for target in "${targets[@]}"; do
    MACOSX_DEPLOYMENT_TARGET="$minimum_macos" cargo build --release --locked \
        --features gui --bin "$binary" --target "$target" >&2
    binaries+=("target/$target/release/$binary")
done

rm -rf "$app" "$zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/$binary" "${binaries[@]}"

# Writes `Assets.car` for macOS 26 and later, and `app-icon.icns` for earlier versions.
xcrun actool "$icon" --compile "$app/Contents/Resources" \
    --platform macosx --minimum-deployment-target "$minimum_macos" \
    --app-icon app-icon --output-partial-info-plist "$out/icon-info.plist" \
    --errors --warnings >&2
rm "$out/icon-info.plist"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>$app_name</string>
	<key>CFBundleExecutable</key>
	<string>$binary</string>
	<key>CFBundleIconFile</key>
	<string>app-icon</string>
	<key>CFBundleIconName</key>
	<string>app-icon</string>
	<key>CFBundleIdentifier</key>
	<string>$bundle_id</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>$app_name</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$version</string>
	<key>CFBundleVersion</key>
	<string>$version</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.education</string>
	<key>LSMinimumSystemVersion</key>
	<string>$minimum_macos</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
EOF
plutil -lint "$app/Contents/Info.plist" >&2

# Apple Silicon only runs signed code. An ad-hoc signature satisfies that, but
# Gatekeeper still asks users to approve the app because it is not notarized.
codesign --force --sign - "$app" >&2
codesign --verify --strict "$app" >&2

ditto -c -k --keepParent "$app" "$zip"
echo "$zip"

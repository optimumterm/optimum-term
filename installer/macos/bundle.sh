#!/bin/sh
set -eu

version="$1"
binary="$2"
out="$3"

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$out/Optimum.app"

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

cp "$binary" "$app/Contents/MacOS/optimum"
sed "s/VERSION/$version/g" "$root/installer/macos/Info.plist" > "$app/Contents/Info.plist"
cp "$root/assets/optimum.icns" "$app/Contents/Resources/optimum.icns"

icon="$(mktemp -d)"
xcrun actool "$root/assets/Optimum.icon" \
	--compile "$icon" \
	--output-partial-info-plist "$icon/partial.plist" \
	--app-icon Optimum \
	--include-all-app-icons \
	--enable-on-demand-resources NO \
	--development-region en \
	--target-device mac \
	--platform macosx \
	--minimum-deployment-target 13.0 \
	--errors --warnings --output-format human-readable-text
cp "$icon/Assets.car" "$app/Contents/Resources/Assets.car"
rm -rf "$icon"

cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/"
codesign --force --sign - "$app"

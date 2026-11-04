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

cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/"
codesign --force --sign - "$app"

#!/bin/sh
set -eu

version="$1"
binary="$2"
out="$3"

root="$(cd "$(dirname "$0")/../.." && pwd)"
app="$out/Optimum.app"
iconset="$out/optimum.iconset"

rm -rf "$app" "$iconset"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$iconset"

cp "$binary" "$app/Contents/MacOS/optimum"
sed "s/VERSION/$version/g" "$root/installer/macos/Info.plist" > "$app/Contents/Info.plist"

source="$root/assets/png/optimum-256.png"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$source" --out "$iconset/icon_${size}x${size}.png" > /dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$source" --out "$iconset/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/optimum.icns"
rm -rf "$iconset"

cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$app/Contents/Resources/"
codesign --force --sign - "$app"

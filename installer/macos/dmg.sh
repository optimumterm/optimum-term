#!/bin/sh
set -eu

app="$1"
dmg="$2"

root="$(cd "$(dirname "$0")/../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir "$work/stage" "$work/mnt"
cp -R "$app" "$work/stage/"
ln -s /Applications "$work/stage/Applications"
cp "$root/assets/optimum.icns" "$work/stage/.VolumeIcon.icns"

hdiutil create -quiet -volname Optimum -srcfolder "$work/stage" -fs HFS+ -format UDRW "$work/rw.dmg"
hdiutil attach -quiet -readwrite -noverify -noautoopen -nobrowse -mountpoint "$work/mnt" "$work/rw.dmg"
SetFile -a C "$work/mnt"
hdiutil detach -quiet "$work/mnt"

rm -f "$dmg"
hdiutil convert -quiet "$work/rw.dmg" -format UDZO -imagekey zlib-level=9 -o "$dmg"

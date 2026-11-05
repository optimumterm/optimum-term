#!/bin/sh
set -eu

version="1.25.260930003"
package_sha256="02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee"
dll_sha256="feeef341d891643c62d30b6b07800bc70f0bc148f44cb8c3bee84aa557ae805a"
exe_sha256="3d66b23d0a71bb8eed2b77edc8b9df9bf54ce6c8fb74c863a30e760997f80586"

root="$(cd "$(dirname "$0")/.." && pwd)"
dest="$root/vendor/conpty/x64"
url="https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/$version/microsoft.windows.console.conpty.$version.nupkg"

sha256() {
  if command -v sha256sum > /dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1
}

check() {
  actual="$(sha256 "$1")"
  if [ "$actual" != "$2" ]; then
    echo "sha256 mismatch for $1: $actual" >&2
    exit 1
  fi
}

if [ -f "$dest/conpty.dll" ] && [ -f "$dest/OpenConsole.exe" ] \
  && [ "$(sha256 "$dest/conpty.dll")" = "$dll_sha256" ] \
  && [ "$(sha256 "$dest/OpenConsole.exe")" = "$exe_sha256" ]; then
  exit 0
fi

python="$(command -v python3 || command -v python)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL -o "$work/conpty.nupkg" "$url"
check "$work/conpty.nupkg" "$package_sha256"

mkdir -p "$dest"
"$python" -I -c '
import sys, zipfile
package, dest = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(package) as z:
    for member, name in (("runtimes/win-x64/native/conpty.dll", "conpty.dll"),
                         ("build/native/runtimes/x64/OpenConsole.exe", "OpenConsole.exe")):
        with open(f"{dest}/{name}", "wb") as out:
            out.write(z.read(member))
' "$work/conpty.nupkg" "$dest"

check "$dest/conpty.dll" "$dll_sha256"
check "$dest/OpenConsole.exe" "$exe_sha256"

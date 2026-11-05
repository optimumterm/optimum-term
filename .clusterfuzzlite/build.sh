#!/bin/bash -eu
cd "$SRC/optimum-term"
[ -f vendor/ghostty/build.zig ] || git submodule update --init --depth 1 vendor/ghostty
cargo fuzz build -O
for target in fuzz/fuzz_targets/*.rs; do
  name="$(basename "${target%.rs}")"
  cp "fuzz/target/x86_64-unknown-linux-gnu/release/$name" "$OUT/"
done

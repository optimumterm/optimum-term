# Building

Everything is built in WSL and cross-compiled to Windows (ADR-007). Nothing needs to be installed on Windows.

## Prerequisites (WSL, one time)

- Rust (stable) with the Windows target:
  ```sh
  rustup target add x86_64-pc-windows-gnu
  cargo install --locked cargo-zigbuild
  ```
- Zig `0.17.0` on `PATH` (or set `ZIG`), see ADR-003.
- libclang for `bindgen`: set `LIBCLANG_PATH` to the directory containing `libclang.so`.
- Submodules: `git submodule update --init`.

### NixOS

libclang and the C headers bindgen needs for the Linux host target:

```sh
export LIBCLANG_PATH=$(nix-build '<nixpkgs>' -A llvmPackages.libclang.lib --no-out-link)/lib
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_unknown_linux_gnu="-isystem $(echo $LIBCLANG_PATH/clang/*/include) -isystem $(nix-build '<nixpkgs>' -A glibc.dev --no-out-link)/include"
```

## Windows build (the product)

```sh
cargo-zigbuild test   --target x86_64-pc-windows-gnu
cargo-zigbuild clippy --target x86_64-pc-windows-gnu --all-targets -- -D warnings
cargo zigbuild        --target x86_64-pc-windows-gnu --release
```

Tests run as Windows executables through WSL interop. Tests that need a WSL distribution are `#[ignore]`d; run them locally with `cargo-zigbuild test --target x86_64-pc-windows-gnu --workspace -- --include-ignored`.

Output: `target/x86_64-pc-windows-gnu/<profile>/optimum.exe`, with `conpty.dll` and `OpenConsole.exe` copied next to it. Copy all three together.

The first build compiles libghostty-vt with Zig and takes a few minutes; later builds reuse the cache.

## Linux host build

`cargo test` builds and runs the platform-agnostic crates as a Linux binary. Useful for quick checks; not the product.

## macOS (from WSL)

Apple Silicon only (ADR-028). WSL can check and lint the macOS code without an Apple SDK, because Zig cross-builds libghostty-vt:

```sh
rustup target add aarch64-apple-darwin
cargo-zigbuild clippy --target aarch64-apple-darwin --workspace --all-targets -- -D warnings
```

Linking, tests and the app bundle need macOS, so they run in CI on `macos-15`. Every push to `main` uploads `Optimum.dmg` as a workflow artifact (ADR-030). On a Mac, open it, drag Optimum to Applications, run `xattr -dr com.apple.quarantine Optimum.app` (the build is not notarized), then open it. On a Mac with Rust and Zig, `cargo build --release -p optimum` followed by `installer/macos/bundle.sh <version> target/release/optimum dist` produces the same bundle, and `installer/macos/dmg.sh dist/Optimum.app dist/Optimum.dmg` the disk image.

## Packaging (M8)

A release holds two files without a version in their names (ADR-030): `Optimum.exe` (the Windows installer) and `Optimum.dmg`, each with a `.sha256`.

- **CI:** every push to `main` that changes more than Markdown or `docs/` runs `.github/workflows/ci.yml` (clippy, tests, build). When it passes, `.github/workflows/release.yml` starts on the same commit (ADR-030); it doesn't test again. A first job picks the next version: major and minor from `version` in the root `Cargo.toml`, patch one above the highest `v<major>.<minor>.*` tag. It creates a draft release for that tag; the build jobs write the version into `Cargo.toml` before building and upload their files to the draft. The `windows-latest` job builds the release exe, builds the installer and its checksum. The `macos-15` job builds `Optimum.app`, packs it into `Optimum.dmg` with a checksum. A last job publishes the draft as the latest release (creating the tag) and deletes every other release, so only the latest one exists; git tags are kept. Runs are queued, never parallel.
- **Locally from WSL:** build with `cargo-zigbuild build --release --target x86_64-pc-windows-gnu -p optimum`, then run the installer step below.
- **Version:** no manual tagging. To start a new minor or major line, change `version` in the root `Cargo.toml` (e.g. `0.2.0`); the patch number is ignored there. Regenerate the Rust crate list in `THIRD_PARTY_NOTICES.md` if dependencies changed (ADR-022).
- **Installer:** `installer/optimum.iss` is an [Inno Setup](https://jrsoftware.org/isinfo.php) 6 script (ADR-025). After the release build, compile it on Windows:
  ```sh
  ISCC.exe /DAppVersion=<version> installer\optimum.iss
  ```
  It reads the files from `target\x86_64-pc-windows-gnu\release` (override with `/DBinDir=…`) and writes `target\installer\Optimum.exe`. From WSL, run `ISCC.exe` through interop with `$(wslpath -w installer/optimum.iss)`. No install needed: Inno's own installer accepts `/VERYSILENT /CURRENTUSER /PORTABLE=1 /DIR=<folder>`. The release workflow downloads `innosetup-6.7.3.exe` from the jrsoftware/issrc GitHub release, checks its sha256 and installs it the same way. The wizard images `installer/wizard-*.png` are generated from `assets/optimum-win.svg`, one per scale at Inno's exact image-area size (ADR-025).
- **Not done yet:** MSIX and code signing.

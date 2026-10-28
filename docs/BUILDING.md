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

## Packaging (M8)

A release is a zip, `optimum-<version>-windows-x64.zip`, holding one folder with `optimum.exe`, `conpty.dll`, `OpenConsole.exe`, `LICENSE`, `THIRD_PARTY_NOTICES.md` and `README.md`. A `.sha256` file goes next to it.

- **CI:** pushing a tag `v<version>` runs `.github/workflows/release.yml` on `windows-latest`. It tests, builds the release exe, packages the zip and checksum, and attaches both to a GitHub Release.
- **Locally from WSL:** build with `cargo-zigbuild build --release --target x86_64-pc-windows-gnu -p optimum`, copy those six files into a folder, and zip it with PowerShell's `Compress-Archive`.
- **Before tagging:** bump `version` in the root `Cargo.toml`, and regenerate the Rust crate list in `THIRD_PARTY_NOTICES.md` if dependencies changed (ADR-022).
- **Installer:** `installer/optimum.iss` is an [Inno Setup](https://jrsoftware.org/isinfo.php) 6 script (ADR-025). After the release build, compile it on Windows:
  ```sh
  ISCC.exe /DAppVersion=<version> installer\optimum.iss
  ```
  It reads the files from `target\x86_64-pc-windows-gnu\release` (override with `/DBinDir=…`) and writes `target\installer\optimum-setup-<version>.exe`. From WSL, run `ISCC.exe` through interop with `$(wslpath -w installer/optimum.iss)`. No install needed: Inno's own installer accepts `/VERYSILENT /CURRENTUSER /PORTABLE=1 /DIR=<folder>`. CI installs it with `choco install innosetup --version 6.7.3`. The wizard images `installer/wizard-*.bmp` are generated from `assets/png/optimum-256.png`.
- **Not done yet:** MSIX and code signing.

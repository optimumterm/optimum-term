use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ghostty = manifest_dir.join("../../vendor/ghostty");
    let ghostty = ghostty.canonicalize().unwrap_or(ghostty);

    if !ghostty.join("build.zig").exists() {
        panic!(
            "Ghostty source not found at {}. Run `git submodule update --init`.",
            ghostty.display()
        );
    }

    for path in ["build.zig", "build.zig.zon", "include", "src", "pkg"] {
        println!("cargo:rerun-if-changed={}", ghostty.join(path).display());
    }
    println!("cargo:rerun-if-env-changed=ZIG");

    let prefix = out_dir.join("ghostty");
    build_ghostty(&ghostty, &prefix, &out_dir.join("zig-cache"));
    link(&prefix.join("lib"));
    generate_bindings(&ghostty.join("include"), &out_dir.join("bindings.rs"));
}

fn build_ghostty(source: &Path, prefix: &Path, cache: &Path) {
    let zig = env::var("ZIG").unwrap_or_else(|_| "zig".into());
    let optimize = match env::var("PROFILE").as_deref() {
        Ok("release") => "ReleaseFast",
        _ => "ReleaseSafe",
    };

    let mut cmd = Command::new(&zig);
    cmd.current_dir(source)
        .arg("build")
        .arg("-Demit-lib-vt=true")
        .arg(format!("-Doptimize={optimize}"))
        .arg("--prefix")
        .arg(prefix)
        .arg("--cache-dir")
        .arg(cache);

    if env::var("TARGET").unwrap() != env::var("HOST").unwrap() {
        cmd.arg(format!("-Dtarget={}", zig_target()));
    }

    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("failed to run `{zig}`: {e}. Install Zig 0.17.0 or set ZIG."));
    if !status.success() {
        panic!("`zig build` for libghostty-vt failed with {status}");
    }
}

fn zig_target() -> String {
    let target = env::var("TARGET").unwrap();
    match target.as_str() {
        "x86_64-pc-windows-msvc" => "x86_64-windows-msvc".into(),
        "aarch64-pc-windows-msvc" => "aarch64-windows-msvc".into(),
        "x86_64-pc-windows-gnu" => "x86_64-windows-gnu".into(),
        "x86_64-apple-darwin" => "x86_64-macos".into(),
        "aarch64-apple-darwin" => "aarch64-macos".into(),
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu".into(),
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu".into(),
        other => panic!("unsupported target for libghostty-vt: {other}"),
    }
}

fn link(lib_dir: &Path) {
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        println!("cargo:rustc-link-lib=static:+verbatim=ghostty-vt-static.lib");
        println!("cargo:rustc-link-lib=dylib=ntdll");
        println!("cargo:rustc-link-lib=dylib=kernel32");
    } else {
        println!("cargo:rustc-link-lib=static=ghostty-vt");
    }
}

fn generate_bindings(include: &Path, out: &Path) {
    bindgen::Builder::default()
        .header(include.join("ghostty/vt.h").to_string_lossy())
        .clang_arg(format!("-I{}", include.display()))
        .clang_arg("-DGHOSTTY_STATIC")
        .allowlist_function("ghostty_.*")
        .allowlist_type("Ghostty.*")
        .allowlist_var("GHOSTTY_.*")
        .use_core()
        .prepend_enum_name(false)
        .derive_default(true)
        .generate_comments(false)
        .layout_tests(false)
        .rust_edition(bindgen::RustEdition::Edition2024)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed on ghostty/vt.h")
        .write_to_file(out)
        .expect("failed to write bindings");
}

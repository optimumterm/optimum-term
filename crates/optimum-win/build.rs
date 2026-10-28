use std::env;
use std::fs;
use std::path::PathBuf;

const CONPTY_FILES: [&str; 2] = ["conpty.dll", "OpenConsole.exe"];

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").unwrap() != "windows" {
        return;
    }
    let arch = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => "x64",
        other => panic!("no bundled ConPTY for {other}"),
    };

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest_dir.join("../../vendor/conpty").join(arch);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("unexpected OUT_DIR layout")
        .to_path_buf();

    for name in CONPTY_FILES {
        let from = source.join(name);
        println!("cargo:rerun-if-changed={}", from.display());
        for dir in [profile_dir.clone(), profile_dir.join("deps")] {
            fs::create_dir_all(&dir).unwrap();
            fs::copy(&from, dir.join(name))
                .unwrap_or_else(|e| panic!("copy {} to {}: {e}", from.display(), dir.display()));
        }
    }
}

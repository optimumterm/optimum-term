use std::env;
use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    optimum_icon::embed(&root.join("../../assets/optimum.ico"));
}

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const ICON_ID: u16 = 1;
const MOVEABLE_PURE_DISCARDABLE: u16 = 0x1030;
const LANG_NEUTRAL: u16 = 0;

pub fn embed(icon: &Path) {
    println!("cargo:rerun-if-changed={}", icon.display());
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let ico = fs::read(icon).expect("icon file");
    let res = PathBuf::from(env::var("OUT_DIR").unwrap()).join("icon.res");
    fs::write(&res, resources(&ico)).unwrap();
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

fn resources(ico: &[u8]) -> Vec<u8> {
    let u16_at = |at: usize| u16::from_le_bytes([ico[at], ico[at + 1]]);
    let u32_at = |at: usize| u32::from_le_bytes(ico[at..at + 4].try_into().unwrap());
    assert_eq!((u16_at(0), u16_at(2)), (0, 1), "not an .ico file");
    let count = u16_at(4);

    let mut out = Vec::new();
    entry(&mut out, 0, 0, &[]);
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&count.to_le_bytes());
    for i in 0..count {
        let dir = 6 + 16 * usize::from(i);
        let size = u32_at(dir + 8) as usize;
        let offset = u32_at(dir + 12) as usize;
        let id = i + 1;
        entry(&mut out, RT_ICON, id, &ico[offset..offset + size]);
        group.extend_from_slice(&ico[dir..dir + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    entry(&mut out, RT_GROUP_ICON, ICON_ID, &group);
    out
}

fn entry(out: &mut Vec<u8>, kind: u16, name: u16, data: &[u8]) {
    let flags = if kind == 0 {
        0
    } else {
        MOVEABLE_PURE_DISCARDABLE
    };
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes());
    for value in [0xffff, kind, 0xffff, name] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&LANG_NEUTRAL.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(data);
    out.resize(out.len().next_multiple_of(4), 0);
}

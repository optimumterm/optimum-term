use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

const FORWARDED: [&str; 4] = ["TERM", "COLORTERM", "TERM_PROGRAM", "TERM_PROGRAM_VERSION"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WslStatus {
    Ready,
    NotInstalled,
    NoDistribution,
}

impl WslStatus {
    pub fn detect() -> WslStatus {
        let exe = exe();
        if !exe.is_file() {
            return WslStatus::NotInstalled;
        }
        let output = Command::new(&exe)
            .args(["--list", "--quiet"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW.0)
            .output();
        match output {
            Err(_) => WslStatus::NotInstalled,
            Ok(out) if !out.status.success() => WslStatus::NoDistribution,
            Ok(out) if decode_utf16le(&out.stdout).trim().is_empty() => WslStatus::NoDistribution,
            Ok(_) => WslStatus::Ready,
        }
    }

    pub fn message(self) -> Option<&'static str> {
        match self {
            WslStatus::Ready => None,
            WslStatus::NotInstalled => Some(
                "WSL is not installed. Run `wsl --install` in PowerShell (as administrator), then restart Optimum.",
            ),
            WslStatus::NoDistribution => Some(
                "WSL has no Linux distribution. Run `wsl --install` in PowerShell (as administrator), then restart Optimum.",
            ),
        }
    }
}

pub(crate) fn exe() -> PathBuf {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` is a valid writable buffer; the return value is the number of characters written.
    let len = unsafe { GetSystemDirectoryW(Some(&mut buf)) } as usize;
    let system = if len == 0 || len > buf.len() {
        PathBuf::from(env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
            .join("System32")
    } else {
        PathBuf::from(OsString::from_wide(&buf[..len]))
    };
    system.join("wsl.exe")
}

pub(crate) fn command_line() -> String {
    format!("\"{}\" --cd ~", exe().display())
}

pub(crate) fn environment() -> Vec<u16> {
    environment_from(env::vars_os())
}

fn environment_from(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Vec<u16> {
    let mut map: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    for (key, value) in vars {
        map.insert(key.to_string_lossy().to_uppercase(), (key, value));
    }

    let wslenv = merge_wslenv(map.get("WSLENV").map(|(_, v)| v.as_os_str()));
    let overrides = [
        ("TERM", "xterm-256color".to_owned()),
        ("COLORTERM", "truecolor".to_owned()),
        ("TERM_PROGRAM", "Optimum".to_owned()),
        ("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION").to_owned()),
        ("WSLENV", wslenv),
    ];
    for (key, value) in overrides {
        map.insert(key.to_owned(), (key.into(), value.into()));
    }

    let mut block = Vec::new();
    for (key, value) in map.values() {
        block.extend(key.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

fn merge_wslenv(existing: Option<&OsStr>) -> String {
    let existing = existing
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut entries: Vec<String> = existing
        .split(':')
        .filter(|entry| {
            let name = entry.split('/').next().unwrap_or_default();
            !entry.is_empty() && !FORWARDED.iter().any(|f| f.eq_ignore_ascii_case(name))
        })
        .map(str::to_owned)
        .collect();
    entries.extend(FORWARDED.iter().map(|name| format!("{name}/u")));
    entries.join(":")
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .collect();
    String::from_utf16_lossy(&units).replace('\0', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_block(block: &[u16]) -> Vec<String> {
        String::from_utf16(block)
            .unwrap()
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn wslenv_forwards_terminal_variables() {
        assert_eq!(
            merge_wslenv(None),
            "TERM/u:COLORTERM/u:TERM_PROGRAM/u:TERM_PROGRAM_VERSION/u"
        );
    }

    #[test]
    fn wslenv_keeps_user_entries_and_drops_duplicates() {
        assert_eq!(
            merge_wslenv(Some(OsStr::new("USERPROFILE/p:TERM/w:"))),
            "USERPROFILE/p:TERM/u:COLORTERM/u:TERM_PROGRAM/u:TERM_PROGRAM_VERSION/u"
        );
    }

    #[test]
    fn environment_block_is_sorted_overridden_and_double_nul_terminated() {
        let block = environment_from([
            ("Path".into(), "C:\\bin".into()),
            ("TERM".into(), "dumb".into()),
            ("a".into(), "1".into()),
        ]);
        assert_eq!(&block[block.len() - 2..], &[0, 0]);
        let vars = decode_block(&block);
        assert_eq!(vars[0], "a=1");
        assert!(vars.contains(&"TERM=xterm-256color".to_owned()));
        assert!(vars.contains(&"COLORTERM=truecolor".to_owned()));
        assert!(vars.contains(&"TERM_PROGRAM=Optimum".to_owned()));
        let mut sorted = vars.clone();
        sorted.sort_by_key(|v| v.to_uppercase());
        assert_eq!(vars, sorted);
    }

    #[test]
    fn utf16le_output_is_decoded() {
        let bytes: Vec<u8> = "Ubuntu\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(decode_utf16le(&bytes).trim(), "Ubuntu");
    }

    #[test]
    fn wsl_exe_lives_in_system32() {
        assert!(exe().ends_with("System32\\wsl.exe") || exe().ends_with("system32\\wsl.exe"));
    }

    #[test]
    #[ignore = "needs a WSL distribution"]
    fn wsl_is_ready() {
        assert_eq!(WslStatus::detect(), WslStatus::Ready);
    }
}

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    optimum_win::run()
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    optimum_mac::run()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn main() -> ExitCode {
    eprintln!("Optimum runs on Windows and macOS only.");
    ExitCode::FAILURE
}

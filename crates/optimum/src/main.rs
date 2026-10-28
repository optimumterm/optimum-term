#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    optimum_win::run()
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("Optimum currently runs on Windows only.");
    ExitCode::FAILURE
}

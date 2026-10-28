#![cfg(unix)]

#[cfg(target_os = "macos")]
mod app;
#[cfg(target_os = "macos")]
mod font;
#[cfg(target_os = "macos")]
mod instance;
#[cfg(any(target_os = "macos", test))]
mod keyboard;
mod pty;
#[cfg(target_os = "macos")]
mod renderer;
#[cfg(target_os = "macos")]
mod settings;
mod shell;
#[cfg(target_os = "macos")]
mod view;

#[cfg(target_os = "macos")]
pub use app::run;
pub use pty::UnixPty;
pub use shell::{LoginShell, login_shell};

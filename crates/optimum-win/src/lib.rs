#![cfg(windows)]

mod app;
mod clipboard;
mod conpty;
mod font;
mod ime;
mod instance;
mod keyboard;
mod pty;
mod renderer;
mod settings;
#[cfg(debug_assertions)]
mod tuner;
mod window;
mod wsl;

pub use app::run;
pub use conpty::ConptySource;
pub use pty::WinPty;
pub use wsl::WslStatus;

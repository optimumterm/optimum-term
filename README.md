# Optimum

A minimal terminal for Windows, made mainly for WSL.

One window, no tabs, no splits, no title bar. Text is rendered with DirectWrite and aims to look as sharp as possible. The terminal core is [libghostty-vt](https://github.com/ghostty-org/ghostty). macOS and Linux versions are planned.

The name comes from the Wooting Optimum keyboard.

## What it does and doesn't do

- Opens your default WSL distro in your home directory. That's the only thing it runs.
- One window per user. Starting it again brings the existing window to the front.
- No shortcuts except `Ctrl+,`, which opens the settings. Every other key goes to the shell.
- Settings only cover how it looks: font, colours, cursor, padding, opacity.
- No tabs or splits. Use tmux or zellij for that.
- No network access, telemetry, accounts or auto-update.

## FAQ

**Why no tabs?**
In the era of multiplexers, tabs have become a vestige. If you're a multiplexer power user, you'll like this terminal: one clean window, and tmux/zellij does the rest.

**Why "Optimum"?**
It's inspired by the Wooting Optimum keyboard: maximum minimalism and elegance.

**Why are there no shortcuts?**
Every key belongs to your shell and your multiplexer. Optimum takes exactly one, `Ctrl+,`, to open the settings.

## Requirements

- Windows 10 1903 or newer, or Windows 11
- WSL with at least one distro (`wsl --install`). To change which distro opens, use `wsl --set-default <name>`.

If WSL isn't installed, the window tells you how to install it.

## Install

Download from the [releases page](../../releases):

- **`optimum-setup-<version>.exe`**: a regular setup wizard. By default it installs for the current user into `%LOCALAPPDATA%\Programs\Optimum` and doesn't need admin rights. You can uninstall it from Settings → Apps. Your settings are kept.
- **`optimum-<version>-windows-x64.zip`**: the portable version. Unzip it anywhere and run `optimum.exe`. Keep `conpty.dll` and `OpenConsole.exe` next to it.

The builds aren't code-signed yet, so SmartScreen may warn you. Click "More info", then "Run anyway".

## Usage

| Action | How |
|---|---|
| Copy | Select text with the mouse. It's copied when you release the button. Double-click selects a word, triple-click a line. |
| Paste | Right-click |
| Select in tmux/vim with mouse mode on | Hold Shift |
| Scroll back | Mouse wheel |
| Move the window | Drag the top edge |
| Close | `Alt+F4`, the taskbar, or `exit` in the shell |
| Settings | `Ctrl+,` |

Programs inside WSL can copy to the Windows clipboard with OSC 52, so copying in tmux or nvim works. They can't read the clipboard.

## Settings

`Ctrl+,` opens `%APPDATA%\Optimum\config.toml`. It's created on first use with all options and their defaults. Changes apply on save. If the file has a mistake, the last working settings stay active and the error is shown at the bottom of the window.

```toml
[font]
family = "JetBrainsMono Nerd Font"   # built in; any installed font works
size = 12.0
line_height = 1.0

[text]
antialiasing = "system"   # "system", "cleartype" or "grayscale"
gamma = "system"          # or 1.0 to 2.2
contrast = "system"       # or 0.0 to 10.0

[colors]
foreground = "#d4d4d4"
background = "#121212"
cursor = "foreground"     # or a colour
selection = "#264f78"
palette = [ ... ]         # 16 ANSI colours

[cursor]
style = "block"           # "block", "bar" or "underline"
blink = false

[window]
padding = 12              # pixels at 100% scaling
opacity = 1.0             # 0.1 to 1.0; text stays opaque
blur = false              # blur behind the window when opacity < 1
```

JetBrains Mono Nerd Font is bundled, so prompt and file icons work out of the box. With `opacity` below 1, text uses grayscale antialiasing, because ClearType needs an opaque background.

## tmux

Optimum sets `TERM=xterm-256color` and `COLORTERM=truecolor`. It doesn't install anything into your distro. For true colour, clipboard, synchronized output and extended keys in tmux, add this to `~/.tmux.conf`:

```
set -g default-terminal "tmux-256color"
set -as terminal-features ",xterm-256color:RGB:sync:clipboard:extkeys:usstyle"
set -g set-clipboard on
set -g extended-keys on
```

## Building

The project is developed in WSL and cross-compiled for Windows with Rust, Zig and `cargo-zigbuild`. See [docs/BUILDING.md](docs/BUILDING.md). [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) explains how the code is organised, and [docs/DECISIONS.md](docs/DECISIONS.md) explains why things are the way they are.

## License

[0BSD](LICENSE). Do whatever you want with it. Bundled third-party parts (libghostty-vt, ConPTY, JetBrains Mono Nerd Font, Rust crates) keep their own licenses. See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

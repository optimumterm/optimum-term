# Optimum term

A minimal terminal for Windows, made mainly for WSL.

One window, no tabs, no splits, no title bar. Text is rendered with DirectWrite and aims to look as sharp as possible. The terminal core is [libghostty-vt](https://github.com/ghostty-org/ghostty). An early macOS version exists (see [macOS](#macos)); Linux is planned.

The name comes from the Wooting Optimum keyboard.

## What it does and doesn't do

- Opens your default WSL distro in your home directory. That's the only thing it runs.
- One window per user. Starting it again brings the existing window to the front.
- Standard Windows shortcuts work: copy, paste, select all, Shift+PageUp and so on. The terminal adds only one of its own, `Ctrl+,` for settings. Everything else goes to the shell.
- Settings only cover how it looks: font, colours, cursor, padding, opacity.
- No tabs or splits. Use tmux or zellij for that.
- No network access, telemetry, accounts or auto-update.

## FAQ

**Why no tabs?**
In the era of multiplexers, tabs have become a vestige. If you're a multiplexer power user, you'll like this terminal: one clean window, and tmux/zellij does the rest.

**Why "Optimum"?**
It's inspired by the Wooting Optimum keyboard: maximum minimalism and elegance.

**Why so few shortcuts?**
Optimum has no features of its own that need keys. It supports the usual shortcuts (copy, paste, select all, scrolling, font size, full screen) and adds only `Ctrl+,` for the settings. Every other key belongs to your shell and your multiplexer.

## Requirements

- Windows 10 1903 or newer, or Windows 11
- WSL with at least one distro (`wsl --install`). To change which distro opens, use `wsl --set-default <name>`.

If WSL isn't installed, the window tells you how to install it.

## Install

Download from the [latest release](../../releases/latest):

- **Windows:** [`Optimum.exe`](../../releases/latest/download/Optimum.exe), a regular setup wizard. By default it installs for the current user into `%LOCALAPPDATA%\Programs\Optimum` and doesn't need admin rights. You can uninstall it from Settings → Apps. Your settings are kept.
- **macOS:** [`Optimum.dmg`](../../releases/latest/download/Optimum.dmg), see [macOS](#macos).

The builds aren't code-signed yet, so SmartScreen may warn you. Click "More info", then "Run anyway".

## Usage

| Action | How |
|---|---|
| Copy | `Ctrl+C` when text is selected (without a selection it's the usual interrupt), `Ctrl+Shift+C`, `Ctrl+Insert`. Selecting with the mouse also copies. |
| Paste | `Ctrl+V`, `Ctrl+Shift+V`, `Shift+Insert`, or right-click |
| Select all | `Ctrl+Shift+A` (`Ctrl+A` stays with the shell) |
| Select | Drag with the mouse. Double-click selects a word, triple-click a line. Shift+click extends the selection. |
| Change the selection | `Shift+←/→/↑/↓`, `Shift+Home/End` |
| Clear the selection | `Esc`, or just keep typing |
| Select in tmux/vim with mouse mode on | Hold Shift |
| Scroll back | Mouse wheel, `Shift+PageUp/PageDown`, `Ctrl+Shift+↑/↓`, `Ctrl+Shift+Home/End` |
| Font size | `Ctrl+=` / `Ctrl+-` / `Ctrl+0`, or `Ctrl`+mouse wheel. It isn't saved; set `size` in the settings for that. |
| Full screen | `F11` or `Alt+Enter` |
| Window menu | `Alt+Space` |
| Move the window | Drag the top edge |
| Close | `Alt+F4`, the taskbar, or `exit` in the shell |
| Settings | `Ctrl+,` |

Inside full-screen apps (vim, less, htop) the scrolling keys go to the app.

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

## macOS

The macOS version is early and not tested much yet. It needs an Apple Silicon Mac with macOS 13 or newer. It opens your login shell (the one set for your user, usually zsh) in your home folder.

### Install on macOS

1. Download [`Optimum.dmg`](../../releases/latest/download/Optimum.dmg).
2. Open it and drag Optimum to Applications.
3. Open Terminal and run this once:

   ```
   xattr -dr com.apple.quarantine /Applications/Optimum.app
   ```

4. Open Optimum from Applications as usual.

Why step 3: Optimum is free and has no paid Apple developer account, so the app isn't signed with an Apple Developer ID or notarized. macOS blocks apps like that when they come from the internet. Depending on the macOS version it says that Apple can't check the app for malicious software, or that the app "is damaged and can't be opened". The app isn't damaged. The command removes the "downloaded from the internet" mark, and macOS stops blocking it. You need to run it again after each update.

If you skipped step 3 and macOS already blocked the app, you can also allow it in **System Settings → Privacy & Security**: scroll down and click **Open Anyway** next to the message about Optimum. This button doesn't appear when macOS says the app is damaged, so the command is the reliable way.

It works like the Windows version, with Mac shortcuts:

| Action | How |
|---|---|
| Copy, paste, select all | `Cmd+C`, `Cmd+V`, `Cmd+A`. Selecting with the mouse also copies; right-click pastes. |
| Change or clear the selection | `Shift+arrows`, `Cmd+Shift+←/→`, `Esc` |
| Scroll back | Trackpad or wheel, `Shift+PageUp/PageDown`, `Cmd+↑/↓`, `Cmd+Home/End` |
| Font size | `Cmd+=` / `Cmd+-` / `Cmd+0` |
| Full screen | `Ctrl+Cmd+F` |
| Close, minimize, hide, quit | `Cmd+W`, `Cmd+M`, `Cmd+H`, `Cmd+Q` |
| Settings | `Cmd+,` opens `~/Library/Application Support/Optimum/config.toml` |

The Option key types characters (like `å`); it is not used as Meta.

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

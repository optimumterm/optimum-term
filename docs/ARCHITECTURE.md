# Architecture

Current state: **M8 (release packaging)**: README, zip packaging, the Inno Setup installer `installer/optimum.iss` (ADR-025) and a tag-triggered release workflow (ADR-022). M7 clarity is signed off with system defaults (ADR-023). `optimum.exe` renders the shell with DirectWrite + D3D11, sends it keyboard, IME, mouse and focus input, and has select-to-copy, right-click paste, OSC 52, wheel scrollback, synchronized output and the WSL-missing panel. Closing the window closes immediately (ADR-021). Standard Windows shortcuts for copy, paste, select all, keyboard selection and scrollback work (ADR-026), plus font zoom, full screen and Shift+click selection (ADR-027). `config.toml` (M6) configures fonts, text rendering, colours, cursor, padding and opacity, with hot reload.

## Crates

| Crate | Status | Role |
|---|---|---|
| `ghostty-vt-sys` | present | `build.rs` runs `zig build -Demit-lib-vt=true` on `vendor/ghostty`, links the static library, generates bindings with `bindgen` from `include/ghostty/vt.h`. No hand-written code. |
| `ghostty-vt` | present | Safe wrapper. Owns every `unsafe` call into `ghostty-vt-sys`. |
| `optimum-core` | present | `Pty` trait, `PtyEvent`, `GridSize`, `Session` (owns the one `Terminal`, its `RenderState` and the one `Pty`), `Frame` (rows of resolved cells + cursor, with dirty flags), layout (`grid_size`, `scale`, `CellSize`), `Rgb`, `Theme`. Later: config, selection. |
| `optimum-win` | partial | `app` (run loop, the one `App`, window procedure, event queue), `window` (frameless helpers, hit testing), `renderer` (D3D11 pipeline, instance building, cursor), `font` (DirectWrite fonts, metrics, fallback, rasterization), `atlas` (glyph cache + packing), `shader.hlsl`, `instance`, `conpty`, `pty`, `wsl`. Later: input, overlays. |
| `optimum-mac` | later | AppKit, Metal, CoreText. |
| `optimum-linux` | later | Window system, GPU API and rasterizer to be decided. |
| `optimum` | present | Binary (`windows` subsystem); on Windows calls `optimum_win::run()`. |
| `optimum-icon` | present | Build helper: turns `assets/optimum.ico` into a `.res` and links it into a binary (ADR-016). Used by `optimum`. |

Dependency direction:

```
optimum → optimum-win | optimum-mac | optimum-linux → optimum-core → ghostty-vt → ghostty-vt-sys → libghostty-vt
```

## Assets

`assets/`: app icon (SVG, 1024 px PNG, generated multi-size `.ico`), embedded into `optimum.exe` by `crates/optimum/build.rs` (ADR-016).

## Fonts

Default font: JetBrainsMono Nerd Font, embedded from `vendor/fonts/` (ADR-020). The `config.toml` family is looked up among installed fonts first, then the bundled font, then Consolas.

## ghostty-vt-sys build

- Ghostty source: `vendor/ghostty` (submodule, pinned commit, see ADR-003).
- Zig: `zig` on `PATH`, or the `ZIG` environment variable.
- Output: `OUT_DIR/ghostty` (prefix) and `OUT_DIR/zig-cache`; the submodule stays clean.
- Optimize mode: `ReleaseFast` for the Cargo `release` profile, `ReleaseSafe` otherwise (ADR-005).
- Cross-compiling passes `-Dtarget=<zig triple>` (the product target `x86_64-pc-windows-gnu` maps to `x86_64-windows-gnu`, ADR-007).
- Link: `static:+verbatim=ghostty-vt-static.lib` (+ `ntdll`, `kernel32`) on Windows, `static=ghostty-vt` elsewhere. The archive is self-contained apart from the C runtime.
- Bindings: `GHOSTTY_STATIC` defined, enums as `c_int` constants without the enum-name prefix (`GHOSTTY_SUCCESS`), `Default` derived for sized option structs.
- Lint: the generated module alone is exempt from `clippy::undocumented_unsafe_blocks`.

## ghostty-vt API (so far)

- `Terminal::new(cols, rows)`, `write(&[u8])`, `resize(cols, rows, cell_w_px, cell_h_px)`, `plain_text()`; freed on `Drop`.
- `Error` maps `GhosttyResult` codes.
- `take_responses()`: bytes the terminal wants to send back to the PTY (DSR, mode reports), collected by the `write_pty` callback during `write`.
- `RenderState`: `update(&Terminal)`, `dirty()`, `size()`, `colors()`, `cursor()`, `dirty_rows()` → per-row `Cells` (graphemes, style, resolved fg/bg, wide), `clean()`.
- `Terminal::set_default_colors(fg, bg, cursor)`.
- `Terminal::mode(Mode)`, `mouse_tracking()`, `alternate_screen()`, `scroll(Scroll)`, `write_capturing` + `held()`/`release_hold()` (mode 2026), `take_clipboard()` (OSC 52), `select_press`/`select_drag`/`select_release`/`selection_text`/`clear_selection` (gesture owned by the terminal), `encode_paste`.
- `KeyEncoder` / `KeyEvent` (physical `Key`, `Mods`, consumed mods, layout text, unshifted code point) and `MouseEncoder` / `MouseEvent` / `MouseGeometry`; both read the terminal's modes (DECCKM, kitty flags, mouse tracking/format) on every encode. `encode_focus`, `codepoint_width`.
- `Terminal` is `!Send`/`!Sync` (raw pointer). It lives on the thread that owns the `Session`.


## Data flow

```
wsl.exe ⇄ ConPTY ── output pipe ──► pty-reader ──callback──► EventQueue ─PostMessage─► UI thread: Session::feed ─► Terminal
                                                                     │                       │
                                                                     ◄── responses (DSR) ────┘
owner thread: Session::input ──► WriteFile(input pipe) ──► ConPTY ──► wsl.exe
pty-waiter: process exit ─► ClosePseudoConsole ─► join reader ─► callback(Exited)
```

- Launch: `"%SystemRoot%\System32\wsl.exe" --cd ~`, fixed. Environment block = current environment + `TERM`, `COLORTERM`, `TERM_PROGRAM`, `TERM_PROGRAM_VERSION`, with those names appended to `WSLENV` as `/u` (existing entries kept, duplicates replaced). Child std handles are set to `INVALID_HANDLE_VALUE` so it never inherits our console.
- `Session::resize` resizes the terminal first, then ConPTY, and skips no-op resizes.
- UI thread: one `App` owns the window, the renderer and `Option<Session<WinPty>>`. A worker thread runs `WslStatus::detect()` and `WinPty::spawn_wsl()`, then posts `Started`/`WslUnavailable`/`SpawnFailed`. `PtyEvent::Exited` destroys the window, which ends the process.
- Render: after a batch of PTY output, `Session::refresh()` updates the `Frame` from the render state (dirty rows only); `Renderer::render` rebuilds instances for dirty rows, uploads new glyphs to the atlas, draws once, presents. See ADR-014.
- Input (ADR-015): `WM_KEYDOWN` → scancode → physical `Key`; the `WM_CHAR`s that `TranslateMessage` queued for that key are taken with `PeekMessageW` before `App::handle` runs and become the event text → `Session::key` → `KeyEncoder` → PTY. IME composition (IMM32) is drawn by the renderer at the cursor; committed text goes to the PTY as is. Mouse → `Session::mouse` only while the app enabled tracking and Shift is not held. `WM_SETFOCUS`/`WM_KILLFOCUS` → focus reports (mode 1004) and a hollow cursor.
- M5 (ADR-017): mouse → selection gesture or app report → `After::Copy` (clipboard) on release; right-click → `After::Paste` → clipboard read → `Session::paste`. Wheel → app report, alt-screen arrows, or viewport scroll. OSC 52 text collected during `feed` → `After::Copy`. Sync hold → frame frozen, 250 ms timer releases holds over 1 s.
- M6 (ADR-018): `settings::load()` at startup → `Config` → `Renderer::new`/`Session::new`. Watcher thread → `ConfigChanged` → `App::reload` → `Renderer::configure`, `Session::configure`, resize, `After::Opacity`. Parse errors keep the old config and show a bottom error strip. `Ctrl+,` → `After::OpenSettings`.
- M7 (ADR-019): debug builds only: `App::tune` intercepts `Ctrl+Shift+F11`/`F12` and, while the panel is open, the arrows and Esc → `Renderer::set_text_params` or `Renderer::configure` (font) → overlay lines from `tuner::Tuner::lines`.
- Resize: `WM_SIZE` → swap chain `ResizeBuffers` → `grid_size(client, padding, cell)` → `Session::resize` (terminal, then ConPTY). `WM_DPICHANGED` applies the suggested rect.

## Tests

- `ghostty-vt`: smoke tests, DSR reply, key/mouse/focus encoding (legacy, application cursor keys, kitty, Cyrillic Ctrl, SGR mouse); selection drag/word/line, OSC 52, paste encoding, viewport scroll, alt screen, render-hold capture.
- `optimum-core`: `Config` parsing (defaults file = `Config::default()`, values, error line numbers), 256-colour palette, `Session::configure` recolouring; `Session` against a fake PTY, including keys, focus reports, selection highlight + copy text, bracketed paste, alt-screen wheel, stuck sync-block release; `cell_at`, `wrap`.
- `optimum-win`: shader compilation, tuner stepping/ranges/layout; hit testing and lparam decoding; scancode → key map, AltGr/Ctrl text rules, `Ctrl+,` detection; bundled and inbox ConPTY run `cmd /c`, exit codes, environment block/WSLENV, UTF-16 decoding. `#[ignore]`d (need a distro, not on CI): WSL sees `TERM`/`COLORTERM`/`TERM_PROGRAM`, WSL sees a resize (`stty size`), `WslStatus::detect() == Ready`. Run them with `-- --include-ignored`. They use `wsl.exe -e sh -c`, never the interactive login shell.

## Manual checks done for M2 (144 DPI)

Frameless window with shadow, rounded corners and `#121212` clear; second launch exits 0 in ~25 ms and restores + foregrounds a minimized first window; `WM_CLOSE` closes; killing the shell's `wsl.exe` closes the window; ConPTY is the bundled `OpenConsole.exe` and follows the window's grid.

## Manual checks done for M3

At 144 DPI with system ClearType: the shell prompt renders in its colours with a block cursor; a zoomed capture shows whole-pixel stems, one baseline, and ClearType colour fringes. Final clarity judgement is made by eye on a real monitor.

## M4: what is verified and what is not

Verified by tests: encoder output for all key paths, scancode mapping, AltGr/Ctrl/Shift text rules, mouse and focus encoding. Verified live: posted key messages reach the shell. **Not verified live**: real typing with modifiers, AltGr, dead keys, IME, mouse in tmux/vim. These are checked by hand. Automated keystroke injection into the window is not used, because it reaches whatever shell is running there.

## M5: what is verified and what is not

Verified by tests: everything listed above, on Windows and Linux. Verified live: the overlay panel renders centred in the window (originally the close panel, removed in ADR-021). **Not verified live** (checked by hand): mouse selection and copy, right-click paste, wheel scrollback, tmux/vim mouse with Shift override, OSC 52 from tmux, sync output in a TUI DPI move between monitors. The clipboard code is not exercised by automated tests, because that would overwrite the user's clipboard.

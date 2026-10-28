# Clarity workflow

The goal is text that looks at least as sharp as Windows Terminal at the same font, size and DPI. The final judgement is made by eye, on a real monitor. Screenshots only help to find concrete pixel differences.

## 1. Tune live (debug build)

1. Run a debug build (`cargo-zigbuild build --target x86_64-pc-windows-gnu -p optimum`). The release build has no tuner.
2. In the window, `cat` the sample file: `cat /mnt/c/.../docs/clarity/sample.txt`.
3. Press `Ctrl+Shift+F11` (or `Ctrl+Shift+F12`; F12 is often taken by Steam or other overlays) to open the tuner panel. These keys exist only in debug builds.
   - Up and Down select a parameter, Left and Right change it, Esc or the same key closes the panel.
   - Parameters: antialiasing (cleartype or grayscale), gamma, ClearType enhanced contrast, grayscale enhanced contrast, ClearType level, font size, line height.
   - The panel starts from the active values (system defaults plus `config.toml`), and changes apply immediately. Nothing is saved: write good values into `config.toml` by hand (`[text] gamma`, `contrast`, `antialiasing`).
4. Things to judge:
   - **stems:** `m n h u` have even, one-pixel-sharp verticals, with no smear into a second column;
   - **ClearType fringes:** coloured edges on `l I 1 |` are visible under magnification but not as colour at normal distance;
   - **weight:** light text on the dark background is neither thin nor bloated compared to Windows Terminal;
   - **coloured text:** red, blue and grey text (lines 8–10 of the sample) keep their weight, which is where gamma correction matters most;
   - **Cyrillic and CJK** glyphs (fallback fonts) sit on the same baseline.

## 2. Compare against Windows Terminal

1. Set up Windows Terminal with the same font (install the same JetBrainsMono Nerd Font TTFs from `vendor/fonts/` and select "JetBrainsMono Nerd Font"), size (12), `antialiasingMode` (`cleartype` or `grayscale`) and opacity 100%. Put it on the same monitor as Optimum.
2. Run `cat sample.txt` in both, with the prompt at the top of the window.
3. Run `powershell -File docs\clarity\compare.ps1` from the repository on Windows. The windows are captured with `PrintWindow`, so they may be covered, but they must not be minimised.
   - `-OptimumOrigin x,y` and `-TerminalOrigin x,y` give the top-left pixel of the first text cell in each client area. Optimum's is 12 px × DPI scale (18,18 at 150%). For Windows Terminal, use the first cell below its tab bar.
   - `-Size w,h` is the region compared. `-ZoomOrigin` and `-ZoomSize` pick the part to magnify ×`-Zoom`.
4. Output in `docs/clarity/captures/`:
   - `optimum.png` and `terminal.png`, the native-pixel crops;
   - `zoom-optimum-vs-terminal.png`, nearest-neighbour magnified, Optimum on the left;
   - `diff.png`, the per-pixel maximum channel difference ×4, plus the mean difference printed to the console.

   Small differences are expected (different line height rounding, cursor, colours). The interesting ones sit inside glyphs at the same position.
5. When a setting is signed off, keep the captures here as the reference and log the values and the date in `docs/DECISIONS.md`.

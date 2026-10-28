use std::ffi::c_void;
use std::mem::{self, size_of};
use std::ptr::{self, NonNull};
use std::slice;

use ghostty_vt_sys as sys;

use crate::selection::SelectionGesture;
use crate::{CursorStyle, Error, RenderState, Rgb};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    FocusEvent,
    BracketedPaste,
    AltScroll,
    SyncOutput,
}

impl Mode {
    fn raw(self) -> sys::GhosttyMode {
        let (value, ansi) = match self {
            Mode::FocusEvent => (1004, false),
            Mode::BracketedPaste => (2004, false),
            Mode::AltScroll => (1007, false),
            Mode::SyncOutput => (2026, false),
        };
        (value & 0x7fff) | (u16::from(ansi) << 15)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scroll {
    Top,
    Bottom,
    Lines(isize),
}

struct Shared {
    responses: Vec<u8>,
    clipboard: Option<String>,
    held: bool,
    capture: *mut sys::GhosttyRenderStateImpl,
}

pub struct Terminal {
    pub(crate) raw: NonNull<sys::GhosttyTerminalImpl>,
    shared: NonNull<Shared>,
    pub(crate) gesture: SelectionGesture,
}

impl Terminal {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        let mut raw: sys::GhosttyTerminal = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `raw` is a valid out-pointer.
        Error::check(unsafe { sys::ghostty_terminal_new(ptr::null(), &mut raw, cols, rows) })?;
        let raw = NonNull::new(raw).ok_or(Error::InvalidValue)?;
        let gesture = match SelectionGesture::new() {
            Ok(gesture) => gesture,
            Err(e) => {
                // SAFETY: `raw` was just created and is not used again.
                unsafe { sys::ghostty_terminal_free(raw.as_ptr()) };
                return Err(e);
            }
        };
        let shared = NonNull::from(Box::leak(Box::new(Shared {
            responses: Vec::new(),
            clipboard: None,
            held: false,
            capture: ptr::null_mut(),
        })));
        let terminal = Self {
            raw,
            shared,
            gesture,
        };
        terminal.set(
            sys::GHOSTTY_TERMINAL_OPT_USERDATA,
            shared.as_ptr().cast::<c_void>(),
        )?;
        let write: unsafe extern "C" fn(sys::GhosttyTerminal, *mut c_void, *const u8, usize) =
            write_pty;
        terminal.set(sys::GHOSTTY_TERMINAL_OPT_WRITE_PTY, write as *const c_void)?;
        let clipboard: unsafe extern "C" fn(
            sys::GhosttyTerminal,
            *mut c_void,
            *const sys::GhosttyClipboardWrite,
        ) = clipboard_write;
        terminal.set(
            sys::GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE,
            clipboard as *const c_void,
        )?;
        let hold: unsafe extern "C" fn(sys::GhosttyTerminal, *mut c_void, bool) = render_hold;
        terminal.set(sys::GHOSTTY_TERMINAL_OPT_RENDER_HOLD, hold as *const c_void)?;
        Ok(terminal)
    }

    fn shared(&mut self) -> &mut Shared {
        // SAFETY: `shared` is owned by `self` and only aliased by the callbacks, which run inside `write` while `&mut self` is held, never concurrently with this borrow.
        unsafe { self.shared.as_mut() }
    }

    pub fn write_capturing(&mut self, bytes: &[u8], render: &mut RenderState) {
        self.shared().capture = render.raw.as_ptr();
        self.write(bytes);
        self.shared().capture = ptr::null_mut();
    }

    pub fn held(&self) -> bool {
        // SAFETY: a plain read of state owned by `self`; no callback is running.
        unsafe { self.shared.as_ref() }.held
    }

    pub fn release_hold(&mut self) {
        let mode = sys::GhosttyTerminalModeConfig {
            mode: Mode::SyncOutput.raw(),
            value: false,
        };
        let _ = self.set(
            sys::GHOSTTY_TERMINAL_OPT_MODE,
            (&raw const mode).cast::<c_void>(),
        );
        self.shared().held = false;
    }

    pub fn take_clipboard(&mut self) -> Option<String> {
        self.shared().clipboard.take()
    }

    pub fn scroll(&mut self, scroll: Scroll) {
        let mut behavior = sys::GhosttyTerminalScrollViewport::default();
        match scroll {
            Scroll::Top => behavior.tag = sys::GHOSTTY_SCROLL_VIEWPORT_TOP,
            Scroll::Bottom => behavior.tag = sys::GHOSTTY_SCROLL_VIEWPORT_BOTTOM,
            Scroll::Lines(delta) => {
                behavior.tag = sys::GHOSTTY_SCROLL_VIEWPORT_DELTA;
                behavior.value.delta = delta;
            }
        }
        // SAFETY: `self.raw` is live and `behavior` is a fully initialised tagged union.
        unsafe { sys::ghostty_terminal_scroll_viewport(self.raw.as_ptr(), behavior) }
    }

    pub fn alternate_screen(&self) -> bool {
        let mut screen: sys::GhosttyTerminalScreen = sys::GHOSTTY_TERMINAL_SCREEN_PRIMARY;
        self.get(sys::GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN, &mut screen)
            .is_ok()
            && screen == sys::GHOSTTY_TERMINAL_SCREEN_ALTERNATE
    }

    fn set(&self, option: sys::GhosttyTerminalOption, value: *const c_void) -> Result<(), Error> {
        // SAFETY: `self.raw` is live; `value` has the type `option` expects, and outlives the terminal.
        Error::check(unsafe { sys::ghostty_terminal_set(self.raw.as_ptr(), option, value) })
    }

    pub fn set_default_colors(
        &mut self,
        foreground: Rgb,
        background: Rgb,
        cursor: Option<Rgb>,
    ) -> Result<(), Error> {
        let foreground = sys::GhosttyColorRgb::from(foreground);
        let background = sys::GhosttyColorRgb::from(background);
        let cursor = cursor.map(sys::GhosttyColorRgb::from);
        let ptr = |c: &sys::GhosttyColorRgb| (c as *const sys::GhosttyColorRgb).cast::<c_void>();
        self.set(sys::GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND, ptr(&foreground))?;
        self.set(sys::GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND, ptr(&background))?;
        self.set(
            sys::GHOSTTY_TERMINAL_OPT_COLOR_CURSOR,
            cursor.as_ref().map_or(ptr::null(), ptr),
        )
    }

    pub fn set_palette(&mut self, palette: &[Rgb; 256]) -> Result<(), Error> {
        let palette = palette.map(sys::GhosttyColorRgb::from);
        self.set(
            sys::GHOSTTY_TERMINAL_OPT_COLOR_PALETTE,
            palette.as_ptr().cast::<c_void>(),
        )
    }

    pub fn set_default_cursor(&mut self, style: CursorStyle, blink: bool) -> Result<(), Error> {
        let style = match style {
            CursorStyle::Bar => sys::GHOSTTY_TERMINAL_CURSOR_STYLE_BAR,
            CursorStyle::Underline => sys::GHOSTTY_TERMINAL_CURSOR_STYLE_UNDERLINE,
            CursorStyle::BlockHollow => sys::GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK_HOLLOW,
            CursorStyle::Block => sys::GHOSTTY_TERMINAL_CURSOR_STYLE_BLOCK,
        };
        self.set(
            sys::GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE,
            (&raw const style).cast::<c_void>(),
        )?;
        self.set(
            sys::GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK,
            (&raw const blink).cast::<c_void>(),
        )
    }

    pub(crate) fn get<T>(&self, data: sys::GhosttyTerminalData, out: &mut T) -> Result<(), Error> {
        // SAFETY: `self.raw` is live; `out` points to a value of the type `data` writes.
        Error::check(unsafe {
            sys::ghostty_terminal_get(self.raw.as_ptr(), data, (out as *mut T).cast::<c_void>())
        })
    }

    pub fn mode(&self, mode: Mode) -> bool {
        let mut config = sys::GhosttyTerminalModeConfig {
            mode: mode.raw(),
            value: false,
        };
        self.get(sys::GHOSTTY_TERMINAL_DATA_MODE, &mut config)
            .is_ok()
            && config.value
    }

    pub fn mouse_tracking(&self) -> bool {
        let mut tracking = false;
        self.get(sys::GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING, &mut tracking)
            .is_ok()
            && tracking
    }

    pub fn take_responses(&mut self) -> Vec<u8> {
        mem::take(&mut self.shared().responses)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        // SAFETY: `self.raw` is a live terminal owned by `self`; `bytes` is valid for `len` bytes.
        unsafe { sys::ghostty_terminal_vt_write(self.raw.as_ptr(), bytes.as_ptr(), bytes.len()) }
    }

    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<(), Error> {
        // SAFETY: `self.raw` is a live terminal owned by `self`.
        Error::check(unsafe {
            sys::ghostty_terminal_resize(
                self.raw.as_ptr(),
                cols,
                rows,
                cell_width_px,
                cell_height_px,
            )
        })
    }

    pub fn plain_text(&self) -> Result<String, Error> {
        let options = sys::GhosttyFormatterTerminalOptions {
            size: size_of::<sys::GhosttyFormatterTerminalOptions>(),
            emit: sys::GHOSTTY_FORMATTER_FORMAT_PLAIN,
            trim: true,
            extra: sys::GhosttyFormatterTerminalExtra {
                size: size_of::<sys::GhosttyFormatterTerminalExtra>(),
                screen: sys::GhosttyFormatterScreenExtra {
                    size: size_of::<sys::GhosttyFormatterScreenExtra>(),
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let formatter = Formatter::new(self, options)?;
        let bytes = formatter.format()?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // SAFETY: the gesture is released against this still-live terminal, then the terminal (from ghostty_terminal_new) is freed exactly once.
        unsafe {
            self.gesture.destroy(self.raw.as_ptr());
            sys::ghostty_terminal_free(self.raw.as_ptr());
        }
        // SAFETY: `shared` came from Box::leak in `new`; the terminal that referenced it is already freed.
        drop(unsafe { Box::from_raw(self.shared.as_ptr()) });
    }
}

unsafe extern "C" fn write_pty(
    _terminal: sys::GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    if userdata.is_null() || data.is_null() || len == 0 {
        return;
    }
    // SAFETY: `userdata` is the `Shared` registered in `Terminal::new`; it is not otherwise borrowed during a VT write.
    let shared = unsafe { &mut *userdata.cast::<Shared>() };
    // SAFETY: the library guarantees `data` is valid for `len` bytes for the duration of this call.
    shared
        .responses
        .extend_from_slice(unsafe { slice::from_raw_parts(data, len) });
}

unsafe extern "C" fn clipboard_write(
    _terminal: sys::GhosttyTerminal,
    userdata: *mut c_void,
    write: *const sys::GhosttyClipboardWrite,
) {
    if userdata.is_null() || write.is_null() {
        return;
    }
    // SAFETY: `userdata` is the registered `Shared`; `write` and everything it points to are valid for the duration of this call.
    let (shared, write) = unsafe { (&mut *userdata.cast::<Shared>(), &*write) };
    let contents = if write.contents.is_null() {
        &[][..]
    } else {
        // SAFETY: `contents` holds `contents_len` entries for the duration of this call.
        unsafe { slice::from_raw_parts(write.contents, write.contents_len) }
    };
    let text = contents.iter().find_map(|content| {
        // SAFETY: MIME and data strings are borrowed for the duration of this call.
        let (mime, data) = unsafe { (bytes(content.mime), bytes(content.data)) };
        mime.starts_with(b"text/plain")
            .then(|| String::from_utf8_lossy(data).into_owned())
    });
    let result = match (&text, contents.is_empty()) {
        (Some(_), _) | (None, true) => sys::GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS,
        (None, false) => sys::GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED,
    };
    shared.clipboard = Some(text.unwrap_or_default());
    let reply = sys::GhosttyClipboardWriteReply {
        size: size_of::<sys::GhosttyClipboardWriteReply>(),
        result,
        remember: false,
    };
    if let Some(answer) = write.reply {
        // SAFETY: replying inside the callback, as the contract requires, with a fully initialised reply.
        unsafe { answer(write, &reply) };
    }
}

unsafe fn bytes<'a>(string: sys::GhosttyString) -> &'a [u8] {
    if string.ptr.is_null() {
        return &[];
    }
    // SAFETY: the caller guarantees `string` points to `len` live bytes for 'a.
    unsafe { slice::from_raw_parts(string.ptr, string.len) }
}

unsafe extern "C" fn render_hold(
    terminal: sys::GhosttyTerminal,
    userdata: *mut c_void,
    held: bool,
) {
    if userdata.is_null() {
        return;
    }
    // SAFETY: `userdata` is the registered `Shared`; it is not otherwise borrowed during a VT write.
    let shared = unsafe { &mut *userdata.cast::<Shared>() };
    if held && !shared.capture.is_null() {
        // SAFETY: `capture` is the render state lent to `write_capturing` for exactly this write; the library allows updating it from this callback.
        let _ = unsafe { sys::ghostty_render_state_update(shared.capture, terminal) };
    }
    shared.held = held;
}

pub fn encode_paste(text: &str, bracketed: bool) -> Result<Vec<u8>, Error> {
    let mut data = text.as_bytes().to_vec();
    let mut out = vec![0u8; data.len() + 16];
    let mut written = 0;
    for _ in 0..2 {
        // SAFETY: `data` and `out` are valid for their lengths; `written` is a valid out-pointer.
        let result = Error::check(unsafe {
            sys::ghostty_paste_encode(
                data.as_mut_ptr().cast(),
                data.len(),
                bracketed,
                out.as_mut_ptr().cast(),
                out.len(),
                &mut written,
            )
        });
        match result {
            Ok(()) => {
                out.truncate(written);
                return Ok(out);
            }
            Err(Error::OutOfSpace) => {
                data = text.as_bytes().to_vec();
                out.resize(written, 0);
            }
            Err(e) => return Err(e),
        }
    }
    Err(Error::OutOfSpace)
}

struct Formatter<'t> {
    raw: NonNull<sys::GhosttyFormatterImpl>,
    _terminal: &'t Terminal,
}

impl<'t> Formatter<'t> {
    fn new(
        terminal: &'t Terminal,
        options: sys::GhosttyFormatterTerminalOptions,
    ) -> Result<Self, Error> {
        let mut raw: sys::GhosttyFormatter = ptr::null_mut();
        // SAFETY: `terminal.raw` is live for 't, which outlives the formatter; `raw` is a valid out-pointer.
        Error::check(unsafe {
            sys::ghostty_formatter_terminal_new(
                ptr::null(),
                &mut raw,
                terminal.raw.as_ptr(),
                options,
            )
        })?;
        let raw = NonNull::new(raw).ok_or(Error::InvalidValue)?;
        Ok(Self {
            raw,
            _terminal: terminal,
        })
    }

    fn format(&self) -> Result<Vec<u8>, Error> {
        let mut needed = 0;
        // SAFETY: a null buffer with zero length only queries the required size into `needed`.
        match Error::check(unsafe {
            sys::ghostty_formatter_format_buf(self.raw.as_ptr(), ptr::null_mut(), 0, &mut needed)
        }) {
            Ok(()) | Err(Error::OutOfSpace) => {}
            Err(e) => return Err(e),
        }

        let mut buf = vec![0u8; needed];
        let mut written = 0;
        // SAFETY: `buf` is valid for writes of `buf.len()` bytes; `written` is a valid out-pointer.
        Error::check(unsafe {
            sys::ghostty_formatter_format_buf(
                self.raw.as_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut written,
            )
        })?;
        buf.truncate(written);
        Ok(buf)
    }
}

impl Drop for Formatter<'_> {
    fn drop(&mut self) {
        // SAFETY: `self.raw` came from ghostty_formatter_terminal_new and is freed exactly once, here.
        unsafe { sys::ghostty_formatter_free(self.raw.as_ptr()) }
    }
}

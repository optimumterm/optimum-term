use std::ops::{BitOr, BitOrAssign};
use std::ptr::{self, NonNull};

use ghostty_vt_sys as sys;

use crate::{Error, Terminal};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Mods(u16);

impl Mods {
    pub const NONE: Mods = Mods(0);
    pub const SHIFT: Mods = Mods(sys::GHOSTTY_MODS_SHIFT as u16);
    pub const CTRL: Mods = Mods(sys::GHOSTTY_MODS_CTRL as u16);
    pub const ALT: Mods = Mods(sys::GHOSTTY_MODS_ALT as u16);
    pub const SUPER: Mods = Mods(sys::GHOSTTY_MODS_SUPER as u16);
    pub const CAPS_LOCK: Mods = Mods(sys::GHOSTTY_MODS_CAPS_LOCK as u16);
    pub const NUM_LOCK: Mods = Mods(sys::GHOSTTY_MODS_NUM_LOCK as u16);
    pub const RIGHT_SHIFT: Mods = Mods(sys::GHOSTTY_MODS_SHIFT_SIDE as u16);
    pub const RIGHT_CTRL: Mods = Mods(sys::GHOSTTY_MODS_CTRL_SIDE as u16);
    pub const RIGHT_ALT: Mods = Mods(sys::GHOSTTY_MODS_ALT_SIDE as u16);
    pub const RIGHT_SUPER: Mods = Mods(sys::GHOSTTY_MODS_SUPER_SIDE as u16);

    pub fn contains(self, other: Mods) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn intersects(self, other: Mods) -> bool {
        self.0 & other.0 != 0
    }

    pub fn without(self, other: Mods) -> Mods {
        Mods(self.0 & !other.0)
    }

    pub fn binding(self) -> Mods {
        Mods(self.0 & (Mods::SHIFT | Mods::CTRL | Mods::ALT | Mods::SUPER).0)
    }
}

impl BitOr for Mods {
    type Output = Mods;

    fn bitor(self, rhs: Mods) -> Mods {
        Mods(self.0 | rhs.0)
    }
}

impl BitOrAssign for Mods {
    fn bitor_assign(&mut self, rhs: Mods) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Key {
    Unidentified = sys::GHOSTTY_KEY_UNIDENTIFIED,
    Backquote = sys::GHOSTTY_KEY_BACKQUOTE,
    Backslash = sys::GHOSTTY_KEY_BACKSLASH,
    BracketLeft = sys::GHOSTTY_KEY_BRACKET_LEFT,
    BracketRight = sys::GHOSTTY_KEY_BRACKET_RIGHT,
    Comma = sys::GHOSTTY_KEY_COMMA,
    Digit0 = sys::GHOSTTY_KEY_DIGIT_0,
    Digit1 = sys::GHOSTTY_KEY_DIGIT_1,
    Digit2 = sys::GHOSTTY_KEY_DIGIT_2,
    Digit3 = sys::GHOSTTY_KEY_DIGIT_3,
    Digit4 = sys::GHOSTTY_KEY_DIGIT_4,
    Digit5 = sys::GHOSTTY_KEY_DIGIT_5,
    Digit6 = sys::GHOSTTY_KEY_DIGIT_6,
    Digit7 = sys::GHOSTTY_KEY_DIGIT_7,
    Digit8 = sys::GHOSTTY_KEY_DIGIT_8,
    Digit9 = sys::GHOSTTY_KEY_DIGIT_9,
    Equal = sys::GHOSTTY_KEY_EQUAL,
    IntlBackslash = sys::GHOSTTY_KEY_INTL_BACKSLASH,
    IntlRo = sys::GHOSTTY_KEY_INTL_RO,
    IntlYen = sys::GHOSTTY_KEY_INTL_YEN,
    A = sys::GHOSTTY_KEY_A,
    B = sys::GHOSTTY_KEY_B,
    C = sys::GHOSTTY_KEY_C,
    D = sys::GHOSTTY_KEY_D,
    E = sys::GHOSTTY_KEY_E,
    F = sys::GHOSTTY_KEY_F,
    G = sys::GHOSTTY_KEY_G,
    H = sys::GHOSTTY_KEY_H,
    I = sys::GHOSTTY_KEY_I,
    J = sys::GHOSTTY_KEY_J,
    K = sys::GHOSTTY_KEY_K,
    L = sys::GHOSTTY_KEY_L,
    M = sys::GHOSTTY_KEY_M,
    N = sys::GHOSTTY_KEY_N,
    O = sys::GHOSTTY_KEY_O,
    P = sys::GHOSTTY_KEY_P,
    Q = sys::GHOSTTY_KEY_Q,
    R = sys::GHOSTTY_KEY_R,
    S = sys::GHOSTTY_KEY_S,
    T = sys::GHOSTTY_KEY_T,
    U = sys::GHOSTTY_KEY_U,
    V = sys::GHOSTTY_KEY_V,
    W = sys::GHOSTTY_KEY_W,
    X = sys::GHOSTTY_KEY_X,
    Y = sys::GHOSTTY_KEY_Y,
    Z = sys::GHOSTTY_KEY_Z,
    Minus = sys::GHOSTTY_KEY_MINUS,
    Period = sys::GHOSTTY_KEY_PERIOD,
    Quote = sys::GHOSTTY_KEY_QUOTE,
    Semicolon = sys::GHOSTTY_KEY_SEMICOLON,
    Slash = sys::GHOSTTY_KEY_SLASH,
    AltLeft = sys::GHOSTTY_KEY_ALT_LEFT,
    AltRight = sys::GHOSTTY_KEY_ALT_RIGHT,
    Backspace = sys::GHOSTTY_KEY_BACKSPACE,
    CapsLock = sys::GHOSTTY_KEY_CAPS_LOCK,
    ContextMenu = sys::GHOSTTY_KEY_CONTEXT_MENU,
    ControlLeft = sys::GHOSTTY_KEY_CONTROL_LEFT,
    ControlRight = sys::GHOSTTY_KEY_CONTROL_RIGHT,
    Enter = sys::GHOSTTY_KEY_ENTER,
    MetaLeft = sys::GHOSTTY_KEY_META_LEFT,
    MetaRight = sys::GHOSTTY_KEY_META_RIGHT,
    ShiftLeft = sys::GHOSTTY_KEY_SHIFT_LEFT,
    ShiftRight = sys::GHOSTTY_KEY_SHIFT_RIGHT,
    Space = sys::GHOSTTY_KEY_SPACE,
    Tab = sys::GHOSTTY_KEY_TAB,
    Convert = sys::GHOSTTY_KEY_CONVERT,
    KanaMode = sys::GHOSTTY_KEY_KANA_MODE,
    NonConvert = sys::GHOSTTY_KEY_NON_CONVERT,
    Delete = sys::GHOSTTY_KEY_DELETE,
    End = sys::GHOSTTY_KEY_END,
    Home = sys::GHOSTTY_KEY_HOME,
    Insert = sys::GHOSTTY_KEY_INSERT,
    PageDown = sys::GHOSTTY_KEY_PAGE_DOWN,
    PageUp = sys::GHOSTTY_KEY_PAGE_UP,
    ArrowDown = sys::GHOSTTY_KEY_ARROW_DOWN,
    ArrowLeft = sys::GHOSTTY_KEY_ARROW_LEFT,
    ArrowRight = sys::GHOSTTY_KEY_ARROW_RIGHT,
    ArrowUp = sys::GHOSTTY_KEY_ARROW_UP,
    NumLock = sys::GHOSTTY_KEY_NUM_LOCK,
    Numpad0 = sys::GHOSTTY_KEY_NUMPAD_0,
    Numpad1 = sys::GHOSTTY_KEY_NUMPAD_1,
    Numpad2 = sys::GHOSTTY_KEY_NUMPAD_2,
    Numpad3 = sys::GHOSTTY_KEY_NUMPAD_3,
    Numpad4 = sys::GHOSTTY_KEY_NUMPAD_4,
    Numpad5 = sys::GHOSTTY_KEY_NUMPAD_5,
    Numpad6 = sys::GHOSTTY_KEY_NUMPAD_6,
    Numpad7 = sys::GHOSTTY_KEY_NUMPAD_7,
    Numpad8 = sys::GHOSTTY_KEY_NUMPAD_8,
    Numpad9 = sys::GHOSTTY_KEY_NUMPAD_9,
    NumpadAdd = sys::GHOSTTY_KEY_NUMPAD_ADD,
    NumpadComma = sys::GHOSTTY_KEY_NUMPAD_COMMA,
    NumpadDecimal = sys::GHOSTTY_KEY_NUMPAD_DECIMAL,
    NumpadDivide = sys::GHOSTTY_KEY_NUMPAD_DIVIDE,
    NumpadEnter = sys::GHOSTTY_KEY_NUMPAD_ENTER,
    NumpadEqual = sys::GHOSTTY_KEY_NUMPAD_EQUAL,
    NumpadMultiply = sys::GHOSTTY_KEY_NUMPAD_MULTIPLY,
    NumpadSubtract = sys::GHOSTTY_KEY_NUMPAD_SUBTRACT,
    NumpadUp = sys::GHOSTTY_KEY_NUMPAD_UP,
    NumpadDown = sys::GHOSTTY_KEY_NUMPAD_DOWN,
    NumpadRight = sys::GHOSTTY_KEY_NUMPAD_RIGHT,
    NumpadLeft = sys::GHOSTTY_KEY_NUMPAD_LEFT,
    NumpadBegin = sys::GHOSTTY_KEY_NUMPAD_BEGIN,
    NumpadHome = sys::GHOSTTY_KEY_NUMPAD_HOME,
    NumpadEnd = sys::GHOSTTY_KEY_NUMPAD_END,
    NumpadInsert = sys::GHOSTTY_KEY_NUMPAD_INSERT,
    NumpadDelete = sys::GHOSTTY_KEY_NUMPAD_DELETE,
    NumpadPageUp = sys::GHOSTTY_KEY_NUMPAD_PAGE_UP,
    NumpadPageDown = sys::GHOSTTY_KEY_NUMPAD_PAGE_DOWN,
    Escape = sys::GHOSTTY_KEY_ESCAPE,
    F1 = sys::GHOSTTY_KEY_F1,
    F2 = sys::GHOSTTY_KEY_F2,
    F3 = sys::GHOSTTY_KEY_F3,
    F4 = sys::GHOSTTY_KEY_F4,
    F5 = sys::GHOSTTY_KEY_F5,
    F6 = sys::GHOSTTY_KEY_F6,
    F7 = sys::GHOSTTY_KEY_F7,
    F8 = sys::GHOSTTY_KEY_F8,
    F9 = sys::GHOSTTY_KEY_F9,
    F10 = sys::GHOSTTY_KEY_F10,
    F11 = sys::GHOSTTY_KEY_F11,
    F12 = sys::GHOSTTY_KEY_F12,
    F13 = sys::GHOSTTY_KEY_F13,
    F14 = sys::GHOSTTY_KEY_F14,
    F15 = sys::GHOSTTY_KEY_F15,
    F16 = sys::GHOSTTY_KEY_F16,
    F17 = sys::GHOSTTY_KEY_F17,
    F18 = sys::GHOSTTY_KEY_F18,
    F19 = sys::GHOSTTY_KEY_F19,
    F20 = sys::GHOSTTY_KEY_F20,
    F21 = sys::GHOSTTY_KEY_F21,
    F22 = sys::GHOSTTY_KEY_F22,
    F23 = sys::GHOSTTY_KEY_F23,
    F24 = sys::GHOSTTY_KEY_F24,
    PrintScreen = sys::GHOSTTY_KEY_PRINT_SCREEN,
    ScrollLock = sys::GHOSTTY_KEY_SCROLL_LOCK,
    Pause = sys::GHOSTTY_KEY_PAUSE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Repeat,
    Release,
}

impl KeyAction {
    fn raw(self) -> sys::GhosttyKeyAction {
        match self {
            KeyAction::Press => sys::GHOSTTY_KEY_ACTION_PRESS,
            KeyAction::Repeat => sys::GHOSTTY_KEY_ACTION_REPEAT,
            KeyAction::Release => sys::GHOSTTY_KEY_ACTION_RELEASE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent<'a> {
    pub action: KeyAction,
    pub key: Key,
    pub mods: Mods,
    pub consumed: Mods,
    pub text: &'a str,
    pub unshifted: Option<char>,
}

impl KeyEvent<'_> {
    pub fn press(key: Key, mods: Mods) -> KeyEvent<'static> {
        KeyEvent {
            action: KeyAction::Press,
            key,
            mods,
            consumed: Mods::NONE,
            text: "",
            unshifted: None,
        }
    }
}

pub struct KeyEncoder {
    raw: NonNull<sys::GhosttyKeyEncoderImpl>,
    event: NonNull<sys::GhosttyKeyEventImpl>,
}

impl KeyEncoder {
    pub fn new() -> Result<Self, Error> {
        let mut raw: sys::GhosttyKeyEncoder = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `raw` is a valid out-pointer.
        Error::check(unsafe { sys::ghostty_key_encoder_new(ptr::null(), &mut raw) })?;
        let raw = NonNull::new(raw).ok_or(Error::InvalidValue)?;
        let mut event: sys::GhosttyKeyEvent = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `event` is a valid out-pointer.
        let created = Error::check(unsafe { sys::ghostty_key_event_new(ptr::null(), &mut event) });
        let Some(event) = created.ok().and_then(|()| NonNull::new(event)) else {
            // SAFETY: `raw` came from ghostty_key_encoder_new and is not used again.
            unsafe { sys::ghostty_key_encoder_free(raw.as_ptr()) };
            return Err(created.err().unwrap_or(Error::InvalidValue));
        };
        Ok(Self { raw, event })
    }

    pub fn encode(
        &mut self,
        terminal: &Terminal,
        event: &KeyEvent<'_>,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let (raw, ev) = (self.raw.as_ptr(), self.event.as_ptr());
        // SAFETY: the encoder, the event and the terminal are live; `event.text` outlives the encode call below, and the pointer is cleared before returning.
        unsafe {
            sys::ghostty_key_encoder_setopt_from_terminal(raw, terminal.raw.as_ptr());
            sys::ghostty_key_event_set_action(ev, event.action.raw());
            sys::ghostty_key_event_set_key(ev, event.key as sys::GhosttyKey);
            sys::ghostty_key_event_set_mods(ev, event.mods.0);
            sys::ghostty_key_event_set_consumed_mods(ev, event.consumed.0);
            sys::ghostty_key_event_set_composing(ev, false);
            sys::ghostty_key_event_set_unshifted_codepoint(
                ev,
                event.unshifted.map_or(0, u32::from),
            );
            if event.text.is_empty() {
                sys::ghostty_key_event_set_utf8(ev, ptr::null(), 0);
            } else {
                sys::ghostty_key_event_set_utf8(ev, event.text.as_ptr().cast(), event.text.len());
            }
        }
        let result = encode_into(out, |buf, len, written| {
            // SAFETY: `buf` is valid for `len` bytes and `written` is a valid out-pointer.
            unsafe { sys::ghostty_key_encoder_encode(raw, ev, buf, len, written) }
        });
        // SAFETY: clearing the borrowed text pointer so the event never holds a dangling reference.
        unsafe { sys::ghostty_key_event_set_utf8(ev, ptr::null(), 0) };
        result
    }
}

impl Drop for KeyEncoder {
    fn drop(&mut self) {
        // SAFETY: both handles came from their constructors and are freed exactly once, here.
        unsafe {
            sys::ghostty_key_event_free(self.event.as_ptr());
            sys::ghostty_key_encoder_free(self.raw.as_ptr());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    Press,
    Release,
    Motion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    WheelUp,
    WheelDown,
    WheelLeft,
    WheelRight,
    Back,
    Forward,
}

impl MouseButton {
    fn raw(self) -> sys::GhosttyMouseButton {
        match self {
            MouseButton::Left => sys::GHOSTTY_MOUSE_BUTTON_LEFT,
            MouseButton::Right => sys::GHOSTTY_MOUSE_BUTTON_RIGHT,
            MouseButton::Middle => sys::GHOSTTY_MOUSE_BUTTON_MIDDLE,
            MouseButton::WheelUp => sys::GHOSTTY_MOUSE_BUTTON_FOUR,
            MouseButton::WheelDown => sys::GHOSTTY_MOUSE_BUTTON_FIVE,
            MouseButton::WheelLeft => sys::GHOSTTY_MOUSE_BUTTON_SIX,
            MouseButton::WheelRight => sys::GHOSTTY_MOUSE_BUTTON_SEVEN,
            MouseButton::Back => sys::GHOSTTY_MOUSE_BUTTON_EIGHT,
            MouseButton::Forward => sys::GHOSTTY_MOUSE_BUTTON_NINE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseEvent {
    pub action: MouseAction,
    pub button: Option<MouseButton>,
    pub mods: Mods,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseGeometry {
    pub width: u32,
    pub height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub padding_left: u32,
    pub padding_top: u32,
    pub padding_right: u32,
    pub padding_bottom: u32,
}

pub struct MouseEncoder {
    raw: NonNull<sys::GhosttyMouseEncoderImpl>,
    event: NonNull<sys::GhosttyMouseEventImpl>,
}

impl MouseEncoder {
    pub fn new() -> Result<Self, Error> {
        let mut raw: sys::GhosttyMouseEncoder = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `raw` is a valid out-pointer.
        Error::check(unsafe { sys::ghostty_mouse_encoder_new(ptr::null(), &mut raw) })?;
        let raw = NonNull::new(raw).ok_or(Error::InvalidValue)?;
        let mut event: sys::GhosttyMouseEvent = ptr::null_mut();
        // SAFETY: a null allocator selects the library default; `event` is a valid out-pointer.
        let created =
            Error::check(unsafe { sys::ghostty_mouse_event_new(ptr::null(), &mut event) });
        let Some(event) = created.ok().and_then(|()| NonNull::new(event)) else {
            // SAFETY: `raw` came from ghostty_mouse_encoder_new and is not used again.
            unsafe { sys::ghostty_mouse_encoder_free(raw.as_ptr()) };
            return Err(created.err().unwrap_or(Error::InvalidValue));
        };
        let track = true;
        // SAFETY: the encoder is live and TRACK_LAST_CELL takes a bool.
        unsafe {
            sys::ghostty_mouse_encoder_setopt(
                raw.as_ptr(),
                sys::GHOSTTY_MOUSE_ENCODER_OPT_TRACK_LAST_CELL,
                (&raw const track).cast(),
            );
        }
        Ok(Self { raw, event })
    }

    pub fn encode(
        &mut self,
        terminal: &Terminal,
        event: &MouseEvent,
        geometry: MouseGeometry,
        any_pressed: bool,
        out: &mut Vec<u8>,
    ) -> Result<(), Error> {
        let (raw, ev) = (self.raw.as_ptr(), self.event.as_ptr());
        let size = sys::GhosttyMouseEncoderSize {
            size: size_of::<sys::GhosttyMouseEncoderSize>(),
            screen_width: geometry.width,
            screen_height: geometry.height,
            cell_width: geometry.cell_width.max(1),
            cell_height: geometry.cell_height.max(1),
            padding_top: geometry.padding_top,
            padding_bottom: geometry.padding_bottom,
            padding_right: geometry.padding_right,
            padding_left: geometry.padding_left,
        };
        let action = match event.action {
            MouseAction::Press => sys::GHOSTTY_MOUSE_ACTION_PRESS,
            MouseAction::Release => sys::GHOSTTY_MOUSE_ACTION_RELEASE,
            MouseAction::Motion => sys::GHOSTTY_MOUSE_ACTION_MOTION,
        };
        // SAFETY: the encoder, the event and the terminal are live; each option value points to a local of the type the option expects.
        unsafe {
            sys::ghostty_mouse_encoder_setopt_from_terminal(raw, terminal.raw.as_ptr());
            sys::ghostty_mouse_encoder_setopt(
                raw,
                sys::GHOSTTY_MOUSE_ENCODER_OPT_SIZE,
                (&raw const size).cast(),
            );
            sys::ghostty_mouse_encoder_setopt(
                raw,
                sys::GHOSTTY_MOUSE_ENCODER_OPT_ANY_BUTTON_PRESSED,
                (&raw const any_pressed).cast(),
            );
            sys::ghostty_mouse_event_set_action(ev, action);
            match event.button {
                Some(button) => sys::ghostty_mouse_event_set_button(ev, button.raw()),
                None => sys::ghostty_mouse_event_clear_button(ev),
            }
            sys::ghostty_mouse_event_set_mods(ev, event.mods.0);
            sys::ghostty_mouse_event_set_position(
                ev,
                sys::GhosttyMousePosition {
                    x: event.x,
                    y: event.y,
                },
            );
        }
        encode_into(out, |buf, len, written| {
            // SAFETY: `buf` is valid for `len` bytes and `written` is a valid out-pointer.
            unsafe { sys::ghostty_mouse_encoder_encode(raw, ev, buf, len, written) }
        })
    }
}

impl Drop for MouseEncoder {
    fn drop(&mut self) {
        // SAFETY: both handles came from their constructors and are freed exactly once, here.
        unsafe {
            sys::ghostty_mouse_event_free(self.event.as_ptr());
            sys::ghostty_mouse_encoder_free(self.raw.as_ptr());
        }
    }
}

pub fn encode_focus(gained: bool, out: &mut Vec<u8>) -> Result<(), Error> {
    let event = if gained {
        sys::GHOSTTY_FOCUS_GAINED
    } else {
        sys::GHOSTTY_FOCUS_LOST
    };
    encode_into(out, |buf, len, written| {
        // SAFETY: `buf` is valid for `len` bytes and `written` is a valid out-pointer.
        unsafe { sys::ghostty_focus_encode(event, buf, len, written) }
    })
}

pub fn codepoint_width(c: char) -> u8 {
    // SAFETY: a pure lookup with no pointer arguments.
    unsafe { sys::ghostty_unicode_codepoint_width(u32::from(c)) }
}

fn encode_into(
    out: &mut Vec<u8>,
    mut encode: impl FnMut(*mut core::ffi::c_char, usize, &mut usize) -> sys::GhosttyResult,
) -> Result<(), Error> {
    let mut buf = [0u8; 128];
    let mut written = 0;
    match Error::check(encode(buf.as_mut_ptr().cast(), buf.len(), &mut written)) {
        Ok(()) => {
            out.extend_from_slice(&buf[..written]);
            Ok(())
        }
        Err(Error::OutOfSpace) => {
            let mut big = vec![0u8; written];
            let mut len = 0;
            Error::check(encode(big.as_mut_ptr().cast(), big.len(), &mut len))?;
            out.extend_from_slice(&big[..len]);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

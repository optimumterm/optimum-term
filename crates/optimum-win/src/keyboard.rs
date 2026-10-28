use optimum_core::{Adjust, Key, KeyAction, KeyEvent, Mods};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardLayout, GetKeyboardState, ToUnicodeEx, VIRTUAL_KEY, VK_ADD, VK_CAPITAL,
    VK_CONTROL, VK_DECIMAL, VK_DOWN, VK_END, VK_ESCAPE, VK_F11, VK_HOME, VK_INSERT, VK_LCONTROL,
    VK_LEFT, VK_LMENU, VK_LWIN, VK_MENU, VK_NEXT, VK_NUMLOCK, VK_NUMPAD0, VK_NUMPAD9, VK_OEM_MINUS,
    VK_OEM_PLUS, VK_PRIOR, VK_RCONTROL, VK_RETURN, VK_RIGHT, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_SPACE, VK_SUBTRACT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MSG, PM_REMOVE, PeekMessageW, WM_CHAR, WM_DEADCHAR, WM_SYSCHAR, WM_SYSDEADCHAR,
};

const NO_STATE_CHANGE: u32 = 1 << 2;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Chars {
    pub text: String,
    pub dead: bool,
}

pub(crate) fn take_chars(hwnd: HWND) -> Chars {
    let mut units = Vec::new();
    let mut dead = false;
    let mut msg = MSG::default();
    for (first, last) in [(WM_CHAR, WM_DEADCHAR), (WM_SYSCHAR, WM_SYSDEADCHAR)] {
        // SAFETY: `msg` is a valid out-pointer; only character messages generated for this window are removed.
        while unsafe { PeekMessageW(&mut msg, Some(hwnd), first, last, PM_REMOVE) }.as_bool() {
            match msg.message {
                WM_CHAR | WM_SYSCHAR => units.push(msg.wParam.0 as u16),
                _ => dead = true,
            }
        }
    }
    Chars {
        text: String::from_utf16_lossy(&units),
        dead,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stroke {
    pub vk: u16,
    pub scancode: u32,
    pub key: Key,
    pub action: KeyAction,
}

impl Stroke {
    pub fn new(wparam: WPARAM, lparam: LPARAM, down: bool) -> Stroke {
        let vk = wparam.0 as u16;
        let scancode = ((lparam.0 >> 16) & 0xff) as u32;
        let extended = lparam.0 & (1 << 24) != 0;
        let repeat = lparam.0 & (1 << 30) != 0;
        Stroke {
            vk,
            scancode,
            key: key_from_scancode(scancode, extended, vk),
            action: match (down, repeat) {
                (false, _) => KeyAction::Release,
                (true, true) => KeyAction::Repeat,
                (true, false) => KeyAction::Press,
            },
        }
    }
}

pub(crate) fn mods() -> Mods {
    let state = |vk: VIRTUAL_KEY| {
        // SAFETY: GetKeyState has no preconditions.
        unsafe { GetKeyState(i32::from(vk.0)) }
    };
    let down = |vk: VIRTUAL_KEY| state(vk) < 0;
    let toggled = |vk: VIRTUAL_KEY| state(vk) & 1 != 0;
    let mut mods = Mods::NONE;
    if down(VK_SHIFT) {
        mods |= Mods::SHIFT;
        if down(VK_RSHIFT) {
            mods |= Mods::RIGHT_SHIFT;
        }
    }
    if down(VK_CONTROL) {
        mods |= Mods::CTRL;
        if down(VK_RCONTROL) && !down(VK_LCONTROL) {
            mods |= Mods::RIGHT_CTRL;
        }
    }
    if down(VK_MENU) {
        mods |= Mods::ALT;
        if down(VK_RMENU) && !down(VK_LMENU) {
            mods |= Mods::RIGHT_ALT;
        }
    }
    if down(VK_LWIN) || down(VK_RWIN) {
        mods |= Mods::SUPER;
        if down(VK_RWIN) && !down(VK_LWIN) {
            mods |= Mods::RIGHT_SUPER;
        }
    }
    if toggled(VK_CAPITAL) {
        mods |= Mods::CAPS_LOCK;
    }
    if toggled(VK_NUMLOCK) {
        mods |= Mods::NUM_LOCK;
    }
    mods
}

pub(crate) fn translate(stroke: Stroke, strip_ctrl_alt: bool, unshifted: bool) -> Option<String> {
    let mut state = [0u8; 256];
    if !unshifted {
        // SAFETY: `state` is a valid 256-byte buffer.
        unsafe { GetKeyboardState(&mut state) }.ok()?;
        if strip_ctrl_alt {
            for vk in [
                VK_CONTROL,
                VK_LCONTROL,
                VK_RCONTROL,
                VK_MENU,
                VK_LMENU,
                VK_RMENU,
            ] {
                state[usize::from(vk.0)] = 0;
            }
        }
    }
    let mut buf = [0u16; 8];
    // SAFETY: `state` and `buf` are valid buffers; NO_STATE_CHANGE keeps pending dead keys intact.
    let len = unsafe {
        ToUnicodeEx(
            u32::from(stroke.vk),
            stroke.scancode,
            &state,
            &mut buf,
            NO_STATE_CHANGE,
            Some(GetKeyboardLayout(0)),
        )
    };
    if len <= 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&buf[..len as usize]);
    printable(&text).then_some(text)
}

pub(crate) fn printable(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

pub(crate) fn event<'a>(
    stroke: Stroke,
    mods: Mods,
    typed: &'a str,
    stripped: Option<&'a str>,
    unshifted: Option<char>,
) -> KeyEvent<'a> {
    let ctrl_alt = Mods::CTRL | Mods::ALT;
    let (mods, text) = if printable(typed) {
        let mods = if mods.contains(ctrl_alt) {
            mods.without(ctrl_alt | Mods::RIGHT_CTRL | Mods::RIGHT_ALT)
        } else {
            mods
        };
        (mods, typed)
    } else if mods.intersects(ctrl_alt) {
        (mods, stripped.unwrap_or(""))
    } else {
        (mods, "")
    };
    let consumed = if !text.is_empty() && mods.contains(Mods::SHIFT) {
        Mods::SHIFT
    } else {
        Mods::NONE
    };
    KeyEvent {
        action: stroke.action,
        key: stroke.key,
        mods,
        consumed,
        text,
        unshifted,
    }
}

pub(crate) fn is_settings(key: Key, mods: Mods) -> bool {
    key == Key::Comma && mods.binding() == Mods::CTRL
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Copy,
    CopyOrInterrupt,
    Paste,
    SelectAll,
    Escape,
    Extend(Adjust),
    ScrollPage(isize),
    ScrollLine(isize),
    ScrollTop,
    ScrollBottom,
    SystemMenu,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
}

pub(crate) fn command(vk: u16, mods: Mods) -> Option<Command> {
    const KEY_A: u16 = b'A' as u16;
    const KEY_C: u16 = b'C' as u16;
    const KEY_V: u16 = b'V' as u16;
    const KEY_0: u16 = b'0' as u16;
    let ctrl = Mods::CTRL;
    let shift = Mods::SHIFT;
    let ctrl_shift = Mods::CTRL | Mods::SHIFT;
    let mods = mods.binding();
    let command = match vk {
        KEY_C if mods == ctrl => Command::CopyOrInterrupt,
        KEY_C if mods == ctrl_shift => Command::Copy,
        KEY_V if mods == ctrl || mods == ctrl_shift => Command::Paste,
        KEY_A if mods == ctrl_shift => Command::SelectAll,
        _ if vk == VK_INSERT.0 && mods == ctrl => Command::Copy,
        _ if vk == VK_INSERT.0 && mods == shift => Command::Paste,
        _ if vk == VK_ESCAPE.0 && mods == Mods::default() => Command::Escape,
        _ if vk == VK_SPACE.0 && mods == Mods::ALT => Command::SystemMenu,
        _ if vk == VK_OEM_PLUS.0 && (mods == ctrl || mods == ctrl_shift) => Command::ZoomIn,
        _ if vk == VK_ADD.0 && mods == ctrl => Command::ZoomIn,
        _ if (vk == VK_OEM_MINUS.0 || vk == VK_SUBTRACT.0) && mods == ctrl => Command::ZoomOut,
        _ if (vk == KEY_0 || vk == VK_NUMPAD0.0) && mods == ctrl => Command::ZoomReset,
        _ if vk == VK_F11.0 && mods == Mods::default() => Command::Fullscreen,
        _ if vk == VK_RETURN.0 && mods == Mods::ALT => Command::Fullscreen,
        _ if vk == VK_PRIOR.0 && mods == shift => Command::ScrollPage(-1),
        _ if vk == VK_NEXT.0 && mods == shift => Command::ScrollPage(1),
        _ if vk == VK_UP.0 && mods == ctrl_shift => Command::ScrollLine(-1),
        _ if vk == VK_DOWN.0 && mods == ctrl_shift => Command::ScrollLine(1),
        _ if vk == VK_HOME.0 && mods == ctrl_shift => Command::ScrollTop,
        _ if vk == VK_END.0 && mods == ctrl_shift => Command::ScrollBottom,
        _ if mods == shift => Command::Extend(match vk {
            _ if vk == VK_LEFT.0 => Adjust::Left,
            _ if vk == VK_RIGHT.0 => Adjust::Right,
            _ if vk == VK_UP.0 => Adjust::Up,
            _ if vk == VK_DOWN.0 => Adjust::Down,
            _ if vk == VK_HOME.0 => Adjust::LineStart,
            _ if vk == VK_END.0 => Adjust::LineEnd,
            _ => return None,
        }),
        _ => return None,
    };
    Some(command)
}

pub(crate) fn key_from_scancode(scancode: u32, extended: bool, vk: u16) -> Key {
    let numpad_digit = (VK_NUMPAD0.0..=VK_NUMPAD9.0).contains(&vk) || vk == VK_DECIMAL.0;
    if extended {
        return match scancode {
            0x1c => Key::NumpadEnter,
            0x1d => Key::ControlRight,
            0x35 => Key::NumpadDivide,
            0x37 => Key::PrintScreen,
            0x38 => Key::AltRight,
            0x45 => Key::NumLock,
            0x46 => Key::Pause,
            0x47 => Key::Home,
            0x48 => Key::ArrowUp,
            0x49 => Key::PageUp,
            0x4b => Key::ArrowLeft,
            0x4d => Key::ArrowRight,
            0x4f => Key::End,
            0x50 => Key::ArrowDown,
            0x51 => Key::PageDown,
            0x52 => Key::Insert,
            0x53 => Key::Delete,
            0x5b => Key::MetaLeft,
            0x5c => Key::MetaRight,
            0x5d => Key::ContextMenu,
            _ => Key::Unidentified,
        };
    }
    match scancode {
        0x01 => Key::Escape,
        0x02 => Key::Digit1,
        0x03 => Key::Digit2,
        0x04 => Key::Digit3,
        0x05 => Key::Digit4,
        0x06 => Key::Digit5,
        0x07 => Key::Digit6,
        0x08 => Key::Digit7,
        0x09 => Key::Digit8,
        0x0a => Key::Digit9,
        0x0b => Key::Digit0,
        0x0c => Key::Minus,
        0x0d => Key::Equal,
        0x0e => Key::Backspace,
        0x0f => Key::Tab,
        0x10 => Key::Q,
        0x11 => Key::W,
        0x12 => Key::E,
        0x13 => Key::R,
        0x14 => Key::T,
        0x15 => Key::Y,
        0x16 => Key::U,
        0x17 => Key::I,
        0x18 => Key::O,
        0x19 => Key::P,
        0x1a => Key::BracketLeft,
        0x1b => Key::BracketRight,
        0x1c => Key::Enter,
        0x1d => Key::ControlLeft,
        0x1e => Key::A,
        0x1f => Key::S,
        0x20 => Key::D,
        0x21 => Key::F,
        0x22 => Key::G,
        0x23 => Key::H,
        0x24 => Key::J,
        0x25 => Key::K,
        0x26 => Key::L,
        0x27 => Key::Semicolon,
        0x28 => Key::Quote,
        0x29 => Key::Backquote,
        0x2a => Key::ShiftLeft,
        0x2b => Key::Backslash,
        0x2c => Key::Z,
        0x2d => Key::X,
        0x2e => Key::C,
        0x2f => Key::V,
        0x30 => Key::B,
        0x31 => Key::N,
        0x32 => Key::M,
        0x33 => Key::Comma,
        0x34 => Key::Period,
        0x35 => Key::Slash,
        0x36 => Key::ShiftRight,
        0x37 => Key::NumpadMultiply,
        0x38 => Key::AltLeft,
        0x39 => Key::Space,
        0x3a => Key::CapsLock,
        0x3b => Key::F1,
        0x3c => Key::F2,
        0x3d => Key::F3,
        0x3e => Key::F4,
        0x3f => Key::F5,
        0x40 => Key::F6,
        0x41 => Key::F7,
        0x42 => Key::F8,
        0x43 => Key::F9,
        0x44 => Key::F10,
        0x45 => Key::Pause,
        0x46 => Key::ScrollLock,
        0x47 if numpad_digit => Key::Numpad7,
        0x47 => Key::NumpadHome,
        0x48 if numpad_digit => Key::Numpad8,
        0x48 => Key::NumpadUp,
        0x49 if numpad_digit => Key::Numpad9,
        0x49 => Key::NumpadPageUp,
        0x4a => Key::NumpadSubtract,
        0x4b if numpad_digit => Key::Numpad4,
        0x4b => Key::NumpadLeft,
        0x4c if numpad_digit => Key::Numpad5,
        0x4c => Key::NumpadBegin,
        0x4d if numpad_digit => Key::Numpad6,
        0x4d => Key::NumpadRight,
        0x4e => Key::NumpadAdd,
        0x4f if numpad_digit => Key::Numpad1,
        0x4f => Key::NumpadEnd,
        0x50 if numpad_digit => Key::Numpad2,
        0x50 => Key::NumpadDown,
        0x51 if numpad_digit => Key::Numpad3,
        0x51 => Key::NumpadPageDown,
        0x52 if numpad_digit => Key::Numpad0,
        0x52 => Key::NumpadInsert,
        0x53 if numpad_digit => Key::NumpadDecimal,
        0x53 => Key::NumpadDelete,
        0x56 => Key::IntlBackslash,
        0x57 => Key::F11,
        0x58 => Key::F12,
        0x59 => Key::NumpadEqual,
        0x64 => Key::F13,
        0x65 => Key::F14,
        0x66 => Key::F15,
        0x67 => Key::F16,
        0x68 => Key::F17,
        0x69 => Key::F18,
        0x6a => Key::F19,
        0x6b => Key::F20,
        0x6c => Key::F21,
        0x6d => Key::F22,
        0x6e => Key::F23,
        0x70 => Key::KanaMode,
        0x73 => Key::IntlRo,
        0x76 => Key::F24,
        0x79 => Key::Convert,
        0x7b => Key::NonConvert,
        0x7d => Key::IntlYen,
        0x7e => Key::NumpadComma,
        _ => Key::Unidentified,
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_NUMPAD7;

    use super::*;

    fn stroke(key: Key) -> Stroke {
        Stroke {
            vk: 0,
            scancode: 0,
            key,
            action: KeyAction::Press,
        }
    }

    #[test]
    fn scancodes_map_to_physical_keys() {
        assert_eq!(key_from_scancode(0x1e, false, 0), Key::A);
        assert_eq!(key_from_scancode(0x33, false, 0), Key::Comma);
        assert_eq!(key_from_scancode(0x1c, false, 0), Key::Enter);
        assert_eq!(key_from_scancode(0x1c, true, 0), Key::NumpadEnter);
        assert_eq!(key_from_scancode(0x48, true, 0), Key::ArrowUp);
        assert_eq!(key_from_scancode(0x38, true, 0), Key::AltRight);
        assert_eq!(key_from_scancode(0x47, false, VK_NUMPAD7.0), Key::Numpad7);
        assert_eq!(key_from_scancode(0x47, false, VK_HOME.0), Key::NumpadHome);
    }

    #[test]
    fn lparam_flags_select_action_and_extended_keys() {
        let up = LPARAM((0x48 << 16) | (1 << 24));
        assert_eq!(Stroke::new(WPARAM(0), up, true).key, Key::ArrowUp);
        assert_eq!(
            Stroke::new(WPARAM(0), LPARAM(up.0 | 1 << 30), true).action,
            KeyAction::Repeat
        );
        assert_eq!(Stroke::new(WPARAM(0), up, false).action, KeyAction::Release);
    }

    #[test]
    fn altgr_text_drops_ctrl_and_alt() {
        let mods = Mods::CTRL | Mods::ALT | Mods::RIGHT_ALT;
        let e = event(stroke(Key::Q), mods, "@", None, Some('q'));
        assert_eq!((e.text, e.mods), ("@", Mods::NONE));
    }

    #[test]
    fn ctrl_uses_stripped_text_not_control_chars() {
        let e = event(stroke(Key::C), Mods::CTRL, "\x03", Some("с"), Some('с'));
        assert_eq!((e.text, e.mods), ("с", Mods::CTRL));
    }

    #[test]
    fn shifted_text_consumes_shift() {
        let e = event(stroke(Key::A), Mods::SHIFT, "A", None, Some('a'));
        assert_eq!((e.text, e.consumed), ("A", Mods::SHIFT));
        let e = event(stroke(Key::Enter), Mods::SHIFT, "\r", None, None);
        assert_eq!((e.text, e.consumed), ("", Mods::NONE));
    }

    #[test]
    fn only_ctrl_comma_opens_settings() {
        assert!(is_settings(Key::Comma, Mods::CTRL));
        assert!(is_settings(Key::Comma, Mods::CTRL | Mods::NUM_LOCK));
        assert!(!is_settings(Key::Comma, Mods::CTRL | Mods::SHIFT));
        assert!(!is_settings(Key::Comma, Mods::CTRL | Mods::ALT));
        assert!(!is_settings(Key::Period, Mods::CTRL));
    }

    #[test]
    fn standard_shortcuts_map_to_commands() {
        let ctrl = Mods::CTRL;
        let shift = Mods::SHIFT;
        let ctrl_shift = Mods::CTRL | Mods::SHIFT;
        assert_eq!(command(b'C' as u16, ctrl), Some(Command::CopyOrInterrupt));
        assert_eq!(
            command(b'C' as u16, ctrl | Mods::NUM_LOCK),
            Some(Command::CopyOrInterrupt)
        );
        assert_eq!(command(b'C' as u16, ctrl_shift), Some(Command::Copy));
        assert_eq!(command(VK_INSERT.0, ctrl), Some(Command::Copy));
        assert_eq!(command(b'V' as u16, ctrl), Some(Command::Paste));
        assert_eq!(command(b'V' as u16, ctrl_shift), Some(Command::Paste));
        assert_eq!(command(VK_INSERT.0, shift), Some(Command::Paste));
        assert_eq!(command(b'A' as u16, ctrl_shift), Some(Command::SelectAll));
        assert_eq!(command(VK_SPACE.0, Mods::ALT), Some(Command::SystemMenu));
        assert_eq!(command(VK_PRIOR.0, shift), Some(Command::ScrollPage(-1)));
        assert_eq!(command(VK_END.0, ctrl_shift), Some(Command::ScrollBottom));
        assert_eq!(
            command(VK_LEFT.0, shift),
            Some(Command::Extend(Adjust::Left))
        );
    }

    #[test]
    fn shell_keys_are_left_alone() {
        assert_eq!(command(b'A' as u16, Mods::CTRL), None);
        assert_eq!(command(b'C' as u16, Mods::CTRL | Mods::ALT), None);
        assert_eq!(command(b'V' as u16, Mods::default()), None);
        assert_eq!(command(VK_PRIOR.0, Mods::default()), None);
        assert_eq!(command(VK_LEFT.0, Mods::CTRL), None);
        assert_eq!(command(b'Z' as u16, Mods::SHIFT), None);
    }
}

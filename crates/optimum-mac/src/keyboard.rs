use optimum_core::{Adjust, Key, KeyAction, KeyEvent, Mods};

const CAPS_LOCK: usize = 1 << 16;
const SHIFT: usize = 1 << 17;
const CONTROL: usize = 1 << 18;
const OPTION: usize = 1 << 19;
const COMMAND: usize = 1 << 20;
const DEVICE_RIGHT_SHIFT: usize = 0x04;
const DEVICE_RIGHT_CONTROL: usize = 0x2000;
const DEVICE_RIGHT_OPTION: usize = 0x40;
const DEVICE_RIGHT_COMMAND: usize = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Command {
    Copy,
    Paste,
    SelectAll,
    Escape,
    Extend(Adjust),
    ScrollPage(isize),
    ScrollLine(isize),
    ScrollTop,
    ScrollBottom,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Fullscreen,
    Settings,
    Close,
    Minimize,
}

pub(crate) fn mods(flags: usize) -> Mods {
    let mut mods = Mods::NONE;
    for (bit, side, mod_, right) in [
        (SHIFT, DEVICE_RIGHT_SHIFT, Mods::SHIFT, Mods::RIGHT_SHIFT),
        (CONTROL, DEVICE_RIGHT_CONTROL, Mods::CTRL, Mods::RIGHT_CTRL),
        (OPTION, DEVICE_RIGHT_OPTION, Mods::ALT, Mods::RIGHT_ALT),
        (
            COMMAND,
            DEVICE_RIGHT_COMMAND,
            Mods::SUPER,
            Mods::RIGHT_SUPER,
        ),
    ] {
        if flags & bit != 0 {
            mods |= mod_;
            if flags & side != 0 {
                mods |= right;
            }
        }
    }
    if flags & CAPS_LOCK != 0 {
        mods |= Mods::CAPS_LOCK;
    }
    mods
}

pub(crate) fn printable(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(|c| c.is_control() || is_function_key(c))
}

fn is_function_key(c: char) -> bool {
    ('\u{F700}'..='\u{F8FF}').contains(&c)
}

pub(crate) fn event<'a>(
    key: Key,
    action: KeyAction,
    mods: Mods,
    typed: &'a str,
    ignoring: &'a str,
    unshifted: Option<char>,
) -> KeyEvent<'a> {
    let text = if printable(typed) {
        typed
    } else if mods.intersects(Mods::CTRL) && printable(ignoring) {
        ignoring
    } else {
        ""
    };
    let mut consumed = Mods::NONE;
    if !text.is_empty() {
        if mods.contains(Mods::SHIFT) {
            consumed |= Mods::SHIFT;
        }
        if mods.contains(Mods::ALT) && text == typed {
            consumed |= Mods::ALT;
        }
    }
    KeyEvent {
        action,
        key,
        mods,
        consumed,
        text,
        unshifted,
    }
}

pub(crate) fn command(code: u16, mods: Mods) -> Option<Command> {
    let key = key(code);
    let binding = mods.binding();
    let cmd = Mods::SUPER;
    let cmd_shift = Mods::SUPER | Mods::SHIFT;
    let command = match (binding, key) {
        (b, Key::Comma) if b == cmd => Command::Settings,
        (b, Key::C) if b == cmd => Command::Copy,
        (b, Key::V) if b == cmd => Command::Paste,
        (b, Key::A) if b == cmd => Command::SelectAll,
        (b, Key::Equal) if b == cmd || b == cmd_shift => Command::ZoomIn,
        (b, Key::NumpadAdd) if b == cmd => Command::ZoomIn,
        (b, Key::Minus | Key::NumpadSubtract) if b == cmd => Command::ZoomOut,
        (b, Key::Digit0 | Key::Numpad0) if b == cmd => Command::ZoomReset,
        (b, Key::F) if b == Mods::SUPER | Mods::CTRL => Command::Fullscreen,
        (b, Key::W) if b == cmd => Command::Close,
        (b, Key::M) if b == cmd => Command::Minimize,
        (b, Key::Escape) if b == Mods::NONE => Command::Escape,
        (b, Key::PageUp) if b == Mods::SHIFT || b == cmd => Command::ScrollPage(-1),
        (b, Key::PageDown) if b == Mods::SHIFT || b == cmd => Command::ScrollPage(1),
        (b, Key::ArrowUp) if b == cmd => Command::ScrollLine(-1),
        (b, Key::ArrowDown) if b == cmd => Command::ScrollLine(1),
        (b, Key::Home) if b == cmd => Command::ScrollTop,
        (b, Key::End) if b == cmd => Command::ScrollBottom,
        (b, Key::ArrowLeft) if b == Mods::SHIFT => Command::Extend(Adjust::Left),
        (b, Key::ArrowRight) if b == Mods::SHIFT => Command::Extend(Adjust::Right),
        (b, Key::ArrowUp) if b == Mods::SHIFT => Command::Extend(Adjust::Up),
        (b, Key::ArrowDown) if b == Mods::SHIFT => Command::Extend(Adjust::Down),
        (b, Key::ArrowLeft) if b == cmd_shift => Command::Extend(Adjust::LineStart),
        (b, Key::ArrowRight) if b == cmd_shift => Command::Extend(Adjust::LineEnd),
        _ => return None,
    };
    Some(command)
}

pub(crate) fn key(code: u16) -> Key {
    match code {
        0x00 => Key::A,
        0x01 => Key::S,
        0x02 => Key::D,
        0x03 => Key::F,
        0x04 => Key::H,
        0x05 => Key::G,
        0x06 => Key::Z,
        0x07 => Key::X,
        0x08 => Key::C,
        0x09 => Key::V,
        0x0A => Key::IntlBackslash,
        0x0B => Key::B,
        0x0C => Key::Q,
        0x0D => Key::W,
        0x0E => Key::E,
        0x0F => Key::R,
        0x10 => Key::Y,
        0x11 => Key::T,
        0x12 => Key::Digit1,
        0x13 => Key::Digit2,
        0x14 => Key::Digit3,
        0x15 => Key::Digit4,
        0x16 => Key::Digit6,
        0x17 => Key::Digit5,
        0x18 => Key::Equal,
        0x19 => Key::Digit9,
        0x1A => Key::Digit7,
        0x1B => Key::Minus,
        0x1C => Key::Digit8,
        0x1D => Key::Digit0,
        0x1E => Key::BracketRight,
        0x1F => Key::O,
        0x20 => Key::U,
        0x21 => Key::BracketLeft,
        0x22 => Key::I,
        0x23 => Key::P,
        0x24 => Key::Enter,
        0x25 => Key::L,
        0x26 => Key::J,
        0x27 => Key::Quote,
        0x28 => Key::K,
        0x29 => Key::Semicolon,
        0x2A => Key::Backslash,
        0x2B => Key::Comma,
        0x2C => Key::Slash,
        0x2D => Key::N,
        0x2E => Key::M,
        0x2F => Key::Period,
        0x30 => Key::Tab,
        0x31 => Key::Space,
        0x32 => Key::Backquote,
        0x33 => Key::Backspace,
        0x35 => Key::Escape,
        0x36 => Key::MetaRight,
        0x37 => Key::MetaLeft,
        0x38 => Key::ShiftLeft,
        0x39 => Key::CapsLock,
        0x3A => Key::AltLeft,
        0x3B => Key::ControlLeft,
        0x3C => Key::ShiftRight,
        0x3D => Key::AltRight,
        0x3E => Key::ControlRight,
        0x40 => Key::F17,
        0x41 => Key::NumpadDecimal,
        0x43 => Key::NumpadMultiply,
        0x45 => Key::NumpadAdd,
        0x47 => Key::NumLock,
        0x4B => Key::NumpadDivide,
        0x4C => Key::NumpadEnter,
        0x4E => Key::NumpadSubtract,
        0x4F => Key::F18,
        0x50 => Key::F19,
        0x51 => Key::NumpadEqual,
        0x52 => Key::Numpad0,
        0x53 => Key::Numpad1,
        0x54 => Key::Numpad2,
        0x55 => Key::Numpad3,
        0x56 => Key::Numpad4,
        0x57 => Key::Numpad5,
        0x58 => Key::Numpad6,
        0x59 => Key::Numpad7,
        0x5A => Key::F20,
        0x5B => Key::Numpad8,
        0x5C => Key::Numpad9,
        0x5D => Key::IntlYen,
        0x5E => Key::IntlRo,
        0x5F => Key::NumpadComma,
        0x60 => Key::F5,
        0x61 => Key::F6,
        0x62 => Key::F7,
        0x63 => Key::F3,
        0x64 => Key::F8,
        0x65 => Key::F9,
        0x66 => Key::NonConvert,
        0x67 => Key::F11,
        0x68 => Key::KanaMode,
        0x69 => Key::F13,
        0x6A => Key::F16,
        0x6B => Key::F14,
        0x6D => Key::F10,
        0x6F => Key::F12,
        0x71 => Key::F15,
        0x72 => Key::Insert,
        0x73 => Key::Home,
        0x74 => Key::PageUp,
        0x75 => Key::Delete,
        0x76 => Key::F4,
        0x77 => Key::End,
        0x78 => Key::F2,
        0x79 => Key::PageDown,
        0x7A => Key::F1,
        0x7B => Key::ArrowLeft,
        0x7C => Key::ArrowRight,
        0x7D => Key::ArrowDown,
        0x7E => Key::ArrowUp,
        _ => Key::Unidentified,
    }
}

pub(crate) fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_flags_map_to_mods() {
        assert_eq!(mods(SHIFT | COMMAND), Mods::SHIFT | Mods::SUPER);
        assert_eq!(
            mods(OPTION | DEVICE_RIGHT_OPTION),
            Mods::ALT | Mods::RIGHT_ALT
        );
        assert_eq!(mods(0), Mods::NONE);
    }

    #[test]
    fn physical_keys_follow_ansi_codes() {
        assert_eq!(key(0x00), Key::A);
        assert_eq!(key(0x24), Key::Enter);
        assert_eq!(key(0x7E), Key::ArrowUp);
        assert_eq!(key(0x33), Key::Backspace);
        assert_eq!(key(0x75), Key::Delete);
    }

    #[test]
    fn standard_shortcuts_map_to_commands() {
        assert_eq!(command(0x08, Mods::SUPER), Some(Command::Copy));
        assert_eq!(command(0x09, Mods::SUPER), Some(Command::Paste));
        assert_eq!(command(0x00, Mods::SUPER), Some(Command::SelectAll));
        assert_eq!(command(0x2B, Mods::SUPER), Some(Command::Settings));
        assert_eq!(command(0x18, Mods::SUPER), Some(Command::ZoomIn));
        assert_eq!(command(0x1B, Mods::SUPER), Some(Command::ZoomOut));
        assert_eq!(command(0x1D, Mods::SUPER), Some(Command::ZoomReset));
        assert_eq!(
            command(0x03, Mods::SUPER | Mods::CTRL),
            Some(Command::Fullscreen)
        );
        assert_eq!(
            command(0x7B, Mods::SHIFT),
            Some(Command::Extend(Adjust::Left))
        );
        assert_eq!(command(0x74, Mods::SHIFT), Some(Command::ScrollPage(-1)));
    }

    #[test]
    fn shell_keys_are_left_alone() {
        assert_eq!(command(0x08, Mods::CTRL), None);
        assert_eq!(command(0x00, Mods::CTRL), None);
        assert_eq!(command(0x7B, Mods::NONE), None);
        assert_eq!(command(0x35, Mods::SHIFT), None);
    }

    #[test]
    fn control_letters_send_the_plain_letter() {
        let e = event(
            Key::C,
            KeyAction::Press,
            Mods::CTRL,
            "\u{3}",
            "c",
            Some('c'),
        );
        assert_eq!((e.text, e.consumed), ("c", Mods::NONE));
    }

    #[test]
    fn option_text_consumes_option() {
        let e = event(Key::E, KeyAction::Press, Mods::ALT, "é", "e", Some('e'));
        assert_eq!((e.text, e.consumed), ("é", Mods::ALT));
    }

    #[test]
    fn single_char_rejects_clusters() {
        assert_eq!(single_char("a"), Some('a'));
        assert_eq!(single_char("ab"), None);
        assert_eq!(single_char(""), None);
    }

    #[test]
    fn arrows_carry_no_text() {
        let e = event(
            Key::ArrowUp,
            KeyAction::Press,
            Mods::NONE,
            "\u{F700}",
            "\u{F700}",
            None,
        );
        assert_eq!(e.text, "");
    }
}

use ghostty_vt::{
    Key, KeyAction, KeyEncoder, KeyEvent, Mode, Mods, MouseAction, MouseButton, MouseEncoder,
    MouseEvent, MouseGeometry, Terminal, codepoint_width, encode_focus,
};

fn encode(term: &Terminal, event: KeyEvent<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    KeyEncoder::new()
        .unwrap()
        .encode(term, &event, &mut out)
        .unwrap();
    out
}

fn text<'a>(key: Key, mods: Mods, text: &'a str, unshifted: char) -> KeyEvent<'a> {
    KeyEvent {
        text,
        unshifted: Some(unshifted),
        consumed: if mods.contains(Mods::SHIFT) {
            Mods::SHIFT
        } else {
            Mods::NONE
        },
        ..KeyEvent::press(key, mods)
    }
}

#[test]
fn printable_text_is_sent_as_is() {
    let term = Terminal::new(80, 24).unwrap();
    assert_eq!(encode(&term, text(Key::A, Mods::NONE, "a", 'a')), b"a");
    assert_eq!(encode(&term, text(Key::A, Mods::SHIFT, "A", 'a')), b"A");
    assert_eq!(
        encode(&term, text(Key::Q, Mods::NONE, "й", 'й')),
        "й".as_bytes()
    );
}

#[test]
fn control_keys_use_legacy_sequences() {
    let term = Terminal::new(80, 24).unwrap();
    assert_eq!(
        encode(&term, KeyEvent::press(Key::Enter, Mods::NONE)),
        b"\r"
    );
    assert_eq!(
        encode(&term, KeyEvent::press(Key::Backspace, Mods::NONE)),
        b"\x7f"
    );
    assert_eq!(
        encode(&term, KeyEvent::press(Key::ArrowUp, Mods::NONE)),
        b"\x1b[A"
    );
    assert_eq!(
        encode(&term, KeyEvent::press(Key::F5, Mods::NONE)),
        b"\x1b[15~"
    );
    assert_eq!(
        encode(&term, KeyEvent::press(Key::ArrowLeft, Mods::CTRL)),
        b"\x1b[1;5D"
    );
}

#[test]
fn ctrl_letters_become_c0_even_on_cyrillic_layouts() {
    let term = Terminal::new(80, 24).unwrap();
    assert_eq!(encode(&term, text(Key::C, Mods::CTRL, "c", 'c')), b"\x03");
    assert_eq!(encode(&term, text(Key::C, Mods::CTRL, "с", 'с')), b"\x03");
}

#[test]
#[cfg(not(target_os = "macos"))]
fn alt_prefixes_escape() {
    let term = Terminal::new(80, 24).unwrap();
    assert_eq!(encode(&term, text(Key::X, Mods::ALT, "x", 'x')), b"\x1bx");
}

#[test]
#[cfg(target_os = "macos")]
fn option_sends_its_text() {
    let term = Terminal::new(80, 24).unwrap();
    assert_eq!(
        encode(&term, text(Key::X, Mods::ALT, "≈", 'x')),
        "≈".as_bytes()
    );
}

#[test]
fn cursor_keys_follow_application_mode() {
    let mut term = Terminal::new(80, 24).unwrap();
    term.write(b"\x1b[?1h");
    assert_eq!(
        encode(&term, KeyEvent::press(Key::ArrowUp, Mods::NONE)),
        b"\x1bOA"
    );
}

#[test]
fn kitty_protocol_reports_releases_when_enabled() {
    let mut term = Terminal::new(80, 24).unwrap();
    let release = KeyEvent {
        action: KeyAction::Release,
        ..text(Key::A, Mods::NONE, "", 'a')
    };
    assert!(encode(&term, release).is_empty());

    term.write(b"\x1b[>3u");
    assert_eq!(encode(&term, release), b"\x1b[97;1:3u");
    assert_eq!(
        encode(&term, KeyEvent::press(Key::Escape, Mods::NONE)),
        b"\x1b[27u"
    );
}

#[test]
fn mouse_reports_only_when_tracking() {
    let mut term = Terminal::new(80, 24).unwrap();
    let geometry = MouseGeometry {
        width: 800,
        height: 480,
        cell_width: 10,
        cell_height: 20,
        ..Default::default()
    };
    let press = MouseEvent {
        action: MouseAction::Press,
        button: Some(MouseButton::Left),
        mods: Mods::NONE,
        x: 25.0,
        y: 45.0,
    };
    let mut encoder = MouseEncoder::new().unwrap();
    let mut out = Vec::new();

    assert!(!term.mouse_tracking());
    encoder
        .encode(&term, &press, geometry, true, &mut out)
        .unwrap();
    assert!(out.is_empty());

    term.write(b"\x1b[?1000h\x1b[?1006h");
    assert!(term.mouse_tracking());
    encoder
        .encode(&term, &press, geometry, true, &mut out)
        .unwrap();
    assert_eq!(out, b"\x1b[<0;3;3M");

    out.clear();
    let wheel = MouseEvent {
        button: Some(MouseButton::WheelUp),
        ..press
    };
    encoder
        .encode(&term, &wheel, geometry, false, &mut out)
        .unwrap();
    assert_eq!(out, b"\x1b[<64;3;3M");
}

#[test]
fn focus_events_follow_mode_1004() {
    let mut term = Terminal::new(80, 24).unwrap();
    assert!(!term.mode(Mode::FocusEvent));
    term.write(b"\x1b[?1004h");
    assert!(term.mode(Mode::FocusEvent));

    let mut out = Vec::new();
    encode_focus(true, &mut out).unwrap();
    encode_focus(false, &mut out).unwrap();
    assert_eq!(out, b"\x1b[I\x1b[O");
}

#[test]
fn codepoint_widths() {
    assert_eq!(codepoint_width('a'), 1);
    assert_eq!(codepoint_width('日'), 2);
}

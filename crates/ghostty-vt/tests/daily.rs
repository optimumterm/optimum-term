use ghostty_vt::{
    Adjust, ClickTiming, Mode, Pointer, RenderState, Scroll, SelectionGeometry, Terminal,
    encode_paste,
};

const TIMING: ClickTiming = ClickTiming {
    time_ns: 1_000_000_000,
    interval_ns: 500_000_000,
    distance: 4.0,
};

const GEOMETRY: SelectionGeometry = SelectionGeometry {
    columns: 20,
    cell_width: 10,
    padding_left: 0,
    screen_height: 80,
};

fn at(col: u16, row: u16) -> Pointer {
    Pointer {
        col,
        row,
        x: f64::from(col) * 10.0 + 2.0,
        y: f64::from(row) * 20.0 + 5.0,
    }
}

#[test]
fn drag_selects_text() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"hello world\r\nsecond line");
    term.select_press(at(0, 0), TIMING).unwrap();
    let past_middle = Pointer {
        x: 47.0,
        ..at(4, 0)
    };
    term.select_drag(past_middle, GEOMETRY).unwrap();
    term.select_release(at(4, 0)).unwrap();
    assert_eq!(term.selection_text().as_deref(), Some("hello"));
    term.clear_selection();
    assert!(!term.has_selection());
}

#[test]
fn double_click_selects_word_and_triple_click_selects_line() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"hello world\r\nsecond line");
    let mut timing = TIMING;
    for _ in 0..2 {
        term.select_press(at(7, 0), timing).unwrap();
        term.select_release(at(7, 0)).unwrap();
        timing.time_ns += 100_000_000;
    }
    assert_eq!(term.selection_text().as_deref(), Some("world"));
    term.select_press(at(7, 0), timing).unwrap();
    term.select_release(at(7, 0)).unwrap();
    assert_eq!(term.selection_text().as_deref(), Some("hello world"));
}

#[test]
fn select_all_takes_every_line() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"hello world\r\nsecond line");
    term.select_all().unwrap();
    assert_eq!(
        term.selection_text().as_deref(),
        Some("hello world\nsecond line")
    );
}

#[test]
fn keyboard_adjusts_an_existing_selection_only() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"hello world\r\nsecond line");
    assert!(!term.adjust_selection(Adjust::Right).unwrap());
    let mut timing = TIMING;
    for _ in 0..2 {
        term.select_press(at(0, 0), timing).unwrap();
        term.select_release(at(0, 0)).unwrap();
        timing.time_ns += 100_000_000;
    }
    assert_eq!(term.selection_text().as_deref(), Some("hello"));
    assert!(term.adjust_selection(Adjust::LineEnd).unwrap());
    assert_eq!(term.selection_text().as_deref(), Some("hello world"));
}

#[test]
fn shift_click_extends_an_existing_selection() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"hello world\r\nsecond line");
    assert!(!term.extend_selection(at(3, 1)).unwrap());
    term.select_press(at(0, 0), TIMING).unwrap();
    let past_middle = Pointer {
        x: 47.0,
        ..at(4, 0)
    };
    term.select_drag(past_middle, GEOMETRY).unwrap();
    term.select_release(at(4, 0)).unwrap();
    assert!(term.extend_selection(at(5, 1)).unwrap());
    assert_eq!(
        term.selection_text().as_deref(),
        Some("hello world\nsecond")
    );
}

#[test]
fn osc52_writes_reach_the_clipboard() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"\x1b]52;c;aGVsbG8=\x07");
    assert_eq!(term.take_clipboard().as_deref(), Some("hello"));
    assert_eq!(term.take_clipboard(), None);
}

#[test]
fn paste_is_bracketed_and_sanitized() {
    assert_eq!(encode_paste("a\nb", false).unwrap(), b"a\rb");
    assert_eq!(
        encode_paste("a\x1bb", true).unwrap(),
        b"\x1b[200~a b\x1b[201~"
    );
    let mut term = Terminal::new(20, 4).unwrap();
    term.write(b"\x1b[?2004h");
    assert!(term.mode(Mode::BracketedPaste));
}

#[test]
fn viewport_scrolls_through_history() {
    let mut term = Terminal::new(10, 3).unwrap();
    for i in 0..10 {
        term.write(format!("line{i}\r\n").as_bytes());
    }
    term.scroll(Scroll::Top);
    assert!(term.plain_text().unwrap().contains("line9"));
    let mut state = RenderState::new().unwrap();
    state.update(&term).unwrap();
    let mut rows = state.dirty_rows().unwrap();
    let mut first = String::new();
    if let Some(mut cells) = rows.next_row().map(|row| row.cells) {
        let mut graphemes = Vec::new();
        while cells.advance() {
            cells.graphemes(&mut graphemes);
            first.extend(graphemes.iter().filter_map(|&c| char::from_u32(c)));
        }
    }
    assert!(first.starts_with("line0"), "{first:?}");
    term.scroll(Scroll::Bottom);
}

#[test]
fn alternate_screen_is_reported() {
    let mut term = Terminal::new(10, 3).unwrap();
    assert!(!term.alternate_screen());
    term.write(b"\x1b[?1049h");
    assert!(term.alternate_screen());
}

#[test]
fn render_hold_captures_the_frame_before_the_sync_block() {
    let mut term = Terminal::new(10, 3).unwrap();
    let mut state = RenderState::new().unwrap();
    term.write_capturing(b"before\x1b[?2026h\x1b[2J\x1b[Hafter", &mut state);
    assert!(term.held());
    let mut text = String::new();
    {
        let mut rows = state.dirty_rows().unwrap();
        let mut cells = rows.next_row().unwrap().cells;
        let mut graphemes = Vec::new();
        while cells.advance() {
            cells.graphemes(&mut graphemes);
            text.extend(graphemes.iter().filter_map(|&c| char::from_u32(c)));
        }
    }
    assert!(text.starts_with("before"), "{text:?}");

    term.write_capturing(b"\x1b[?2026l", &mut state);
    assert!(!term.held());
    term.write_capturing(b"\x1b[?2026h", &mut state);
    term.release_hold();
    assert!(!term.held());
    assert!(!term.mode(Mode::SyncOutput));
}

use ghostty_vt::{CursorStyle, Dirty, DirtyRow, RenderState, Rgb, Terminal, Wide};

fn rows(state: &mut RenderState) -> Vec<(u16, String, Vec<Wide>)> {
    let mut out = Vec::new();
    let mut graphemes = Vec::new();
    let mut rows = state.dirty_rows().unwrap();
    while let Some(DirtyRow { y, mut cells, .. }) = rows.next_row() {
        let mut text = String::new();
        let mut wides = Vec::new();
        while cells.advance() {
            cells.graphemes(&mut graphemes);
            text.extend(graphemes.iter().filter_map(|&c| char::from_u32(c)));
            wides.push(cells.wide());
        }
        out.push((y, text, wides));
    }
    out
}

#[test]
fn first_update_is_full_and_clean_resets_it() {
    let mut term = Terminal::new(10, 3).unwrap();
    let mut state = RenderState::new().unwrap();
    term.write(b"hi");
    state.update(&term).unwrap();
    assert_eq!(state.dirty().unwrap(), Dirty::Full);
    assert_eq!(state.size().unwrap(), (10, 3));
    let rows = rows(&mut state);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].1, "hi");

    state.clean().unwrap();
    state.update(&term).unwrap();
    assert_eq!(state.dirty().unwrap(), Dirty::Clean);
}

#[test]
fn only_changed_rows_are_dirty() {
    let mut term = Terminal::new(10, 3).unwrap();
    let mut state = RenderState::new().unwrap();
    state.update(&term).unwrap();
    let _ = rows(&mut state);
    state.clean().unwrap();

    term.write(b"\x1b[3;1Hx");
    state.update(&term).unwrap();
    assert_eq!(state.dirty().unwrap(), Dirty::Partial);
    let rows = rows(&mut state);
    assert!(rows.iter().any(|(y, text, _)| *y == 2 && text == "x"));
    assert!(rows.iter().all(|(y, ..)| *y != 1));
}

#[test]
fn wide_cells_and_cursor_are_reported() {
    let mut term = Terminal::new(10, 3).unwrap();
    let mut state = RenderState::new().unwrap();
    term.write("日a".as_bytes());
    state.update(&term).unwrap();
    let rows = rows(&mut state);
    assert_eq!(
        &rows[0].2[..3],
        &[Wide::Wide, Wide::SpacerTail, Wide::Narrow]
    );

    let cursor = state.cursor().unwrap();
    assert_eq!(cursor.position, Some((3, 0)));
    assert!(cursor.visible);
    assert_eq!(cursor.style, CursorStyle::Block);
}

#[test]
fn default_and_explicit_colors() {
    let mut term = Terminal::new(10, 3).unwrap();
    let fg = Rgb {
        r: 200,
        g: 200,
        b: 200,
    };
    let bg = Rgb {
        r: 18,
        g: 18,
        b: 18,
    };
    term.set_default_colors(fg, bg, None).unwrap();
    term.write(b"\x1b[38;2;255;0;0;48;2;0;0;255;1;7mX");
    let mut state = RenderState::new().unwrap();
    state.update(&term).unwrap();
    let colors = state.colors().unwrap();
    assert_eq!(
        (colors.foreground, colors.background, colors.cursor),
        (fg, bg, None)
    );

    let mut rows = state.dirty_rows().unwrap();
    let mut cells = rows.next_row().unwrap().cells;
    assert!(cells.advance());
    assert_eq!(cells.foreground(), Some(Rgb { r: 255, g: 0, b: 0 }));
    assert_eq!(cells.background(), Some(Rgb { r: 0, g: 0, b: 255 }));
    let style = cells.style();
    assert!(style.bold && style.inverse);
    assert!(cells.advance());
    assert_eq!(cells.foreground(), None);
}

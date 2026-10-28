use ghostty_vt::{Cells, CursorStyle, DirtyRow, RenderState, Wide};

use crate::Rgb;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub width: u8,
    pub foreground: Rgb,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameCursor {
    pub x: u16,
    pub y: u16,
    pub wide: bool,
    pub style: CursorStyle,
    pub color: Rgb,
    pub blinking: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    pub cols: u16,
    pub rows: Vec<Row>,
    pub cursor: Option<FrameCursor>,
    pub foreground: Rgb,
    pub background: Rgb,
    pub selection: Rgb,
}

impl Frame {
    pub(crate) fn update(&mut self, state: &mut RenderState) -> Result<bool, ghostty_vt::Error> {
        let (cols, rows) = state.size()?;
        let colors = state.colors()?;
        let foreground = Rgb::from(colors.foreground);
        let background = Rgb::from(colors.background);
        let mut changed = false;

        if self.cols != cols || self.rows.len() != rows as usize {
            self.cols = cols;
            self.rows.resize_with(rows as usize, Row::default);
            for row in &mut self.rows {
                row.cells.resize_with(cols as usize, Cell::default);
                row.dirty = true;
            }
            changed = true;
        }
        if self.foreground != foreground || self.background != background {
            self.foreground = foreground;
            self.background = background;
            self.rows.iter_mut().for_each(|r| r.dirty = true);
            changed = true;
        }

        let mut graphemes = Vec::new();
        let mut dirty_rows = state.dirty_rows()?;
        while let Some(DirtyRow {
            y,
            selection,
            mut cells,
        }) = dirty_rows.next_row()
        {
            let Some(row) = self.rows.get_mut(y as usize) else {
                continue;
            };
            let mut x = 0;
            while cells.advance() && x < row.cells.len() {
                fill(
                    &mut row.cells[x],
                    &cells,
                    &mut graphemes,
                    foreground,
                    background,
                );
                if selection.is_some_and(|(start, end)| (start..=end).contains(&(x as u16))) {
                    row.cells[x].background = Some(self.selection);
                }
                x += 1;
            }
            row.dirty = true;
            changed = true;
        }

        let cursor = state.cursor()?;
        let cursor = match (cursor.visible, cursor.position) {
            (true, Some((x, y))) => Some(FrameCursor {
                x: if cursor.wide_tail {
                    x.saturating_sub(1)
                } else {
                    x
                },
                y,
                wide: cursor.wide_tail
                    || self
                        .rows
                        .get(y as usize)
                        .and_then(|r| r.cells.get(x as usize))
                        .is_some_and(|c| c.width == 2),
                style: cursor.style,
                color: colors.cursor.map_or(foreground, Rgb::from),
                blinking: cursor.blinking,
            }),
            _ => None,
        };
        if cursor != self.cursor {
            self.cursor = cursor;
            changed = true;
        }

        state.clean()?;
        Ok(changed)
    }
}

fn fill(cell: &mut Cell, source: &Cells<'_>, graphemes: &mut Vec<u32>, fg: Rgb, bg: Rgb) {
    source.graphemes(graphemes);
    cell.text.clear();
    cell.text
        .extend(graphemes.iter().filter_map(|&c| char::from_u32(c)));
    cell.width = match source.wide() {
        Wide::Narrow => 1,
        Wide::Wide => 2,
        Wide::SpacerTail | Wide::SpacerHead => 0,
    };

    let style = source.style();
    let mut foreground = source.foreground().map_or(fg, Rgb::from);
    let mut background = source.background().map(Rgb::from);
    if style.inverse {
        let swapped = background.unwrap_or(bg);
        background = Some(foreground);
        foreground = swapped;
    }
    if style.faint {
        foreground = foreground.mix(background.unwrap_or(bg), 0.5);
    }
    if style.invisible {
        foreground = background.unwrap_or(bg);
    }
    cell.foreground = foreground;
    cell.background = background;
    cell.bold = style.bold;
    cell.italic = style.italic;
    cell.underline = style.underline;
    cell.strikethrough = style.strikethrough;
}

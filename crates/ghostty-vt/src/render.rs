use std::ffi::c_void;
use std::marker::PhantomData;
use std::mem::size_of;
use std::ptr::{self, NonNull};

use ghostty_vt_sys as sys;

use crate::{Error, Terminal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<sys::GhosttyColorRgb> for Rgb {
    fn from(c: sys::GhosttyColorRgb) -> Rgb {
        Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

impl From<Rgb> for sys::GhosttyColorRgb {
    fn from(c: Rgb) -> sys::GhosttyColorRgb {
        sys::GhosttyColorRgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dirty {
    Clean,
    Partial,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub cursor: Option<Rgb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Bar,
    Block,
    Underline,
    BlockHollow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub position: Option<(u16, u16)>,
    pub wide_tail: bool,
    pub visible: bool,
    pub blinking: bool,
    pub style: CursorStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wide {
    Narrow,
    Wide,
    SpacerTail,
    SpacerHead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

pub struct RenderState {
    pub(crate) raw: NonNull<sys::GhosttyRenderStateImpl>,
    rows: NonNull<sys::GhosttyRenderStateRowIteratorImpl>,
    cells: NonNull<sys::GhosttyRenderStateRowCellsImpl>,
}

impl RenderState {
    pub fn new() -> Result<RenderState, Error> {
        let mut raw: sys::GhosttyRenderState = ptr::null_mut();
        let mut rows: sys::GhosttyRenderStateRowIterator = ptr::null_mut();
        let mut cells: sys::GhosttyRenderStateRowCells = ptr::null_mut();
        // SAFETY: null allocators select the library default; all out-pointers are valid; partial failures free what was created.
        unsafe {
            Error::check(sys::ghostty_render_state_new(ptr::null(), &mut raw))?;
            if let Err(e) = Error::check(sys::ghostty_render_state_row_iterator_new(
                ptr::null(),
                &mut rows,
            )) {
                sys::ghostty_render_state_free(raw);
                return Err(e);
            }
            if let Err(e) = Error::check(sys::ghostty_render_state_row_cells_new(
                ptr::null(),
                &mut cells,
            )) {
                sys::ghostty_render_state_row_iterator_free(rows);
                sys::ghostty_render_state_free(raw);
                return Err(e);
            }
        }
        Ok(RenderState {
            raw: NonNull::new(raw).ok_or(Error::InvalidValue)?,
            rows: NonNull::new(rows).ok_or(Error::InvalidValue)?,
            cells: NonNull::new(cells).ok_or(Error::InvalidValue)?,
        })
    }

    pub fn update(&mut self, terminal: &Terminal) -> Result<(), Error> {
        // SAFETY: both handles are live; the terminal is not mutated during the call.
        Error::check(unsafe {
            sys::ghostty_render_state_update(self.raw.as_ptr(), terminal.raw.as_ptr())
        })
    }

    fn get<T>(&self, data: sys::GhosttyRenderStateData, out: &mut T) -> Result<(), Error> {
        // SAFETY: `out` has the output type documented for `data`.
        Error::check(unsafe {
            sys::ghostty_render_state_get(self.raw.as_ptr(), data, (out as *mut T).cast::<c_void>())
        })
    }

    pub fn dirty(&self) -> Result<Dirty, Error> {
        let mut dirty: sys::GhosttyRenderStateDirty = 0;
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_DIRTY, &mut dirty)?;
        Ok(match dirty {
            sys::GHOSTTY_RENDER_STATE_DIRTY_FALSE => Dirty::Clean,
            sys::GHOSTTY_RENDER_STATE_DIRTY_PARTIAL => Dirty::Partial,
            _ => Dirty::Full,
        })
    }

    pub fn size(&self) -> Result<(u16, u16), Error> {
        let mut cols = 0u16;
        let mut rows = 0u16;
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_COLS, &mut cols)?;
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_ROWS, &mut rows)?;
        Ok((cols, rows))
    }

    pub fn colors(&self) -> Result<Colors, Error> {
        let mut colors = sys::GhosttyRenderStateColors {
            size: size_of::<sys::GhosttyRenderStateColors>(),
            ..Default::default()
        };
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_COLORS, &mut colors)?;
        Ok(Colors {
            background: colors.background.into(),
            foreground: colors.foreground.into(),
            cursor: colors.cursor_has_value.then(|| colors.cursor.into()),
        })
    }

    pub fn cursor(&self) -> Result<Cursor, Error> {
        let mut cursor = sys::GhosttyRenderStateCursor {
            size: size_of::<sys::GhosttyRenderStateCursor>(),
            ..Default::default()
        };
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_CURSOR, &mut cursor)?;
        Ok(Cursor {
            position: cursor
                .viewport_has_value
                .then_some((cursor.viewport_x, cursor.viewport_y)),
            wide_tail: cursor.wide_tail,
            visible: cursor.visible,
            blinking: cursor.blinking,
            style: match cursor.visual_style {
                sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BAR => CursorStyle::Bar,
                sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_UNDERLINE => CursorStyle::Underline,
                sys::GHOSTTY_RENDER_STATE_CURSOR_VISUAL_STYLE_BLOCK_HOLLOW => {
                    CursorStyle::BlockHollow
                }
                _ => CursorStyle::Block,
            },
        })
    }

    pub fn dirty_rows(&mut self) -> Result<DirtyRows<'_>, Error> {
        let mut rows = self.rows.as_ptr();
        self.get(sys::GHOSTTY_RENDER_STATE_DATA_ROW_ITERATOR, &mut rows)?;
        Ok(DirtyRows { state: self })
    }

    pub fn clean(&mut self) -> Result<(), Error> {
        // SAFETY: `self.raw` is live.
        Error::check(unsafe { sys::ghostty_render_state_clean(self.raw.as_ptr()) })
    }
}

impl Drop for RenderState {
    fn drop(&mut self) {
        // SAFETY: each handle was created in `new` and is freed exactly once, here.
        unsafe {
            sys::ghostty_render_state_row_cells_free(self.cells.as_ptr());
            sys::ghostty_render_state_row_iterator_free(self.rows.as_ptr());
            sys::ghostty_render_state_free(self.raw.as_ptr());
        }
    }
}

pub struct DirtyRows<'a> {
    state: &'a mut RenderState,
}

impl DirtyRows<'_> {
    fn selection(&self) -> Option<(u16, u16)> {
        let mut range = sys::GhosttyRenderStateRowSelection {
            size: size_of::<sys::GhosttyRenderStateRowSelection>(),
            ..Default::default()
        };
        // SAFETY: the iterator points at the row returned by the last `next_row`; `range` is a sized out-struct.
        Error::check(unsafe {
            sys::ghostty_render_state_row_get(
                self.state.rows.as_ptr(),
                sys::GHOSTTY_RENDER_STATE_ROW_DATA_SELECTION,
                (&mut range as *mut sys::GhosttyRenderStateRowSelection).cast::<c_void>(),
            )
        })
        .ok()?;
        Some((range.start_x, range.end_x))
    }

    pub fn next_row(&mut self) -> Option<DirtyRow<'_>> {
        let mut y = 0u16;
        // SAFETY: the iterator was populated from the render state borrowed by `self`.
        if !unsafe {
            sys::ghostty_render_state_row_iterator_next_dirty(self.state.rows.as_ptr(), &mut y)
        } {
            return None;
        }
        let mut cells = self.state.cells.as_ptr();
        // SAFETY: the iterator points at a valid row; `cells` is a live row-cells handle to populate.
        let ok = Error::check(unsafe {
            sys::ghostty_render_state_row_get(
                self.state.rows.as_ptr(),
                sys::GHOSTTY_RENDER_STATE_ROW_DATA_CELLS,
                (&mut cells as *mut sys::GhosttyRenderStateRowCells).cast::<c_void>(),
            )
        });
        ok.ok()?;
        let selection = self.selection();
        Some(DirtyRow {
            y,
            selection,
            cells: Cells {
                raw: self.state.cells,
                _rows: PhantomData,
            },
        })
    }
}

pub struct DirtyRow<'a> {
    pub y: u16,
    pub selection: Option<(u16, u16)>,
    pub cells: Cells<'a>,
}

pub struct Cells<'a> {
    raw: NonNull<sys::GhosttyRenderStateRowCellsImpl>,
    _rows: PhantomData<&'a mut ()>,
}

impl Cells<'_> {
    pub fn advance(&mut self) -> bool {
        // SAFETY: `self.raw` was populated for the current row and is exclusively borrowed.
        unsafe { sys::ghostty_render_state_row_cells_next(self.raw.as_ptr()) }
    }

    fn get<T>(&self, data: sys::GhosttyRenderStateRowCellsData, out: &mut T) -> Result<(), Error> {
        // SAFETY: `out` has the output type documented for `data`; the cursor points at a valid cell.
        Error::check(unsafe {
            sys::ghostty_render_state_row_cells_get(
                self.raw.as_ptr(),
                data,
                (out as *mut T).cast::<c_void>(),
            )
        })
    }

    pub fn graphemes(&self, out: &mut Vec<u32>) {
        out.clear();
        let mut len = 0u32;
        if self
            .get(
                sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_LEN,
                &mut len,
            )
            .is_err()
            || len == 0
        {
            return;
        }
        out.resize(len as usize, 0);
        // SAFETY: `out` holds `len` writable u32 values, as GRAPHEMES_BUF requires.
        let result = Error::check(unsafe {
            sys::ghostty_render_state_row_cells_get(
                self.raw.as_ptr(),
                sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF,
                out.as_mut_ptr().cast::<c_void>(),
            )
        });
        if result.is_err() {
            out.clear();
        }
    }

    pub fn style(&self) -> CellStyle {
        let mut has_styling = false;
        if self
            .get(
                sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_HAS_STYLING,
                &mut has_styling,
            )
            .is_err()
            || !has_styling
        {
            return CellStyle::default();
        }
        let mut style = sys::GhosttyStyle {
            size: size_of::<sys::GhosttyStyle>(),
            ..Default::default()
        };
        if self
            .get(sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_STYLE, &mut style)
            .is_err()
        {
            return CellStyle::default();
        }
        CellStyle {
            bold: style.bold,
            italic: style.italic,
            faint: style.faint,
            inverse: style.inverse,
            invisible: style.invisible,
            underline: style.underline != 0,
            strikethrough: style.strikethrough,
        }
    }

    pub fn foreground(&self) -> Option<Rgb> {
        let mut rgb = sys::GhosttyColorRgb::default();
        self.get(sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_FG_COLOR, &mut rgb)
            .ok()
            .map(|()| rgb.into())
    }

    pub fn background(&self) -> Option<Rgb> {
        let mut rgb = sys::GhosttyColorRgb::default();
        self.get(sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_BG_COLOR, &mut rgb)
            .ok()
            .map(|()| rgb.into())
    }

    pub fn wide(&self) -> Wide {
        let mut cell: sys::GhosttyCell = 0;
        if self
            .get(sys::GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_RAW, &mut cell)
            .is_err()
        {
            return Wide::Narrow;
        }
        let mut wide: sys::GhosttyCellWide = sys::GHOSTTY_CELL_WIDE_NARROW;
        // SAFETY: `cell` is a raw cell value just read from the render state; `wide` matches GHOSTTY_CELL_DATA_WIDE.
        let ok = Error::check(unsafe {
            sys::ghostty_cell_get(
                cell,
                sys::GHOSTTY_CELL_DATA_WIDE,
                (&mut wide as *mut sys::GhosttyCellWide).cast::<c_void>(),
            )
        });
        match (ok, wide) {
            (Ok(()), sys::GHOSTTY_CELL_WIDE_WIDE) => Wide::Wide,
            (Ok(()), sys::GHOSTTY_CELL_WIDE_SPACER_TAIL) => Wide::SpacerTail,
            (Ok(()), sys::GHOSTTY_CELL_WIDE_SPACER_HEAD) => Wide::SpacerHead,
            _ => Wide::Narrow,
        }
    }
}

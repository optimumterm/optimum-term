use std::collections::HashMap;
use std::hash::Hash;

use ghostty_vt::{CursorStyle, codepoint_width};

use crate::{Frame, FrameCursor, Rgb, Row, Theme};

pub const ATLAS_SIZE: u32 = 2048;
pub const SOLID: u32 = 0;
pub const GRAYSCALE: u32 = 1;
pub const CLEARTYPE: u32 = 2;
pub const COLOR: u32 = 3;
const NO_COLUMN: u16 = u16::MAX;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Instance {
    pub position: [i16; 2],
    pub size: [u16; 2],
    pub texcoord: [u16; 2],
    pub color: u32,
    pub shading: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellMetrics {
    pub width: u32,
    pub height: u32,
    pub baseline: i32,
    pub underline_position: i32,
    pub underline_thickness: u32,
    pub strikethrough_position: i32,
}

pub struct Bitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub trait GlyphSource {
    type Id: Copy + Eq + Hash;

    fn metrics(&self) -> CellMetrics;
    fn shading(&self, id: Self::Id) -> u32;
    fn lookup(&mut self, text: &str, bold: bool, italic: bool) -> Option<Self::Id>;
    fn rasterize(&mut self, id: Self::Id) -> Option<Bitmap>;
}

#[derive(Debug, Clone, Copy)]
pub struct View<'a> {
    pub focused: bool,
    pub preedit: &'a str,
    pub overlay: &'a [String],
    pub error: Option<&'a str>,
    pub cursor_on: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    pub padding: i32,
    pub border: u32,
    pub opacity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasGlyph {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub left: i32,
    pub top: i32,
}

pub struct Upload {
    pub x: u32,
    pub y: u32,
    pub bitmap: Bitmap,
}

pub enum Slot {
    Cached(Option<AtlasGlyph>),
    Full,
}

pub struct Atlas<K> {
    glyphs: HashMap<(K, u32), Option<AtlasGlyph>>,
    pending: Vec<Upload>,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
}

impl<K: Copy + Eq + Hash> Default for Atlas<K> {
    fn default() -> Self {
        Atlas {
            glyphs: HashMap::new(),
            pending: Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            shelf_height: 0,
        }
    }
}

impl<K: Copy + Eq + Hash> Atlas<K> {
    pub fn clear(&mut self) {
        *self = Atlas::default();
    }

    pub fn get(&self, glyph: K, shading: u32) -> Option<Option<AtlasGlyph>> {
        self.glyphs.get(&(glyph, shading)).copied()
    }

    pub fn insert(&mut self, glyph: K, shading: u32, bitmap: Option<Bitmap>) -> Slot {
        let Some(bitmap) = bitmap else {
            self.glyphs.insert((glyph, shading), None);
            return Slot::Cached(None);
        };
        let Some((x, y)) = self.allocate(bitmap.width, bitmap.height) else {
            return Slot::Full;
        };
        let entry = AtlasGlyph {
            x: x as u16,
            y: y as u16,
            width: bitmap.width as u16,
            height: bitmap.height as u16,
            left: bitmap.left,
            top: bitmap.top,
        };
        self.glyphs.insert((glyph, shading), Some(entry));
        self.pending.push(Upload { x, y, bitmap });
        Slot::Cached(Some(entry))
    }

    pub fn take_uploads(&mut self) -> Vec<Upload> {
        std::mem::take(&mut self.pending)
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<(u32, u32)> {
        let (w, h) = (width + 1, height + 1);
        if w > ATLAS_SIZE || h > ATLAS_SIZE {
            return None;
        }
        if self.cursor_x + w > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.shelf_height;
            self.shelf_height = 0;
        }
        if self.cursor_y + h > ATLAS_SIZE {
            return None;
        }
        let position = (self.cursor_x, self.cursor_y);
        self.cursor_x += w;
        self.shelf_height = self.shelf_height.max(h);
        Some(position)
    }
}

#[derive(Default)]
struct RowCache {
    backgrounds: Vec<Instance>,
    glyphs: Vec<Instance>,
    glyph_columns: Vec<u16>,
}

struct AtlasFull;

pub struct Scene<K> {
    atlas: Atlas<K>,
    rows: Vec<RowCache>,
    instances: Vec<Instance>,
}

impl<K: Copy + Eq + Hash> Default for Scene<K> {
    fn default() -> Self {
        Scene {
            atlas: Atlas::default(),
            rows: Vec::new(),
            instances: Vec::new(),
        }
    }
}

impl<K: Copy + Eq + Hash> Scene<K> {
    pub fn reset(&mut self) {
        self.atlas.clear();
        self.rows.clear();
    }

    pub fn invalidate(&mut self) {
        self.rows.clear();
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn take_uploads(&mut self) -> Vec<Upload> {
        self.atlas.take_uploads()
    }

    pub fn compose<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        frame: Option<&mut Frame>,
        theme: Theme,
        view: View<'_>,
    ) -> Rgb {
        self.instances.clear();
        let (foreground, background) = match frame {
            Some(frame) => {
                self.build(glyphs, layout, frame);
                self.assemble(glyphs, layout, frame, view);
                (frame.foreground, frame.background)
            }
            None => (theme.foreground, theme.background),
        };
        if !view.overlay.is_empty() {
            self.overlay(glyphs, layout, view.overlay, foreground, background);
        }
        if let Some(error) = view.error {
            let strip = background.mix(theme.palette[1], 0.45);
            self.error_line(glyphs, layout, error, foreground, strip);
        }
        background
    }

    fn error_line<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        text: &str,
        foreground: Rgb,
        background: Rgb,
    ) {
        let m = glyphs.metrics();
        let top = layout.height as i32 - m.height as i32;
        self.instances
            .push(solid(0, top, layout.width, m.height, background));
        let mut x = m.width as i32;
        for c in text.chars() {
            let span = u32::from(codepoint_width(c));
            if span == 0 {
                continue;
            }
            if x + (span * m.width) as i32 > layout.width as i32 {
                break;
            }
            let mut buf = [0u8; 4];
            if let Ok(Some(glyph)) = self.glyph(
                glyphs,
                c.encode_utf8(&mut buf),
                false,
                false,
                x,
                top,
                foreground,
            ) {
                self.instances.push(glyph);
            }
            x += (span * m.width) as i32;
        }
    }

    fn overlay<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        lines: &[String],
        foreground: Rgb,
        background: Rgb,
    ) {
        let m = glyphs.metrics();
        let cells =
            |line: &str| -> u32 { line.chars().map(|c| u32::from(codepoint_width(c))).sum() };
        let widest = lines.iter().map(|l| cells(l)).max().unwrap_or(0);
        let width = (widest + 4) * m.width;
        let height = (lines.len() as u32 + 2) * m.height;
        let left = ((layout.width as i32 - width as i32) / 2).max(0);
        let top = ((layout.height as i32 - height as i32) / 2).max(0);
        let border = layout.border.max(1);
        self.instances.push(solid(
            left,
            top,
            width,
            height,
            background.mix(foreground, 0.35),
        ));
        self.instances.push(solid(
            left + border as i32,
            top + border as i32,
            width.saturating_sub(2 * border),
            height.saturating_sub(2 * border),
            background.mix(foreground, 0.08),
        ));
        for (row, line) in lines.iter().enumerate() {
            let mut x = left + ((2 * m.width + (widest - cells(line)) / 2 * m.width) as i32);
            let y = top + (row as i32 + 1) * m.height as i32;
            for c in line.chars() {
                let span = u32::from(codepoint_width(c));
                if span == 0 {
                    continue;
                }
                let mut buf = [0u8; 4];
                if let Ok(Some(glyph)) = self.glyph(
                    glyphs,
                    c.encode_utf8(&mut buf),
                    false,
                    false,
                    x,
                    y,
                    foreground,
                ) {
                    self.instances.push(glyph);
                }
                x += (span * m.width) as i32;
            }
        }
    }

    fn build<G: GlyphSource<Id = K>>(&mut self, glyphs: &mut G, layout: Layout, frame: &mut Frame) {
        if self.rows.len() != frame.rows.len() {
            self.rows.resize_with(frame.rows.len(), RowCache::default);
            frame.rows.iter_mut().for_each(|r| r.dirty = true);
        }
        for _ in 0..2 {
            let mut full = false;
            for (y, row) in frame.rows.iter_mut().enumerate() {
                if !row.dirty {
                    continue;
                }
                let mut cache = std::mem::take(&mut self.rows[y]);
                let result = self.build_row(glyphs, layout, &mut cache, y, row, frame.background);
                self.rows[y] = cache;
                if result.is_err() {
                    full = true;
                    break;
                }
                row.dirty = false;
            }
            if !full {
                return;
            }
            self.atlas.clear();
            frame.rows.iter_mut().for_each(|r| r.dirty = true);
        }
    }

    fn build_row<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        cache: &mut RowCache,
        y: usize,
        row: &Row,
        default_background: Rgb,
    ) -> Result<(), AtlasFull> {
        cache.backgrounds.clear();
        cache.glyphs.clear();
        cache.glyph_columns.clear();

        let m = glyphs.metrics();
        let top = layout.padding + y as i32 * m.height as i32;

        for (column, cell) in row.cells.iter().enumerate() {
            let left = layout.padding + column as i32 * m.width as i32;
            let span = m.width * u32::from(cell.width.max(1));

            if let Some(bg) = cell.background
                && bg != default_background
                && cell.width > 0
            {
                let color = pack_alpha(bg, layout.opacity);
                match cache.backgrounds.last_mut() {
                    Some(last)
                        if last.color == color
                            && i32::from(last.position[0]) + i32::from(last.size[0]) == left =>
                    {
                        last.size[0] += span as u16;
                    }
                    _ => cache.backgrounds.push(Instance {
                        color,
                        ..solid(left, top, span, m.height, bg)
                    }),
                }
            }

            if cell.width == 0 {
                continue;
            }
            if cell.underline {
                cache.glyphs.push(solid(
                    left,
                    top + m.underline_position,
                    span,
                    m.underline_thickness,
                    cell.foreground,
                ));
                cache.glyph_columns.push(NO_COLUMN);
            }
            if cell.strikethrough {
                cache.glyphs.push(solid(
                    left,
                    top + m.strikethrough_position,
                    span,
                    m.underline_thickness,
                    cell.foreground,
                ));
                cache.glyph_columns.push(NO_COLUMN);
            }
            if cell.text.is_empty() || cell.text == " " {
                continue;
            }
            if let Some(glyph) = self.glyph(
                glyphs,
                &cell.text,
                cell.bold,
                cell.italic,
                left,
                top,
                cell.foreground,
            )? {
                cache.glyphs.push(glyph);
                cache.glyph_columns.push(column as u16);
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn glyph<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        text: &str,
        bold: bool,
        italic: bool,
        left: i32,
        top: i32,
        color: Rgb,
    ) -> Result<Option<Instance>, AtlasFull> {
        let Some(id) = glyphs.lookup(text, bold, italic) else {
            return Ok(None);
        };
        let shading = glyphs.shading(id);
        let entry = match self.atlas.get(id, shading) {
            Some(entry) => entry,
            None => {
                let bitmap = glyphs.rasterize(id);
                match self.atlas.insert(id, shading, bitmap) {
                    Slot::Cached(entry) => entry,
                    Slot::Full => return Err(AtlasFull),
                }
            }
        };
        let Some(g) = entry else {
            return Ok(None);
        };
        Ok(Some(Instance {
            position: [
                (left + g.left) as i16,
                (top + glyphs.metrics().baseline + g.top) as i16,
            ],
            size: [g.width, g.height],
            texcoord: [g.x, g.y],
            color: pack(color),
            shading,
        }))
    }

    fn assemble<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        frame: &Frame,
        view: View<'_>,
    ) {
        for row in &self.rows {
            self.instances.extend_from_slice(&row.backgrounds);
        }

        let m = glyphs.metrics();
        let composing = !view.preedit.is_empty();
        let cursor = frame
            .cursor
            .filter(|c| !composing && (view.cursor_on || !c.blinking || !view.focused))
            .map(|mut cursor| {
                if !view.focused && cursor.style == CursorStyle::Block {
                    cursor.style = CursorStyle::BlockHollow;
                }
                cursor
            });
        let block_cursor = cursor.filter(|c| c.style == CursorStyle::Block);
        if let Some(cursor) = cursor {
            self.instances.extend(cursor_quads(
                cursor_rect(cursor, m, layout.padding),
                cursor.style,
                m,
                cursor.color,
            ));
        }

        for (y, row) in self.rows.iter().enumerate() {
            for (glyph, &column) in row.glyphs.iter().zip(&row.glyph_columns) {
                let mut glyph = *glyph;
                if let Some(cursor) = block_cursor
                    && cursor.y as usize == y
                    && cursor.x == column
                    && glyph.shading != COLOR
                {
                    glyph.color = pack(frame.background);
                }
                self.instances.push(glyph);
            }
        }

        if let (true, Some(cursor)) = (composing, frame.cursor) {
            self.preedit(glyphs, layout, view.preedit, cursor, frame);
        }
    }

    fn preedit<G: GlyphSource<Id = K>>(
        &mut self,
        glyphs: &mut G,
        layout: Layout,
        text: &str,
        cursor: FrameCursor,
        frame: &Frame,
    ) {
        let m = glyphs.metrics();
        let start = layout.padding + i32::from(cursor.x) * m.width as i32;
        let top = layout.padding + i32::from(cursor.y) * m.height as i32;
        let mut left = start;
        let mut placed = Vec::new();
        for c in text.chars() {
            let width = u32::from(codepoint_width(c));
            if width == 0 {
                continue;
            }
            let mut buf = [0u8; 4];
            if let Ok(Some(glyph)) = self.glyph(
                glyphs,
                c.encode_utf8(&mut buf),
                false,
                false,
                left,
                top,
                frame.foreground,
            ) {
                placed.push(glyph);
            }
            left += (m.width * width) as i32;
        }
        let span = (left - start) as u32;
        let thickness = m.underline_thickness.max(1);
        self.instances
            .push(solid(start, top, span, m.height, frame.background));
        self.instances.extend(placed);
        self.instances.push(solid(
            start,
            top + m.underline_position,
            span,
            thickness,
            frame.foreground,
        ));
        self.instances
            .push(solid(left, top, thickness, m.height, cursor.color));
    }
}

pub fn overlay_columns(width: u32, metrics: CellMetrics) -> usize {
    (width / metrics.width.max(1)).saturating_sub(8) as usize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

fn cursor_rect(cursor: FrameCursor, m: CellMetrics, padding: i32) -> Rect {
    Rect {
        left: padding + i32::from(cursor.x) * m.width as i32,
        top: padding + i32::from(cursor.y) * m.height as i32,
        width: m.width * if cursor.wide { 2 } else { 1 },
        height: m.height,
    }
}

fn cursor_quads(r: Rect, style: CursorStyle, m: CellMetrics, color: Rgb) -> Vec<Instance> {
    let t = m.underline_thickness.max(1);
    match style {
        CursorStyle::Block => vec![solid(r.left, r.top, r.width, r.height, color)],
        CursorStyle::Bar => vec![solid(r.left, r.top, t, r.height, color)],
        CursorStyle::Underline => vec![solid(
            r.left,
            r.top + m.underline_position,
            r.width,
            t,
            color,
        )],
        CursorStyle::BlockHollow => vec![
            solid(r.left, r.top, r.width, 1, color),
            solid(r.left, r.top + r.height as i32 - 1, r.width, 1, color),
            solid(r.left, r.top, 1, r.height, color),
            solid(r.left + r.width as i32 - 1, r.top, 1, r.height, color),
        ],
    }
}

fn solid(left: i32, top: i32, width: u32, height: u32, color: Rgb) -> Instance {
    Instance {
        position: [left as i16, top as i16],
        size: [width as u16, height as u16],
        texcoord: [0, 0],
        color: pack(color),
        shading: SOLID,
    }
}

pub fn pack(c: Rgb) -> u32 {
    pack_alpha(c, 1.0)
}

pub fn pack_alpha(c: Rgb, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let channel = |v: u8| (f32::from(v) * a).round() as u32;
    channel(c.r) | channel(c.g) << 8 | channel(c.b) << 16 | ((a * 255.0).round() as u32) << 24
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: CellMetrics = CellMetrics {
        width: 10,
        height: 20,
        baseline: 15,
        underline_position: 17,
        underline_thickness: 2,
        strikethrough_position: 10,
    };

    fn bitmap(width: u32, height: u32) -> Bitmap {
        Bitmap {
            left: 0,
            top: -(height as i32),
            width,
            height,
            rgba: vec![255; (width * height * 4) as usize],
        }
    }

    #[test]
    fn instance_is_twenty_bytes() {
        assert_eq!(size_of::<Instance>(), 20);
    }

    #[test]
    fn colors_pack_as_rgba8() {
        assert_eq!(pack(Rgb::new(1, 2, 3)), 0xff03_0201);
        assert_eq!(pack_alpha(Rgb::new(200, 100, 0), 0.5), 0x8000_3264);
    }

    #[test]
    fn cursor_shapes_follow_style() {
        let r = Rect {
            left: 12,
            top: 52,
            width: 20,
            height: 20,
        };
        let c = Rgb::new(255, 255, 255);
        let block = cursor_quads(r, CursorStyle::Block, M, c);
        assert_eq!(block[0].size, [20, 20]);
        let bar = cursor_quads(r, CursorStyle::Bar, M, c);
        assert_eq!(bar[0].size, [2, 20]);
        let underline = cursor_quads(r, CursorStyle::Underline, M, c);
        assert_eq!(underline[0].position, [12, 69]);
        assert_eq!(cursor_quads(r, CursorStyle::BlockHollow, M, c).len(), 4);
    }

    #[test]
    fn glyphs_are_packed_on_shelves_without_overlap() {
        let mut atlas = Atlas::<u16>::default();
        let mut placed = Vec::new();
        for i in 0..500 {
            match atlas.insert(i, GRAYSCALE, Some(bitmap(30, 40))) {
                Slot::Cached(Some(g)) => placed.push(g),
                _ => panic!("atlas filled too early"),
            }
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                let apart = a.x + a.width <= b.x
                    || b.x + b.width <= a.x
                    || a.y + a.height <= b.y
                    || b.y + b.height <= a.y;
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(atlas.take_uploads().len(), 500);
    }

    #[test]
    fn blank_glyphs_are_cached_without_space() {
        let mut atlas = Atlas::<u16>::default();
        atlas.insert(1, GRAYSCALE, None);
        assert_eq!(atlas.get(1, GRAYSCALE), Some(None));
        assert!(atlas.take_uploads().is_empty());
    }

    #[test]
    fn full_atlas_reports_full() {
        let mut atlas = Atlas::<u16>::default();
        let mut i = 0;
        loop {
            if let Slot::Full = atlas.insert(i, GRAYSCALE, Some(bitmap(500, 500))) {
                break;
            }
            i += 1;
            assert!(i < 100);
        }
        assert_eq!(i, 16);
    }
}

use std::collections::HashMap;

use crate::font::{Antialias, Bitmap, GlyphId};

pub(crate) const ATLAS_SIZE: u32 = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AtlasGlyph {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub left: i32,
    pub top: i32,
}

pub(crate) struct Upload {
    pub x: u32,
    pub y: u32,
    pub bitmap: Bitmap,
}

pub(crate) enum Slot {
    Cached(Option<AtlasGlyph>),
    Full,
}

pub(crate) struct Atlas {
    glyphs: HashMap<(GlyphId, Antialias), Option<AtlasGlyph>>,
    pending: Vec<Upload>,
    cursor_x: u32,
    cursor_y: u32,
    shelf_height: u32,
}

impl Atlas {
    pub fn new() -> Atlas {
        Atlas {
            glyphs: HashMap::new(),
            pending: Vec::new(),
            cursor_x: 0,
            cursor_y: 0,
            shelf_height: 0,
        }
    }

    pub fn clear(&mut self) {
        *self = Atlas::new();
    }

    pub fn get(&self, glyph: GlyphId, antialias: Antialias) -> Option<Option<AtlasGlyph>> {
        self.glyphs.get(&(glyph, antialias)).copied()
    }

    pub fn insert(&mut self, glyph: GlyphId, antialias: Antialias, bitmap: Option<Bitmap>) -> Slot {
        let Some(bitmap) = bitmap else {
            self.glyphs.insert((glyph, antialias), None);
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
        self.glyphs.insert((glyph, antialias), Some(entry));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bitmap(width: u32, height: u32) -> Bitmap {
        Bitmap {
            left: 0,
            top: -(height as i32),
            width,
            height,
            rgba: vec![255; (width * height * 4) as usize],
        }
    }

    fn id(index: u16) -> GlyphId {
        GlyphId { face: 0, index }
    }

    #[test]
    fn glyphs_are_packed_on_shelves_without_overlap() {
        let mut atlas = Atlas::new();
        let mut placed = Vec::new();
        for i in 0..500 {
            match atlas.insert(id(i), Antialias::Grayscale, Some(bitmap(30, 40))) {
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
        let mut atlas = Atlas::new();
        atlas.insert(id(1), Antialias::Grayscale, None);
        assert_eq!(atlas.get(id(1), Antialias::Grayscale), Some(None));
        assert!(atlas.take_uploads().is_empty());
    }

    #[test]
    fn full_atlas_reports_full() {
        let mut atlas = Atlas::new();
        let mut i = 0;
        loop {
            if let Slot::Full = atlas.insert(id(i), Antialias::Grayscale, Some(bitmap(500, 500))) {
                break;
            }
            i += 1;
            assert!(i < 100);
        }
        assert_eq!(i, 16);
    }
}

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFData, CFRange, CFRetained, CFString, CGFloat, CGPoint};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_core_text::{
    CTFont, CTFontManagerCreateFontDescriptorFromData, CTFontOrientation, CTFontSymbolicTraits,
};
use optimum_core::{Bitmap, COLOR, CellMetrics, FontConfig, GRAYSCALE, GlyphSource};

const FALLBACK_FAMILY: &str = "Menlo";
const BUNDLED: [&[u8]; 4] = [
    include_bytes!(
        "../../../vendor/fonts/jetbrains-mono-nerd-font/JetBrainsMonoNerdFont-Regular.ttf"
    ),
    include_bytes!("../../../vendor/fonts/jetbrains-mono-nerd-font/JetBrainsMonoNerdFont-Bold.ttf"),
    include_bytes!(
        "../../../vendor/fonts/jetbrains-mono-nerd-font/JetBrainsMonoNerdFont-Italic.ttf"
    ),
    include_bytes!(
        "../../../vendor/fonts/jetbrains-mono-nerd-font/JetBrainsMonoNerdFont-BoldItalic.ttf"
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct GlyphId {
    pub face: u16,
    pub index: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Variant {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

impl Variant {
    const ALL: [Variant; 4] = [
        Variant::Regular,
        Variant::Bold,
        Variant::Italic,
        Variant::BoldItalic,
    ];

    fn new(bold: bool, italic: bool) -> Variant {
        match (bold, italic) {
            (false, false) => Variant::Regular,
            (true, false) => Variant::Bold,
            (false, true) => Variant::Italic,
            (true, true) => Variant::BoldItalic,
        }
    }

    fn traits(self) -> CTFontSymbolicTraits {
        match self {
            Variant::Regular => CTFontSymbolicTraits::empty(),
            Variant::Bold => CTFontSymbolicTraits::TraitBold,
            Variant::Italic => CTFontSymbolicTraits::TraitItalic,
            Variant::BoldItalic => {
                CTFontSymbolicTraits::TraitBold | CTFontSymbolicTraits::TraitItalic
            }
        }
    }
}

struct Face {
    font: CFRetained<CTFont>,
    color: bool,
}

pub(crate) struct Fonts {
    faces: Vec<Face>,
    face_ids: HashMap<String, u16>,
    lookups: HashMap<(String, Variant), Option<GlyphId>>,
    gray: CFRetained<CGColorSpace>,
    rgb: CFRetained<CGColorSpace>,
    pub metrics: CellMetrics,
}

impl Fonts {
    pub fn new(scale: f64, font: &FontConfig) -> Option<Fonts> {
        let size = CGFloat::from(font.size) * scale;
        let primaries = installed(&font.family, size)
            .or_else(|| bundled(size))
            .or_else(|| installed(FALLBACK_FAMILY, size))?;
        let mut fonts = Fonts {
            faces: Vec::new(),
            face_ids: HashMap::new(),
            lookups: HashMap::new(),
            gray: CGColorSpace::new_device_gray()?,
            rgb: CGColorSpace::new_device_rgb()?,
            metrics: CellMetrics {
                width: 1,
                height: 1,
                baseline: 0,
                underline_position: 0,
                underline_thickness: 1,
                strikethrough_position: 0,
            },
        };
        for primary in primaries {
            fonts.push_face(primary);
        }
        fonts.metrics = fonts.compute_metrics(font.line_height);
        Some(fonts)
    }

    fn push_face(&mut self, font: CFRetained<CTFont>) -> u16 {
        // SAFETY: `font` is a live CTFont.
        let (name, traits) =
            unsafe { (font.post_script_name().to_string(), font.symbolic_traits()) };
        if self.faces.len() >= Variant::ALL.len()
            && let Some(&id) = self.face_ids.get(&name)
        {
            return id;
        }
        let id = self.faces.len() as u16;
        self.faces.push(Face {
            font,
            color: traits.contains(CTFontSymbolicTraits::TraitColorGlyphs),
        });
        self.face_ids.entry(name).or_insert(id);
        id
    }

    fn compute_metrics(&self, line_height: f32) -> CellMetrics {
        let font = &self.faces[Variant::Regular as usize].font;
        // SAFETY: `font` is live; the glyph and advance pointers reference one valid element each.
        let (ascent, descent, leading, underline_position, underline_thickness, x_height, advance) = unsafe {
            let advance = glyph_for(font, &[u16::from(b'0')])
                .map(|mut glyph| {
                    font.advances_for_glyphs(
                        CTFontOrientation::Horizontal,
                        NonNull::from(&mut glyph),
                        ptr::null_mut(),
                        1,
                    )
                })
                .filter(|advance| *advance > 0.0)
                .unwrap_or(font.size() / 2.0);
            (
                font.ascent(),
                font.descent(),
                font.leading(),
                font.underline_position(),
                font.underline_thickness(),
                font.x_height(),
                advance,
            )
        };
        let natural = ascent + descent + leading;
        let height = (natural * CGFloat::from(line_height)).round().max(1.0);
        let baseline = (ascent + (leading + height - natural) / 2.0).round();
        CellMetrics {
            width: advance.round().max(1.0) as u32,
            height: height as u32,
            baseline: baseline as i32,
            underline_position: (baseline - underline_position).round() as i32,
            underline_thickness: underline_thickness.round().max(1.0) as u32,
            strikethrough_position: (baseline - x_height / 2.0).round() as i32,
        }
    }

    pub fn lookup(&mut self, text: &str, bold: bool, italic: bool) -> Option<GlyphId> {
        let variant = Variant::new(bold, italic);
        if let Some(found) = self.lookups.get(&(text.to_owned(), variant)) {
            return *found;
        }
        let found = self.resolve(text, variant);
        self.lookups.insert((text.to_owned(), variant), found);
        found
    }

    fn resolve(&mut self, text: &str, variant: Variant) -> Option<GlyphId> {
        let first = text.chars().next()?;
        let mut units = [0u16; 2];
        let units = first.encode_utf16(&mut units);
        let primary = variant as u16;
        if text.chars().nth(1).is_none()
            && let Some(index) = glyph_for(&self.faces[primary as usize].font, units)
        {
            return Some(GlyphId {
                face: primary,
                index,
            });
        }
        let string = CFString::from_str(text);
        let length = text.encode_utf16().count() as isize;
        // SAFETY: the range covers the whole string just created.
        let fallback = unsafe {
            self.faces[primary as usize]
                .font
                .for_string(&string, CFRange::new(0, length))
        };
        let index = glyph_for(&fallback, units)?;
        let face = self.push_face(fallback);
        Some(GlyphId { face, index })
    }

    pub fn is_color(&self, glyph: GlyphId) -> bool {
        self.faces[glyph.face as usize].color
    }

    pub fn rasterize(&self, glyph: GlyphId) -> Option<Bitmap> {
        let face = &self.faces[glyph.face as usize];
        let mut index = glyph.index;
        // SAFETY: one valid glyph in, no per-glyph rects requested.
        let bounds = unsafe {
            face.font.bounding_rects_for_glyphs(
                CTFontOrientation::Horizontal,
                NonNull::from(&mut index),
                ptr::null_mut(),
                1,
            )
        };
        if bounds.size.width <= 0.0 || bounds.size.height <= 0.0 {
            return None;
        }
        let left = bounds.origin.x.floor() as i32 - 1;
        let bottom = bounds.origin.y.floor() as i32 - 1;
        let right = (bounds.origin.x + bounds.size.width).ceil() as i32 + 1;
        let top = (bounds.origin.y + bounds.size.height).ceil() as i32 + 1;
        let width = (right - left) as u32;
        let height = (top - bottom) as u32;

        let channels = if face.color { 4 } else { 1 };
        let stride = width as usize * channels;
        let mut pixels = vec![0u8; stride * height as usize];
        let (space, info) = if face.color {
            (
                &self.rgb,
                CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
            )
        } else {
            (&self.gray, CGImageAlphaInfo::None.0)
        };
        // SAFETY: `pixels` holds `stride * height` bytes and outlives the context, which is dropped below.
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast::<c_void>(),
                width as usize,
                height as usize,
                8,
                stride,
                Some(space),
                info,
            )
        }?;
        draw(&context, &face.font, glyph.index, left, bottom);
        debug_assert_eq!(
            CGBitmapContextGetData(Some(&context)),
            pixels.as_mut_ptr().cast::<c_void>()
        );
        drop(context);

        let rgba = if face.color {
            pixels
        } else {
            pixels.iter().flat_map(|&a| [a, a, a, a]).collect()
        };
        Some(Bitmap {
            left,
            top: -top,
            width,
            height,
            rgba,
        })
    }
}

impl GlyphSource for Fonts {
    type Id = GlyphId;

    fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    fn shading(&self, id: GlyphId) -> u32 {
        if self.is_color(id) { COLOR } else { GRAYSCALE }
    }

    fn lookup(&mut self, text: &str, bold: bool, italic: bool) -> Option<GlyphId> {
        Fonts::lookup(self, text, bold, italic)
    }

    fn rasterize(&mut self, id: GlyphId) -> Option<Bitmap> {
        Fonts::rasterize(self, id)
    }
}

fn draw(context: &CGContext, font: &CTFont, glyph: u16, left: i32, bottom: i32) {
    CGContext::set_allows_antialiasing(Some(context), true);
    CGContext::set_should_antialias(Some(context), true);
    CGContext::set_allows_font_smoothing(Some(context), true);
    CGContext::set_should_smooth_fonts(Some(context), false);
    CGContext::set_allows_font_subpixel_positioning(Some(context), false);
    CGContext::set_should_subpixel_position_fonts(Some(context), false);
    CGContext::set_gray_fill_color(Some(context), 1.0, 1.0);
    let mut glyph = glyph;
    let mut position = CGPoint::new(-CGFloat::from(left), -CGFloat::from(bottom));
    // SAFETY: one glyph and one position, both valid for the call; `context` is a live bitmap context.
    unsafe {
        font.draw_glyphs(
            NonNull::from(&mut glyph),
            NonNull::from(&mut position),
            1,
            context,
        );
    }
}

fn glyph_for(font: &CTFont, units: &[u16]) -> Option<u16> {
    let mut glyphs = [0u16; 2];
    let mut chars = [0u16; 2];
    chars[..units.len()].copy_from_slice(units);
    // SAFETY: `chars` and `glyphs` hold at least `units.len()` elements.
    let found = unsafe {
        font.glyphs_for_characters(
            NonNull::from(&mut chars).cast(),
            NonNull::from(&mut glyphs).cast(),
            units.len() as isize,
        )
    };
    (found && glyphs[0] != 0).then_some(glyphs[0])
}

fn installed(family: &str, size: CGFloat) -> Option<Vec<CFRetained<CTFont>>> {
    let name = CFString::from_str(family);
    // SAFETY: a null matrix means identity.
    let regular = unsafe { CTFont::with_name(&name, size, ptr::null()) };
    // SAFETY: `regular` is a live CTFont.
    let found = unsafe { regular.family_name() }.to_string();
    if !found.eq_ignore_ascii_case(family) {
        return None;
    }
    Some(
        Variant::ALL
            .iter()
            .map(|variant| styled(&regular, *variant, size))
            .collect(),
    )
}

fn styled(regular: &CFRetained<CTFont>, variant: Variant, size: CGFloat) -> CFRetained<CTFont> {
    if variant == Variant::Regular {
        return regular.clone();
    }
    let traits = variant.traits();
    // SAFETY: a null matrix means identity.
    unsafe { regular.copy_with_symbolic_traits(size, ptr::null(), traits, traits) }
        .unwrap_or_else(|| regular.clone())
}

fn bundled(size: CGFloat) -> Option<Vec<CFRetained<CTFont>>> {
    BUNDLED
        .iter()
        .map(|bytes| {
            let data = CFData::from_static_bytes(bytes);
            // SAFETY: `data` holds a complete TrueType file.
            let descriptor = unsafe { CTFontManagerCreateFontDescriptorFromData(&data) }?;
            // SAFETY: a null matrix means identity.
            Some(unsafe { CTFont::with_font_descriptor(&descriptor, size, ptr::null()) })
        })
        .collect()
}

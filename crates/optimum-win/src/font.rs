use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::ManuallyDrop;

use optimum_core::{
    Antialiasing, Bitmap, CLEARTYPE, CellMetrics, FontConfig, GRAYSCALE, GlyphSource, TextConfig,
};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE,
    DWRITE_FONT_STYLE_ITALIC, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_GLYPH_METRICS, DWRITE_GLYPH_RUN,
    DWRITE_GRID_FIT_MODE_DEFAULT, DWRITE_MEASURING_MODE_NATURAL, DWRITE_READING_DIRECTION,
    DWRITE_READING_DIRECTION_LEFT_TO_RIGHT, DWRITE_RENDERING_MODE,
    DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC, DWRITE_RENDERING_MODE_OUTLINE,
    DWRITE_TEXT_ANTIALIAS_MODE_CLEARTYPE, DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    DWRITE_TEXTURE_ALIASED_1x1, DWRITE_TEXTURE_CLEARTYPE_3x1, DWriteCreateFactory, IDWriteFactory2,
    IDWriteFactory5, IDWriteFont, IDWriteFontCollection, IDWriteFontFace, IDWriteFontFallback,
    IDWriteNumberSubstitution, IDWriteRenderingParams, IDWriteRenderingParams1,
    IDWriteTextAnalysisSource, IDWriteTextAnalysisSource_Impl,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FE_FONTSMOOTHINGCLEARTYPE, SPI_GETFONTSMOOTHINGTYPE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};
use windows::core::{BOOL, HSTRING, Interface, OutRef, PCWSTR, Result, implement, w};

const FALLBACK_FAMILY: &str = "Consolas";
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

thread_local! {
    static BUNDLED_COLLECTION: std::cell::OnceCell<Option<(IDWriteFontCollection, HSTRING)>> =
        const { std::cell::OnceCell::new() };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Antialias {
    Grayscale,
    ClearType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TextParams {
    pub antialias: Antialias,
    pub gamma: f32,
    pub enhanced_contrast: f32,
    pub grayscale_enhanced_contrast: f32,
    pub cleartype_level: f32,
}

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
    fn new(bold: bool, italic: bool) -> Variant {
        match (bold, italic) {
            (false, false) => Variant::Regular,
            (true, false) => Variant::Bold,
            (false, true) => Variant::Italic,
            (true, true) => Variant::BoldItalic,
        }
    }

    fn weight(self) -> DWRITE_FONT_WEIGHT {
        match self {
            Variant::Bold | Variant::BoldItalic => DWRITE_FONT_WEIGHT_BOLD,
            _ => DWRITE_FONT_WEIGHT_NORMAL,
        }
    }

    fn style(self) -> DWRITE_FONT_STYLE {
        match self {
            Variant::Italic | Variant::BoldItalic => DWRITE_FONT_STYLE_ITALIC,
            _ => DWRITE_FONT_STYLE_NORMAL,
        }
    }
}

pub(crate) struct Fonts {
    factory: IDWriteFactory2,
    collection: IDWriteFontCollection,
    fallback: IDWriteFontFallback,
    rendering_params: IDWriteRenderingParams,
    family: HSTRING,
    faces: Vec<IDWriteFontFace>,
    face_ids: HashMap<usize, u16>,
    lookups: HashMap<(String, Variant), Option<GlyphId>>,
    size_px: f32,
    line_height: f32,
    pub metrics: CellMetrics,
    pub params: TextParams,
}

impl Fonts {
    pub fn new(dpi: u32, font: &FontConfig, text: &TextConfig) -> Result<Fonts> {
        // SAFETY: DirectWrite factory and system objects are created and queried through their documented COM methods.
        unsafe {
            let factory: IDWriteFactory2 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let mut collection = None;
            factory.GetSystemFontCollection(&mut collection, false)?;
            let collection = collection.ok_or_else(windows::core::Error::empty)?;
            let system_fallback = factory.GetSystemFontFallback()?;
            let rendering_params = factory.CreateRenderingParams()?;
            let params = configured(system_text_params(&rendering_params), text);

            let requested = HSTRING::from(font.family.as_str());
            let fallback = HSTRING::from(FALLBACK_FAMILY);
            let (collection, family) = if family_index(&collection, &requested).is_some() {
                (collection, requested)
            } else if let Some(bundled) = bundled(&factory) {
                bundled
            } else if family_index(&collection, &fallback).is_some() {
                (collection, fallback)
            } else {
                return Err(windows::core::Error::empty());
            };

            let mut fonts = Fonts {
                factory,
                collection,
                fallback: system_fallback,
                rendering_params,
                family,
                faces: Vec::new(),
                face_ids: HashMap::new(),
                lookups: HashMap::new(),
                size_px: font.size * dpi as f32 / 72.0,
                line_height: font.line_height,
                metrics: CellMetrics {
                    width: 1,
                    height: 1,
                    baseline: 0,
                    underline_position: 0,
                    underline_thickness: 1,
                    strikethrough_position: 0,
                },
                params,
            };
            for variant in [
                Variant::Regular,
                Variant::Bold,
                Variant::Italic,
                Variant::BoldItalic,
            ] {
                let font = fonts.primary_font(variant)?;
                let face = font.CreateFontFace()?;
                fonts.add_face(face);
            }
            fonts.metrics = fonts.compute_metrics()?;
            Ok(fonts)
        }
    }

    fn primary_font(&self, variant: Variant) -> Result<IDWriteFont> {
        let index =
            family_index(&self.collection, &self.family).ok_or_else(windows::core::Error::empty)?;
        // SAFETY: `index` was returned by FindFamilyName on this collection.
        unsafe {
            self.collection.GetFontFamily(index)?.GetFirstMatchingFont(
                variant.weight(),
                DWRITE_FONT_STRETCH_NORMAL,
                variant.style(),
            )
        }
    }

    fn add_face(&mut self, face: IDWriteFontFace) -> u16 {
        let key = face.as_raw() as usize;
        if let Some(&id) = self.face_ids.get(&key) {
            return id;
        }
        let id = self.faces.len() as u16;
        self.faces.push(face);
        self.face_ids.insert(key, id);
        id
    }

    fn compute_metrics(&self) -> Result<CellMetrics> {
        let face = &self.faces[Variant::Regular as usize];
        let mut m = DWRITE_FONT_METRICS::default();
        // SAFETY: `face` is live; `m` is a valid out-pointer.
        unsafe { face.GetMetrics(&mut m) };
        let scale = self.size_px / f32::from(m.designUnitsPerEm);

        let zero = glyph_index(face, '0').unwrap_or(0);
        let mut glyph = DWRITE_GLYPH_METRICS::default();
        // SAFETY: one glyph index in, one metrics struct out.
        unsafe { face.GetDesignGlyphMetrics(&zero, 1, &mut glyph, false)? };
        let advance = if glyph.advanceWidth > 0 {
            glyph.advanceWidth as f32 * scale
        } else {
            self.size_px / 2.0
        };

        let ascent = f32::from(m.ascent) * scale;
        let descent = f32::from(m.descent) * scale;
        let line_gap = f32::from(m.lineGap) * scale;
        let natural_height = ascent + descent + line_gap;
        let height = (natural_height * self.line_height).round().max(1.0);
        let baseline = (ascent + (line_gap + height - natural_height) / 2.0).round();

        Ok(CellMetrics {
            width: advance.round().max(1.0) as u32,
            height: height as u32,
            baseline: baseline as i32,
            underline_position: (baseline - f32::from(m.underlinePosition) * scale).round() as i32,
            underline_thickness: (f32::from(m.underlineThickness) * scale).round().max(1.0) as u32,
            strikethrough_position: (baseline - f32::from(m.strikethroughPosition) * scale).round()
                as i32,
        })
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
        let primary = variant as u16;
        if text.chars().nth(1).is_none()
            && let Some(index) = glyph_index(&self.faces[primary as usize], first)
        {
            return Some(GlyphId {
                face: primary,
                index,
            });
        }

        let face = self.fallback_face(text, variant)?;
        let index = glyph_index(&face, first)?;
        let face = self.add_face(face);
        Some(GlyphId { face, index })
    }

    fn fallback_face(&self, text: &str, variant: Variant) -> Option<IDWriteFontFace> {
        let source: IDWriteTextAnalysisSource = AnalysisSource::new(text).into();
        let length = text.encode_utf16().count() as u32;
        let mut mapped_length = 0;
        let mut font = None;
        let mut scale = 1.0;
        // SAFETY: `source` stays alive for the call; all out-pointers are valid.
        unsafe {
            self.fallback
                .MapCharacters(
                    &source,
                    0,
                    length,
                    &self.collection,
                    &self.family,
                    variant.weight(),
                    variant.style(),
                    DWRITE_FONT_STRETCH_NORMAL,
                    &mut mapped_length,
                    &mut font,
                    &mut scale,
                )
                .ok()?;
            font?.CreateFontFace().ok()
        }
    }

    pub fn rasterize(&self, glyph: GlyphId, antialias: Antialias) -> Result<Option<Bitmap>> {
        let face = &self.faces[glyph.face as usize];
        let advance = 0.0f32;
        let run = DWRITE_GLYPH_RUN {
            fontFace: ManuallyDrop::new(Some(face.clone())),
            fontEmSize: self.size_px,
            glyphCount: 1,
            glyphIndices: &glyph.index,
            glyphAdvances: &advance,
            glyphOffsets: std::ptr::null(),
            isSideways: BOOL(0),
            bidiLevel: 0,
        };
        let result = self.rasterize_run(&run, face, antialias);
        drop(ManuallyDrop::into_inner(run.fontFace));
        result
    }

    fn rasterize_run(
        &self,
        run: &DWRITE_GLYPH_RUN,
        face: &IDWriteFontFace,
        antialias: Antialias,
    ) -> Result<Option<Bitmap>> {
        // SAFETY: `run` references a live face and one glyph index; the analysis and texture calls use matching texture types and buffer sizes.
        unsafe {
            let mut mode: DWRITE_RENDERING_MODE = face.GetRecommendedRenderingMode(
                self.size_px,
                1.0,
                DWRITE_MEASURING_MODE_NATURAL,
                &self.rendering_params,
            )?;
            if mode == DWRITE_RENDERING_MODE_OUTLINE {
                mode = DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC;
            }

            if antialias == Antialias::ClearType {
                let analysis = self.factory.CreateGlyphRunAnalysis(
                    run,
                    None,
                    mode,
                    DWRITE_MEASURING_MODE_NATURAL,
                    DWRITE_GRID_FIT_MODE_DEFAULT,
                    DWRITE_TEXT_ANTIALIAS_MODE_CLEARTYPE,
                    0.0,
                    0.0,
                )?;
                let bounds = analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_CLEARTYPE_3x1)?;
                if !is_empty(&bounds) {
                    let (width, height) = rect_size(&bounds);
                    let mut rgb = vec![0u8; (width * height * 3) as usize];
                    analysis.CreateAlphaTexture(DWRITE_TEXTURE_CLEARTYPE_3x1, &bounds, &mut rgb)?;
                    let rgba = rgb
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .flat_map(|&[r, g, b]| [r, g, b, r.max(g).max(b)])
                        .collect();
                    return Ok(Some(bitmap(bounds, rgba)));
                }
            }

            let analysis = self.factory.CreateGlyphRunAnalysis(
                run,
                None,
                mode,
                DWRITE_MEASURING_MODE_NATURAL,
                DWRITE_GRID_FIT_MODE_DEFAULT,
                DWRITE_TEXT_ANTIALIAS_MODE_GRAYSCALE,
                0.0,
                0.0,
            )?;
            let bounds = analysis.GetAlphaTextureBounds(DWRITE_TEXTURE_ALIASED_1x1)?;
            if is_empty(&bounds) {
                return Ok(None);
            }
            let (width, height) = rect_size(&bounds);
            let mut alpha = vec![0u8; (width * height) as usize];
            analysis.CreateAlphaTexture(DWRITE_TEXTURE_ALIASED_1x1, &bounds, &mut alpha)?;
            let rgba = alpha.iter().flat_map(|&a| [a, a, a, a]).collect();
            Ok(Some(bitmap(bounds, rgba)))
        }
    }
}

impl GlyphSource for Fonts {
    type Id = GlyphId;

    fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    fn shading(&self, _id: GlyphId) -> u32 {
        match self.params.antialias {
            Antialias::ClearType => CLEARTYPE,
            Antialias::Grayscale => GRAYSCALE,
        }
    }

    fn lookup(&mut self, text: &str, bold: bool, italic: bool) -> Option<GlyphId> {
        Fonts::lookup(self, text, bold, italic)
    }

    fn rasterize(&mut self, id: GlyphId) -> Option<Bitmap> {
        Fonts::rasterize(self, id, self.params.antialias)
            .ok()
            .flatten()
    }
}

fn bitmap(bounds: RECT, rgba: Vec<u8>) -> Bitmap {
    let (width, height) = rect_size(&bounds);
    Bitmap {
        left: bounds.left,
        top: bounds.top,
        width,
        height,
        rgba,
    }
}

fn is_empty(rect: &RECT) -> bool {
    rect.right <= rect.left || rect.bottom <= rect.top
}

fn rect_size(rect: &RECT) -> (u32, u32) {
    (
        (rect.right - rect.left).max(0) as u32,
        (rect.bottom - rect.top).max(0) as u32,
    )
}

fn bundled(factory: &IDWriteFactory2) -> Option<(IDWriteFontCollection, HSTRING)> {
    BUNDLED_COLLECTION.with(|cell| cell.get_or_init(|| load_bundled(factory)).clone())
}

fn load_bundled(factory: &IDWriteFactory2) -> Option<(IDWriteFontCollection, HSTRING)> {
    // SAFETY: the font data is 'static; the loader copies it (no owner object is passed) and is registered once per thread, since the result is cached.
    unsafe {
        let factory: IDWriteFactory5 = factory.cast().ok()?;
        let loader = factory.CreateInMemoryFontFileLoader().ok()?;
        factory.RegisterFontFileLoader(&loader).ok()?;
        let builder = factory.CreateFontSetBuilder().ok()?;
        for data in BUNDLED {
            let file = loader
                .CreateInMemoryFontFileReference(
                    &factory,
                    data.as_ptr().cast::<c_void>(),
                    data.len() as u32,
                    None,
                )
                .ok()?;
            builder.AddFontFile(&file).ok()?;
        }
        let set = builder.CreateFontSet().ok()?;
        let collection: IDWriteFontCollection = factory
            .CreateFontCollectionFromFontSet(&set)
            .ok()?
            .cast()
            .ok()?;
        let names = collection.GetFontFamily(0).ok()?.GetFamilyNames().ok()?;
        let length = names.GetStringLength(0).ok()? as usize;
        let mut name = vec![0u16; length + 1];
        names.GetString(0, &mut name).ok()?;
        name.truncate(length);
        Some((collection, HSTRING::from_wide(&name)))
    }
}

fn family_index(collection: &IDWriteFontCollection, name: &HSTRING) -> Option<u32> {
    let mut index = 0;
    let mut exists = BOOL(0);
    // SAFETY: `name` is a valid wide string; the out-pointers are valid.
    unsafe { collection.FindFamilyName(name, &mut index, &mut exists) }.ok()?;
    exists.as_bool().then_some(index)
}

fn glyph_index(face: &IDWriteFontFace, c: char) -> Option<u16> {
    let codepoint = c as u32;
    let mut index = 0u16;
    // SAFETY: one codepoint in, one glyph index out.
    unsafe { face.GetGlyphIndices(&codepoint, 1, &mut index) }.ok()?;
    (index != 0).then_some(index)
}

fn configured(system: TextParams, text: &TextConfig) -> TextParams {
    TextParams {
        antialias: match text.antialiasing {
            Antialiasing::System => system.antialias,
            Antialiasing::ClearType => Antialias::ClearType,
            Antialiasing::Grayscale => Antialias::Grayscale,
        },
        gamma: text.gamma.unwrap_or(system.gamma),
        enhanced_contrast: text.contrast.unwrap_or(system.enhanced_contrast),
        grayscale_enhanced_contrast: text.contrast.unwrap_or(system.grayscale_enhanced_contrast),
        cleartype_level: system.cleartype_level,
    }
}

fn system_text_params(params: &IDWriteRenderingParams) -> TextParams {
    let mut smoothing = 0u32;
    // SAFETY: SPI_GETFONTSMOOTHINGTYPE writes one UINT to the pointer passed.
    let cleartype = unsafe {
        SystemParametersInfoW(
            SPI_GETFONTSMOOTHINGTYPE,
            0,
            Some((&raw mut smoothing).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok()
        && smoothing == FE_FONTSMOOTHINGCLEARTYPE;

    // SAFETY: the getters have no preconditions; IDWriteRenderingParams1 exists on Windows 8 and later.
    unsafe {
        let grayscale_enhanced_contrast = params
            .cast::<IDWriteRenderingParams1>()
            .map(|p| p.GetGrayscaleEnhancedContrast())
            .unwrap_or_else(|_| params.GetEnhancedContrast());
        TextParams {
            antialias: if cleartype {
                Antialias::ClearType
            } else {
                Antialias::Grayscale
            },
            gamma: params.GetGamma(),
            enhanced_contrast: params.GetEnhancedContrast(),
            grayscale_enhanced_contrast,
            cleartype_level: params.GetClearTypeLevel(),
        }
    }
}

pub(crate) fn gamma_ratios(gamma: f32) -> [f32; 4] {
    const RATIOS: [[f32; 4]; 13] = [
        [0.0000, 0.0000, 0.0000, 0.0000],
        [0.0166, -0.0807, 0.2227, -0.0751],
        [0.0350, -0.1760, 0.4325, -0.1370],
        [0.0543, -0.2821, 0.6302, -0.1876],
        [0.0739, -0.3963, 0.8167, -0.2287],
        [0.0933, -0.5161, 0.9926, -0.2616],
        [0.1121, -0.6395, 1.1588, -0.2877],
        [0.1300, -0.7649, 1.3159, -0.3080],
        [0.1469, -0.8911, 1.4644, -0.3234],
        [0.1627, -1.0170, 1.6051, -0.3347],
        [0.1773, -1.1420, 1.7385, -0.3426],
        [0.1908, -1.2652, 1.8650, -0.3476],
        [0.2031, -1.3864, 1.9851, -0.3501],
    ];
    let norm13 = (f64::from(0x10000) / (255.0 * 255.0) * 4.0) as f32;
    let norm24 = (f64::from(0x100) / 255.0 * 4.0) as f32;
    let index = ((gamma * 10.0 + 0.5) as i32).clamp(10, 22) - 10;
    let r = RATIOS[index as usize];
    [
        norm13 * r[0] / 4.0,
        norm24 * r[1] / 4.0,
        norm13 * r[2] / 4.0,
        norm24 * r[3] / 4.0,
    ]
}

#[implement(IDWriteTextAnalysisSource)]
struct AnalysisSource {
    text: Vec<u16>,
}

impl AnalysisSource {
    fn new(text: &str) -> AnalysisSource {
        AnalysisSource {
            text: text.encode_utf16().collect(),
        }
    }
}

impl IDWriteTextAnalysisSource_Impl for AnalysisSource_Impl {
    fn GetTextAtPosition(
        &self,
        position: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> Result<()> {
        let position = position as usize;
        let (ptr, len) = if position < self.text.len() {
            (self.text[position..].as_ptr(), self.text.len() - position)
        } else {
            (std::ptr::null(), 0)
        };
        // SAFETY: DirectWrite passes valid out-pointers; `self.text` outlives the analysis.
        unsafe {
            *text = ptr.cast_mut();
            *length = len as u32;
        }
        Ok(())
    }

    fn GetTextBeforePosition(
        &self,
        position: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> Result<()> {
        let position = position as usize;
        let (ptr, len) = if position > 0 && position <= self.text.len() {
            (self.text.as_ptr(), position)
        } else {
            (std::ptr::null(), 0)
        };
        // SAFETY: DirectWrite passes valid out-pointers; `self.text` outlives the analysis.
        unsafe {
            *text = ptr.cast_mut();
            *length = len as u32;
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
    }

    fn GetLocaleName(&self, position: u32, length: *mut u32, locale: *mut *mut u16) -> Result<()> {
        // SAFETY: DirectWrite passes valid out-pointers; the locale literal is static.
        unsafe {
            *length = (self.text.len() as u32).saturating_sub(position);
            *locale = PCWSTR(w!("en-us").as_ptr()).0.cast_mut();
        }
        Ok(())
    }

    fn GetNumberSubstitution(
        &self,
        position: u32,
        length: *mut u32,
        substitution: OutRef<IDWriteNumberSubstitution>,
    ) -> Result<()> {
        // SAFETY: DirectWrite passes a valid out-pointer for the length.
        unsafe { *length = (self.text.len() as u32).saturating_sub(position) };
        substitution.write(None)
    }
}

#[cfg(test)]
mod tests {
    use optimum_core::Config;

    use super::*;

    fn defaults() -> Config {
        Config::default()
    }

    #[test]
    fn config_overrides_system_text_params() {
        let system = TextParams {
            antialias: Antialias::ClearType,
            gamma: 1.8,
            enhanced_contrast: 0.5,
            grayscale_enhanced_contrast: 1.0,
            cleartype_level: 1.0,
        };
        assert_eq!(configured(system, &defaults().text), system);
        let text = TextConfig {
            antialiasing: Antialiasing::Grayscale,
            gamma: Some(2.0),
            contrast: Some(3.0),
        };
        let p = configured(system, &text);
        assert_eq!(p.antialias, Antialias::Grayscale);
        assert_eq!(
            (p.gamma, p.enhanced_contrast, p.grayscale_enhanced_contrast),
            (2.0, 3.0, 3.0)
        );
    }

    #[test]
    fn bundled_font_loads_and_covers_nerd_icons() {
        // SAFETY: creating the shared DirectWrite factory has no preconditions.
        let factory: IDWriteFactory2 =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.unwrap();
        let (_, name) = bundled(&factory).expect("bundled font");
        assert!(name.to_string().starts_with("JetBrainsMono"), "{name}");

        let mut font = defaults().font;
        font.family = "No Such Font 123".to_owned();
        let mut fonts = Fonts::new(96, &font, &defaults().text).unwrap();
        for icon in ["\u{e0b0}", "\u{f121}", "\u{f418}", "a"] {
            assert_eq!(
                fonts.lookup(icon, false, false).map(|g| g.face),
                Some(0),
                "{icon:?}"
            );
        }
    }

    #[test]
    fn line_height_scales_cells() {
        let mut font = defaults().font;
        let normal = Fonts::new(96, &font, &defaults().text).unwrap().metrics;
        font.line_height = 1.5;
        let tall = Fonts::new(96, &font, &defaults().text).unwrap().metrics;
        assert_eq!(tall.width, normal.width);
        assert!(tall.height > normal.height);
        font.family = "No Such Font 123".to_owned();
        assert!(Fonts::new(96, &font, &defaults().text).is_ok());
    }

    #[test]
    fn default_gamma_matches_directwrite_constants() {
        let r = gamma_ratios(1.8);
        let expected = [0.148_054_42, -0.894_594_55, 1.475_908, -0.324_668_26];
        for (a, b) in r.iter().zip(expected) {
            assert!((a - b).abs() < 1e-5, "{r:?}");
        }
    }

    #[test]
    fn metrics_are_whole_pixels() {
        let fonts = Fonts::new(96, &defaults().font, &defaults().text).unwrap();
        let m = fonts.metrics;
        assert!(m.width >= 6 && m.width <= 12, "{m:?}");
        assert!(m.height >= 12 && m.height <= 24, "{m:?}");
        assert!(m.baseline > 0 && (m.baseline as u32) < m.height, "{m:?}");
    }

    #[test]
    fn glyphs_resolve_and_rasterize() {
        let mut fonts = Fonts::new(96, &defaults().font, &defaults().text).unwrap();
        let a = fonts.lookup("A", false, false).unwrap();
        assert_eq!(a.face, 0);
        let bitmap = fonts.rasterize(a, Antialias::Grayscale).unwrap().unwrap();
        assert!(bitmap.width > 0 && bitmap.height > 0);
        assert!(bitmap.top < 0, "glyph should sit above the baseline");
        assert!(bitmap.rgba.iter().any(|&v| v > 200));

        let ct = fonts.rasterize(a, Antialias::ClearType).unwrap().unwrap();
        assert_eq!(ct.rgba.len(), (ct.width * ct.height * 4) as usize);

        assert!(fonts.lookup(" ", false, false).is_some());
        let space = fonts.lookup(" ", false, false).unwrap();
        assert!(
            fonts
                .rasterize(space, Antialias::Grayscale)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn missing_glyphs_use_system_fallback() {
        let mut fonts = Fonts::new(96, &defaults().font, &defaults().text).unwrap();
        let cjk = fonts.lookup("日", false, false).unwrap();
        assert_ne!(
            cjk.face, 0,
            "the bundled font has no CJK; fallback expected"
        );
        assert!(
            fonts
                .rasterize(cjk, Antialias::Grayscale)
                .unwrap()
                .is_some()
        );
    }
}

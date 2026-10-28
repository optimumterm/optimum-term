use std::ffi::c_void;
use std::mem::size_of;

use optimum_core::{CellSize, Config, CursorStyle, Frame, Rgb, Row, Theme, codepoint_width, scale};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_10_0,
    D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
    D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP, ID3DBlob,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
    D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC1_ALPHA, D3D11_BLEND_INV_SRC1_COLOR, D3D11_BLEND_ONE,
    D3D11_BLEND_OP_ADD, D3D11_BOX, D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL,
    D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CULL_NONE, D3D11_FILL_SOLID,
    D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_INSTANCE_DATA, D3D11_INPUT_PER_VERTEX_DATA,
    D3D11_MAP_WRITE_DISCARD, D3D11_MAPPED_SUBRESOURCE, D3D11_RASTERIZER_DESC,
    D3D11_RENDER_TARGET_BLEND_DESC, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE,
    D3D11_VIEWPORT, D3D11CreateDevice, ID3D11BlendState, ID3D11Buffer, ID3D11Device,
    ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
    ID3D11RenderTargetView, ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM,
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R16G16_SINT, DXGI_FORMAT_R16G16_UINT,
    DXGI_FORMAT_R32_UINT, DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIDevice, IDXGIFactory2,
    IDXGISwapChain1,
};
use windows::core::{Interface, PCSTR, Result, s};

use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};

use crate::atlas::{ATLAS_SIZE, Atlas, Slot};
use crate::font::{Antialias, CellMetrics, Fonts, gamma_ratios};

const SHADER: &str = include_str!("shader.hlsl");
const SOLID: u32 = 0;
const GRAYSCALE: u32 = 1;
const CLEARTYPE: u32 = 2;
const NO_COLUMN: u16 = u16::MAX;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Instance {
    position: [i16; 2],
    size: [u16; 2],
    texcoord: [u16; 2],
    color: u32,
    shading: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Constants {
    position_scale: [f32; 2],
    padding0: [f32; 2],
    gamma_ratios: [f32; 4],
    enhanced_contrast: f32,
    grayscale_enhanced_contrast: f32,
    cleartype_level: f32,
    padding1: f32,
}

#[derive(Default)]
struct RowCache {
    backgrounds: Vec<Instance>,
    glyphs: Vec<Instance>,
    glyph_columns: Vec<u16>,
}

struct AtlasFull;

#[derive(Debug, Clone, Copy)]
pub(crate) struct View<'a> {
    pub focused: bool,
    pub preedit: &'a str,
    pub overlay: &'a [String],
    pub error: Option<&'a str>,
    pub cursor_on: bool,
}

struct Pipeline {
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    quad: ID3D11Buffer,
    constants: ID3D11Buffer,
    blend: ID3D11BlendState,
    rasterizer: ID3D11RasterizerState,
    atlas_texture: ID3D11Texture2D,
    atlas_view: ID3D11ShaderResourceView,
}

pub(crate) struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    _composition: Composition,
    opacity: f32,
    target: Option<ID3D11RenderTargetView>,
    pipeline: Pipeline,
    instances: Option<ID3D11Buffer>,
    instance_capacity: usize,
    atlas: Atlas,
    fonts: Fonts,
    dpi: u32,
    padding: u32,
    font: optimum_core::FontConfig,
    text: optimum_core::TextConfig,
    width: u32,
    height: u32,
    rows: Vec<RowCache>,
    scratch: Vec<Instance>,
}

impl Renderer {
    pub fn new(hwnd: HWND, width: u32, height: u32, dpi: u32, config: &Config) -> Result<Renderer> {
        let (device, context) = create_device(D3D_DRIVER_TYPE_HARDWARE)
            .or_else(|_| create_device(D3D_DRIVER_TYPE_WARP))?;
        let (swap_chain, composition) = create_swap_chain(&device, hwnd, width, height)?;
        let pipeline = create_pipeline(&device)?;
        let mut renderer = Renderer {
            device,
            context,
            swap_chain,
            _composition: composition,
            opacity: config.window.opacity,
            target: None,
            pipeline,
            instances: None,
            instance_capacity: 0,
            atlas: Atlas::new(),
            fonts: Fonts::new(dpi, &config.font, &text_for(config))?,
            dpi,
            padding: config.window.padding,
            font: config.font.clone(),
            text: text_for(config),
            width: width.max(1),
            height: height.max(1),
            rows: Vec::new(),
            scratch: Vec::new(),
        };
        renderer.create_target()?;
        Ok(renderer)
    }

    pub fn cell(&self) -> CellSize {
        CellSize {
            width: self.fonts.metrics.width,
            height: self.fonts.metrics.height,
        }
    }

    pub fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        if dpi == self.dpi {
            return Ok(());
        }
        self.fonts = Fonts::new(dpi, &self.font, &self.text)?;
        self.dpi = dpi;
        self.atlas.clear();
        self.rows.clear();
        Ok(())
    }

    pub fn configure(&mut self, config: &Config) -> Result<()> {
        self.padding = config.window.padding;
        self.opacity = config.window.opacity;
        let text = text_for(config);
        if config.font != self.font || text != self.text {
            self.fonts = Fonts::new(self.dpi, &config.font, &text)?;
            self.font = config.font.clone();
            self.text = text;
            self.atlas.clear();
        }
        self.rows.clear();
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub fn text_params(&self) -> crate::font::TextParams {
        self.fonts.params
    }

    #[cfg(debug_assertions)]
    pub fn set_text_params(&mut self, params: crate::font::TextParams) {
        if params.antialias != self.fonts.params.antialias {
            self.rows.clear();
        }
        self.fonts.params = params;
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.target = None;
        // SAFETY: the render target view was released and unbound, so no references to the back buffers remain.
        unsafe {
            self.context.OMSetRenderTargets(None, None);
            self.swap_chain.ResizeBuffers(
                0,
                width,
                height,
                DXGI_FORMAT_UNKNOWN,
                DXGI_SWAP_CHAIN_FLAG(0),
            )?;
        }
        self.width = width;
        self.height = height;
        self.create_target()
    }

    pub fn render(
        &mut self,
        frame: Option<&mut Frame>,
        theme: Theme,
        view: View<'_>,
    ) -> Result<()> {
        self.scratch.clear();
        let (foreground, background) = match frame {
            Some(frame) => {
                self.build(frame);
                self.assemble(frame, view);
                (frame.foreground, frame.background)
            }
            None => (theme.foreground, theme.background),
        };
        if !view.overlay.is_empty() {
            self.overlay(view.overlay, foreground, background);
        }
        if let Some(error) = view.error {
            self.error_line(error, foreground, background.mix(theme.palette[1], 0.45));
        }
        self.upload_glyphs();
        self.draw(background)
    }

    pub fn overlay_columns(&self) -> usize {
        (self.width / self.fonts.metrics.width.max(1)).saturating_sub(8) as usize
    }

    fn error_line(&mut self, text: &str, foreground: Rgb, background: Rgb) {
        let m = self.fonts.metrics;
        let top = self.height as i32 - m.height as i32;
        self.scratch
            .push(solid(0, top, self.width, m.height, background));
        let mut x = m.width as i32;
        for c in text.chars() {
            let span = u32::from(codepoint_width(c));
            if span == 0 {
                continue;
            }
            if x + (span * m.width) as i32 > self.width as i32 {
                break;
            }
            let mut buf = [0u8; 4];
            if let Ok(Some(glyph)) =
                self.glyph(c.encode_utf8(&mut buf), false, false, x, top, foreground)
            {
                self.scratch.push(glyph);
            }
            x += (span * m.width) as i32;
        }
    }

    fn overlay(&mut self, lines: &[String], foreground: Rgb, background: Rgb) {
        let m = self.fonts.metrics;
        let cells =
            |line: &str| -> u32 { line.chars().map(|c| u32::from(codepoint_width(c))).sum() };
        let widest = lines.iter().map(|l| cells(l)).max().unwrap_or(0);
        let width = (widest + 4) * m.width;
        let height = (lines.len() as u32 + 2) * m.height;
        let left = ((self.width as i32 - width as i32) / 2).max(0);
        let top = ((self.height as i32 - height as i32) / 2).max(0);
        let border = scale(1, self.dpi).max(1);
        self.scratch.push(solid(
            left,
            top,
            width,
            height,
            background.mix(foreground, 0.35),
        ));
        self.scratch.push(solid(
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
                if let Ok(Some(glyph)) =
                    self.glyph(c.encode_utf8(&mut buf), false, false, x, y, foreground)
                {
                    self.scratch.push(glyph);
                }
                x += (span * m.width) as i32;
            }
        }
    }

    fn build(&mut self, frame: &mut Frame) {
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
                let result = self.build_row(&mut cache, y, row, frame.background);
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

    fn build_row(
        &mut self,
        cache: &mut RowCache,
        y: usize,
        row: &Row,
        default_background: Rgb,
    ) -> std::result::Result<(), AtlasFull> {
        cache.backgrounds.clear();
        cache.glyphs.clear();
        cache.glyph_columns.clear();

        let m = self.fonts.metrics;
        let padding = scale(self.padding, self.dpi) as i32;
        let top = padding + y as i32 * m.height as i32;

        for (column, cell) in row.cells.iter().enumerate() {
            let left = padding + column as i32 * m.width as i32;
            let span = m.width * u32::from(cell.width.max(1));

            if let Some(bg) = cell.background
                && bg != default_background
                && cell.width > 0
            {
                let color = pack_alpha(bg, self.opacity);
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

    fn glyph(
        &mut self,
        text: &str,
        bold: bool,
        italic: bool,
        left: i32,
        top: i32,
        color: Rgb,
    ) -> std::result::Result<Option<Instance>, AtlasFull> {
        let antialias = self.fonts.params.antialias;
        let Some(id) = self.fonts.lookup(text, bold, italic) else {
            return Ok(None);
        };
        let entry = match self.atlas.get(id, antialias) {
            Some(entry) => entry,
            None => {
                let bitmap = self.fonts.rasterize(id, antialias).ok().flatten();
                match self.atlas.insert(id, antialias, bitmap) {
                    Slot::Cached(entry) => entry,
                    Slot::Full => return Err(AtlasFull),
                }
            }
        };
        let Some(g) = entry else {
            return Ok(None);
        };
        let shading = if antialias == Antialias::ClearType {
            CLEARTYPE
        } else {
            GRAYSCALE
        };
        Ok(Some(Instance {
            position: [
                (left + g.left) as i16,
                (top + self.fonts.metrics.baseline + g.top) as i16,
            ],
            size: [g.width, g.height],
            texcoord: [g.x, g.y],
            color: pack(color),
            shading,
        }))
    }

    fn assemble(&mut self, frame: &Frame, view: View<'_>) {
        for row in &self.rows {
            self.scratch.extend_from_slice(&row.backgrounds);
        }

        let m = self.fonts.metrics;
        let padding = scale(self.padding, self.dpi) as i32;
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
            self.scratch.extend(cursor_quads(
                cursor_rect(cursor, m, padding),
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
                {
                    glyph.color = pack(frame.background);
                }
                self.scratch.push(glyph);
            }
        }

        if let (true, Some(cursor)) = (composing, frame.cursor) {
            self.preedit(view.preedit, cursor, frame);
        }
    }

    fn preedit(&mut self, text: &str, cursor: optimum_core::FrameCursor, frame: &Frame) {
        let m = self.fonts.metrics;
        let padding = scale(self.padding, self.dpi) as i32;
        let start = padding + i32::from(cursor.x) * m.width as i32;
        let top = padding + i32::from(cursor.y) * m.height as i32;
        let mut left = start;
        let mut glyphs = Vec::new();
        for c in text.chars() {
            let width = u32::from(codepoint_width(c));
            if width == 0 {
                continue;
            }
            let mut buf = [0u8; 4];
            if let Ok(Some(glyph)) = self.glyph(
                c.encode_utf8(&mut buf),
                false,
                false,
                left,
                top,
                frame.foreground,
            ) {
                glyphs.push(glyph);
            }
            left += (m.width * width) as i32;
        }
        let span = (left - start) as u32;
        let thickness = m.underline_thickness.max(1);
        self.scratch
            .push(solid(start, top, span, m.height, frame.background));
        self.scratch.extend(glyphs);
        self.scratch.push(solid(
            start,
            top + m.underline_position,
            span,
            thickness,
            frame.foreground,
        ));
        self.scratch
            .push(solid(left, top, thickness, m.height, cursor.color));
    }

    fn upload_glyphs(&mut self) {
        for upload in self.atlas.take_uploads() {
            let b = &upload.bitmap;
            let region = D3D11_BOX {
                left: upload.x,
                top: upload.y,
                front: 0,
                right: upload.x + b.width,
                bottom: upload.y + b.height,
                back: 1,
            };
            // SAFETY: `region` lies inside the atlas texture and `b.rgba` holds width * height RGBA texels.
            unsafe {
                self.context.UpdateSubresource(
                    &self.pipeline.atlas_texture,
                    0,
                    Some(&region),
                    b.rgba.as_ptr().cast::<c_void>(),
                    b.width * 4,
                    0,
                );
            }
        }
    }

    fn draw(&mut self, clear: Rgb) -> Result<()> {
        let Some(target) = self.target.clone() else {
            return Ok(());
        };
        let count = self.scratch.len();
        if count > 0 {
            self.ensure_instance_capacity(count)?;
        }
        let params = self.fonts.params;
        let constants = Constants {
            position_scale: [2.0 / self.width as f32, -2.0 / self.height as f32],
            gamma_ratios: gamma_ratios(params.gamma),
            enhanced_contrast: params.enhanced_contrast,
            grayscale_enhanced_contrast: params.grayscale_enhanced_contrast,
            cleartype_level: params.cleartype_level.clamp(0.0, 1.0),
            ..Default::default()
        };
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: self.width as f32,
            Height: self.height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let p = &self.pipeline;
        // SAFETY: every bound resource was created on `self.device`; the instance buffer holds at least `count` instances, written just before drawing.
        unsafe {
            if count > 0 {
                let instances = self.instances.as_ref().unwrap();
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context
                    .Map(instances, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
                std::ptr::copy_nonoverlapping(
                    self.scratch.as_ptr(),
                    mapped.pData.cast::<Instance>(),
                    count,
                );
                self.context.Unmap(instances, 0);
            }
            self.context.UpdateSubresource(
                &p.constants,
                0,
                None,
                (&raw const constants).cast::<c_void>(),
                0,
                0,
            );

            self.context.IASetInputLayout(&p.layout);
            self.context
                .IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            let buffers = [Some(p.quad.clone()), self.instances.clone()];
            let strides = [8u32, size_of::<Instance>() as u32];
            let offsets = [0u32, 0];
            self.context.IASetVertexBuffers(
                0,
                2,
                Some(buffers.as_ptr()),
                Some(strides.as_ptr()),
                Some(offsets.as_ptr()),
            );
            self.context.VSSetShader(&p.vertex_shader, None);
            self.context
                .VSSetConstantBuffers(0, Some(&[Some(p.constants.clone())]));
            self.context.PSSetShader(&p.pixel_shader, None);
            self.context
                .PSSetConstantBuffers(0, Some(&[Some(p.constants.clone())]));
            self.context
                .PSSetShaderResources(0, Some(&[Some(p.atlas_view.clone())]));
            self.context.OMSetBlendState(&p.blend, None, u32::MAX);
            self.context.RSSetState(&p.rasterizer);
            self.context.RSSetViewports(Some(&[viewport]));
            self.context
                .OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            let [r, g, b, _] = clear.to_f32();
            let a = self.opacity;
            self.context
                .ClearRenderTargetView(&target, &[r * a, g * a, b * a, a]);
            if count > 0 {
                self.context.DrawInstanced(4, count as u32, 0, 0);
            }
            self.swap_chain.Present(1, DXGI_PRESENT(0)).ok()
        }
    }

    fn ensure_instance_capacity(&mut self, count: usize) -> Result<()> {
        if count <= self.instance_capacity {
            return Ok(());
        }
        let capacity = count.next_power_of_two().max(1024);
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (capacity * size_of::<Instance>()) as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut buffer = None;
        // SAFETY: `desc` describes a valid dynamic vertex buffer.
        unsafe { self.device.CreateBuffer(&desc, None, Some(&mut buffer))? };
        self.instances = buffer;
        self.instance_capacity = capacity;
        Ok(())
    }

    fn create_target(&mut self) -> Result<()> {
        // SAFETY: buffer 0 of a flip-model swap chain is a 2D texture usable as a render target.
        let target = unsafe {
            let buffer: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let mut target = None;
            self.device
                .CreateRenderTargetView(&buffer, None, Some(&mut target))?;
            target
        };
        self.target = target;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

fn cursor_rect(cursor: optimum_core::FrameCursor, m: CellMetrics, padding: i32) -> Rect {
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

fn pack(c: Rgb) -> u32 {
    pack_alpha(c, 1.0)
}

fn pack_alpha(c: Rgb, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let channel = |v: u8| (f32::from(v) * a).round() as u32;
    channel(c.r) | channel(c.g) << 8 | channel(c.b) << 16 | ((a * 255.0).round() as u32) << 24
}

fn create_device(driver: D3D_DRIVER_TYPE) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let levels = [
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];
    let mut device = None;
    let mut context = None;
    // SAFETY: all out-pointers are valid; no adapter or software module is passed.
    unsafe {
        D3D11CreateDevice(
            None,
            driver,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    Ok((device.unwrap(), context.unwrap()))
}

struct Composition {
    _device: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
}

fn create_swap_chain(
    device: &ID3D11Device,
    hwnd: HWND,
    width: u32,
    height: u32,
) -> Result<(IDXGISwapChain1, Composition)> {
    let desc = DXGI_SWAP_CHAIN_DESC1 {
        Width: width.max(1),
        Height: height.max(1),
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        ..Default::default()
    };
    // SAFETY: `device` is a live D3D11 device (every one implements IDXGIDevice); `hwnd` is a live window owned by this thread, created with WS_EX_NOREDIRECTIONBITMAP so DirectComposition supplies its content.
    unsafe {
        let dxgi = device.cast::<IDXGIDevice>()?;
        let factory: IDXGIFactory2 = dxgi.GetAdapter()?.GetParent()?;
        let swap_chain = factory.CreateSwapChainForComposition(device, &desc, None)?;
        let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi)?;
        let target = composition.CreateTargetForHwnd(hwnd, true)?;
        let visual = composition.CreateVisual()?;
        visual.SetContent(&swap_chain)?;
        target.SetRoot(&visual)?;
        composition.Commit()?;
        Ok((
            swap_chain,
            Composition {
                _device: composition,
                _target: target,
                _visual: visual,
            },
        ))
    }
}

fn text_for(config: &Config) -> optimum_core::TextConfig {
    let mut text = config.text;
    if config.window.opacity < 1.0 {
        text.antialiasing = optimum_core::Antialiasing::Grayscale;
    }
    text
}

fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob> {
    let mut code = None;
    let mut errors = None;
    // SAFETY: the source pointer and length describe SHADER; entry and target are NUL-terminated literals.
    let result = unsafe {
        D3DCompile(
            SHADER.as_ptr().cast::<c_void>(),
            SHADER.len(),
            s!("shader.hlsl"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    match (result, code) {
        (Ok(()), Some(code)) => Ok(code),
        (Err(e), _) => Err(e),
        (Ok(()), None) => Err(windows::core::Error::empty()),
    }
}

fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: a blob's pointer is valid for its reported size while the blob is alive.
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}

fn element(
    name: PCSTR,
    format: DXGI_FORMAT,
    slot: u32,
    offset: u32,
    per_instance: bool,
) -> D3D11_INPUT_ELEMENT_DESC {
    D3D11_INPUT_ELEMENT_DESC {
        SemanticName: name,
        SemanticIndex: 0,
        Format: format,
        InputSlot: slot,
        AlignedByteOffset: offset,
        InputSlotClass: if per_instance {
            D3D11_INPUT_PER_INSTANCE_DATA
        } else {
            D3D11_INPUT_PER_VERTEX_DATA
        },
        InstanceDataStepRate: u32::from(per_instance),
    }
}

fn create_pipeline(device: &ID3D11Device) -> Result<Pipeline> {
    let vs = compile(s!("vs_main"), s!("vs_4_0"))?;
    let ps = compile(s!("ps_main"), s!("ps_4_0"))?;
    let elements = [
        element(s!("POSITION"), DXGI_FORMAT_R32G32_FLOAT, 0, 0, false),
        element(s!("QUADPOS"), DXGI_FORMAT_R16G16_SINT, 1, 0, true),
        element(s!("QUADSIZE"), DXGI_FORMAT_R16G16_UINT, 1, 4, true),
        element(s!("TEXCOORD"), DXGI_FORMAT_R16G16_UINT, 1, 8, true),
        element(s!("COLOR"), DXGI_FORMAT_R8G8B8A8_UNORM, 1, 12, true),
        element(s!("SHADING"), DXGI_FORMAT_R32_UINT, 1, 16, true),
    ];
    let quad: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];

    let target_blend = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        SrcBlend: D3D11_BLEND_ONE,
        DestBlend: D3D11_BLEND_INV_SRC1_COLOR,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ONE,
        DestBlendAlpha: D3D11_BLEND_INV_SRC1_ALPHA,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut blend_desc = D3D11_BLEND_DESC::default();
    blend_desc.RenderTarget[0] = target_blend;

    let raster_desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        DepthClipEnable: true.into(),
        ..Default::default()
    };

    let atlas_desc = D3D11_TEXTURE2D_DESC {
        Width: ATLAS_SIZE,
        Height: ATLAS_SIZE,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };

    // SAFETY: all descriptors are fully initialized and the shader bytecode comes from successful compiles.
    unsafe {
        let mut vertex_shader = None;
        device.CreateVertexShader(blob_bytes(&vs), None, Some(&mut vertex_shader))?;
        let mut pixel_shader = None;
        device.CreatePixelShader(blob_bytes(&ps), None, Some(&mut pixel_shader))?;
        let mut layout = None;
        device.CreateInputLayout(&elements, blob_bytes(&vs), Some(&mut layout))?;

        let quad_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&quad) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        };
        let quad_data = D3D11_SUBRESOURCE_DATA {
            pSysMem: quad.as_ptr().cast::<c_void>(),
            ..Default::default()
        };
        let mut quad_buffer = None;
        device.CreateBuffer(&quad_desc, Some(&quad_data), Some(&mut quad_buffer))?;

        let constants_desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of::<Constants>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut constants = None;
        device.CreateBuffer(&constants_desc, None, Some(&mut constants))?;

        let mut blend = None;
        device.CreateBlendState(&blend_desc, Some(&mut blend))?;
        let mut rasterizer = None;
        device.CreateRasterizerState(&raster_desc, Some(&mut rasterizer))?;

        let mut atlas_texture = None;
        device.CreateTexture2D(&atlas_desc, None, Some(&mut atlas_texture))?;
        let atlas_texture = atlas_texture.unwrap();
        let mut atlas_view = None;
        device.CreateShaderResourceView(&atlas_texture, None, Some(&mut atlas_view))?;

        Ok(Pipeline {
            vertex_shader: vertex_shader.unwrap(),
            pixel_shader: pixel_shader.unwrap(),
            layout: layout.unwrap(),
            quad: quad_buffer.unwrap(),
            constants: constants.unwrap(),
            blend: blend.unwrap(),
            rasterizer: rasterizer.unwrap(),
            atlas_texture,
            atlas_view: atlas_view.unwrap(),
        })
    }
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

    #[test]
    fn instance_layout_matches_input_layout() {
        assert_eq!(size_of::<Instance>(), 20);
        assert_eq!(size_of::<Constants>() % 16, 0);
    }

    #[test]
    fn shaders_compile() {
        assert!(compile(s!("vs_main"), s!("vs_4_0")).is_ok());
        assert!(compile(s!("ps_main"), s!("ps_4_0")).is_ok());
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
}

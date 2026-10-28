use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::CGSize;
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBuffer, MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLLibrary, MTLLoadAction, MTLOrigin, MTLPixelFormat,
    MTLPrimitiveType, MTLRegion, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLResourceOptions, MTLSize,
    MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use optimum_core::{
    ATLAS_SIZE, CellSize, Config, FontConfig, Frame, GlyphSource, Instance, Layout, Rgb, Scene,
    Theme, View, overlay_columns,
};

use crate::font::{Fonts, GlyphId};

const SHADER: &str = include_str!("shader.metal");

pub(crate) struct Renderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    atlas: Retained<ProtocolObject<dyn MTLTexture>>,
    layer: Retained<CAMetalLayer>,
    scene: Scene<GlyphId>,
    fonts: Fonts,
    font: FontConfig,
    scale: f64,
    padding: u32,
    opacity: f32,
    width: u32,
    height: u32,
}

impl Renderer {
    pub fn new(
        layer: Retained<CAMetalLayer>,
        width: u32,
        height: u32,
        scale: f64,
        config: &Config,
    ) -> Option<Renderer> {
        let device = MTLCreateSystemDefaultDevice()?;
        let queue = device.newCommandQueue()?;
        let pipeline = pipeline(&device)?;
        let atlas = atlas(&device)?;
        layer.setDevice(Some(&device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        // SAFETY: kCGColorSpaceSRGB is a valid colour space name constant.
        let srgb = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }));
        layer.setColorspace(srgb.as_deref());
        let fonts = Fonts::new(scale, &config.font)?;
        let mut renderer = Renderer {
            device,
            queue,
            pipeline,
            atlas,
            layer,
            scene: Scene::default(),
            fonts,
            font: config.font.clone(),
            scale,
            padding: config.window.padding,
            opacity: config.window.opacity,
            width: 1,
            height: 1,
        };
        renderer.apply_opacity();
        renderer.resize(width, height);
        Some(renderer)
    }

    pub fn cell(&self) -> CellSize {
        let m = self.fonts.metrics();
        CellSize {
            width: m.width,
            height: m.height,
        }
    }

    pub fn padding(&self) -> u32 {
        (f64::from(self.padding) * self.scale).round() as u32
    }

    pub fn set_scale(&mut self, scale: f64) {
        if scale == self.scale {
            return;
        }
        if let Some(fonts) = Fonts::new(scale, &self.font) {
            self.fonts = fonts;
            self.scale = scale;
            self.layer.setContentsScale(scale);
            self.scene.reset();
        }
    }

    pub fn configure(&mut self, config: &Config) {
        self.padding = config.window.padding;
        self.opacity = config.window.opacity;
        if config.font != self.font
            && let Some(fonts) = Fonts::new(self.scale, &config.font)
        {
            self.fonts = fonts;
            self.font = config.font.clone();
            self.scene.reset();
        }
        self.apply_opacity();
        self.scene.invalidate();
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.layer
            .setDrawableSize(CGSize::new(f64::from(self.width), f64::from(self.height)));
    }

    pub fn overlay_columns(&self) -> usize {
        overlay_columns(self.width, self.fonts.metrics())
    }

    pub fn render(&mut self, frame: Option<&mut Frame>, theme: Theme, view: View<'_>) {
        let layout = Layout {
            width: self.width,
            height: self.height,
            padding: self.padding() as i32,
            border: self.scale.round().max(1.0) as u32,
            opacity: self.opacity,
        };
        let background = self
            .scene
            .compose(&mut self.fonts, layout, frame, theme, view);
        self.upload_glyphs();
        self.draw(background);
    }

    fn apply_opacity(&self) {
        self.layer.setOpaque(self.opacity >= 1.0);
    }

    fn upload_glyphs(&mut self) {
        for upload in self.scene.take_uploads() {
            let b = &upload.bitmap;
            let region = MTLRegion {
                origin: MTLOrigin {
                    x: upload.x as usize,
                    y: upload.y as usize,
                    z: 0,
                },
                size: MTLSize {
                    width: b.width as usize,
                    height: b.height as usize,
                    depth: 1,
                },
            };
            let Some(bytes) = NonNull::new(b.rgba.as_ptr().cast_mut().cast::<c_void>()) else {
                continue;
            };
            // SAFETY: `region` lies inside the atlas and `b.rgba` holds width * height RGBA texels.
            unsafe {
                self.atlas.replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    region,
                    0,
                    bytes,
                    b.width as usize * 4,
                );
            }
        }
    }

    fn draw(&mut self, clear: Rgb) {
        let Some(drawable) = self.layer.nextDrawable() else {
            return;
        };
        let Some(commands) = self.queue.commandBuffer() else {
            return;
        };
        let pass = MTLRenderPassDescriptor::renderPassDescriptor();
        let [r, g, b, _] = clear.to_f32();
        let a = self.opacity.clamp(0.0, 1.0);
        // SAFETY: index 0 always exists in a render pass descriptor's colour attachments.
        let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&drawable.texture()));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor {
            red: f64::from(r * a),
            green: f64::from(g * a),
            blue: f64::from(b * a),
            alpha: f64::from(a),
        });
        let Some(encoder) = commands.renderCommandEncoderWithDescriptor(&pass) else {
            return;
        };

        let instances = self.scene.instances();
        let uniforms = [2.0 / self.width as f32, -2.0 / self.height as f32];
        encoder.setRenderPipelineState(&self.pipeline);
        if !instances.is_empty()
            && let Some(buffer) = self.instance_buffer(instances)
        {
            // SAFETY: the buffer holds `instances.len()` instances laid out as the shader's `Instance`; the uniforms are two floats read as `Uniforms`.
            unsafe {
                encoder.setVertexBuffer_offset_atIndex(Some(&buffer), 0, 0);
                encoder.setVertexBytes_length_atIndex(
                    NonNull::from(&uniforms).cast(),
                    size_of_val(&uniforms),
                    1,
                );
                encoder.setFragmentTexture_atIndex(Some(&self.atlas), 0);
                encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    instances.len(),
                );
            }
        }
        encoder.endEncoding();
        commands.presentDrawable(ProtocolObject::from_ref(&*drawable));
        commands.commit();
    }

    fn instance_buffer(
        &self,
        instances: &[Instance],
    ) -> Option<Retained<ProtocolObject<dyn MTLBuffer>>> {
        let bytes = NonNull::new(instances.as_ptr().cast_mut().cast::<c_void>())?;
        // SAFETY: `bytes` points to `size_of_val(instances)` readable bytes, copied by Metal before returning.
        unsafe {
            self.device.newBufferWithBytes_length_options(
                bytes,
                size_of_val(instances),
                MTLResourceOptions::StorageModeShared,
            )
        }
    }
}

fn pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let library = device
        .newLibraryWithSource_options_error(&NSString::from_str(SHADER), None)
        .ok()?;
    let vertex = library.newFunctionWithName(&NSString::from_str("vs_main"))?;
    let fragment = library.newFunctionWithName(&NSString::from_str("fs_main"))?;
    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setVertexFunction(Some(&vertex));
    descriptor.setFragmentFunction(Some(&fragment));
    // SAFETY: index 0 always exists in a pipeline descriptor's colour attachments.
    let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    attachment.setBlendingEnabled(true);
    attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
    attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .ok()
}

fn atlas(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Option<Retained<ProtocolObject<dyn MTLTexture>>> {
    // SAFETY: a plain 2D RGBA8 texture description without mipmaps.
    let descriptor = unsafe {
        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
            MTLPixelFormat::RGBA8Unorm,
            ATLAS_SIZE as usize,
            ATLAS_SIZE as usize,
            false,
        )
    };
    descriptor.setUsage(MTLTextureUsage::ShaderRead);
    device.newTextureWithDescriptor(&descriptor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_compiles() {
        let Some(device) = MTLCreateSystemDefaultDevice() else {
            return;
        };
        assert!(pipeline(&device).is_some());
    }
}

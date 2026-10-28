use std::ffi::c_void;
use std::mem::size_of;

use optimum_core::{
    ATLAS_SIZE, CellSize, Config, Frame, GlyphSource, Instance, Layout, Rgb, Scene, Theme, View,
    overlay_columns, scale,
};
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

use crate::font::{Fonts, GlyphId, gamma_ratios};

const SHADER: &str = include_str!("shader.hlsl");

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
    scene: Scene<GlyphId>,
    fonts: Fonts,
    dpi: u32,
    padding: u32,
    font: optimum_core::FontConfig,
    text: optimum_core::TextConfig,
    width: u32,
    height: u32,
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
            scene: Scene::default(),
            fonts: Fonts::new(dpi, &config.font, &text_for(config))?,
            dpi,
            padding: config.window.padding,
            font: config.font.clone(),
            text: text_for(config),
            width: width.max(1),
            height: height.max(1),
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
        self.scene.reset();
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
            self.scene.reset();
        }
        self.scene.invalidate();
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub fn text_params(&self) -> crate::font::TextParams {
        self.fonts.params
    }

    #[cfg(debug_assertions)]
    pub fn set_text_params(&mut self, params: crate::font::TextParams) {
        if params.antialias != self.fonts.params.antialias {
            self.scene.invalidate();
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
        let layout = Layout {
            width: self.width,
            height: self.height,
            padding: scale(self.padding, self.dpi) as i32,
            border: scale(1, self.dpi),
            opacity: self.opacity,
        };
        let background = self
            .scene
            .compose(&mut self.fonts, layout, frame, theme, view);
        self.upload_glyphs();
        self.draw(background)
    }

    pub fn overlay_columns(&self) -> usize {
        overlay_columns(self.width, self.fonts.metrics())
    }

    fn upload_glyphs(&mut self) {
        for upload in self.scene.take_uploads() {
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
        let count = self.scene.instances().len();
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
                    self.scene.instances().as_ptr(),
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
}

//! The minimap's terrain on the GPU.
//!
//! The game thread composes a north-up image of the map around the player
//! (`meridian_core::minimap_image`: a texture that wraps around, updated a
//! cell at a time) and hands over what changed ([`send`]) and where and how
//! to draw it ([`MapFrame`]). The render thread (`RenderHooks::world_drawn`,
//! after the world and before the HUD) uploads the changed cells and draws
//! one quad into the minimap's rectangle: the shader turns the map with the
//! player, keeps the player's sub-texel position, cuts a round map to its
//! circle and fills unexplored texels with the backdrop. The HUD then draws
//! the frame, compass, marks and text over it.

use horizon_client::mod_api::{RenderHooks, WorldDrawnEvent, wgpu};
use meridian_core::minimap_image::CELL;
use parking_lot::Mutex;

/// Texels for the render thread to upload, oldest first.
#[derive(Debug, Default)]
pub struct Uploads {
    /// The texture's side, texels.
    pub size: u32,
    /// The whole image (ARGB, row-major): after a reset, or when asked for.
    pub full: Option<Vec<u32>>,
    /// Cells: their corner in the texture (texels) and `CELL` x `CELL` texels.
    pub cells: Vec<([u32; 2], Vec<u32>)>,
    /// The render thread has nothing to draw from (a new texture): the game
    /// thread sends the whole image.
    pub want_full: bool,
}

static UPLOADS: Mutex<Uploads> = Mutex::new(Uploads { size: 0, full: None, cells: Vec::new(), want_full: false });

/// Hands texels to the render thread.
pub fn send(f: impl FnOnce(&mut Uploads)) {
    f(&mut UPLOADS.lock());
}

/// What to draw this frame.
#[derive(Debug, Clone)]
pub struct MapFrame {
    /// The map's rectangle in GUI pixels, and the GUI's size.
    pub rect: [f32; 4],
    pub gui: [f32; 2],
    /// World directions (x, z) of the screen's right and up.
    pub right: [f32; 2],
    pub forward: [f32; 2],
    /// The player's position from the window's north-west texel, texels.
    pub center: [f32; 2],
    pub blocks_per_gui_px: f32,
    /// Blocks per texel.
    pub scale: f32,
    /// The texture's side, and where the window's north-west texel is in it.
    pub size: u32,
    pub wrap: [u32; 2],
    pub round: bool,
    pub opacity: f32,
    /// Unexplored texels, premultiplied-free RGBA 0..1.
    pub backdrop: [f32; 4],
    /// When the HUD asked for it: a frame the HUD stopped refreshing (F1, a
    /// screen) stops drawing.
    pub at: std::time::Instant,
    /// The chunk grid, with its line width in blocks.
    pub grid: Option<f32>,
    /// The block at the window's north-west texel, for the grid's phase.
    pub origin: [i32; 2],
}

static FRAME: Mutex<Option<MapFrame>> = Mutex::new(None);

/// Sets (or clears) what the next frames draw.
pub fn set_frame(frame: Option<MapFrame>) {
    *FRAME.lock() = frame;
}

const SHADER: &str = r#"
struct Params {
    right: vec2<f32>,
    forward: vec2<f32>,
    center: vec2<f32>,
    // Texels per physical pixel, the rectangle's half size in physical pixels.
    texels_per_px: f32,
    half: f32,
    backdrop: vec4<f32>,
    // x: texture size, y: round, z: opacity, w: blocks per texel
    misc: vec4<f32>,
    // x: grid on, y: line width in blocks, zw: origin modulo 16
    grid: vec4<f32>,
    // xy: the window's north-west texel in the texture (it wraps around)
    wrap: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: Params;
@group(0) @binding(1) var map: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
}

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    var corners = array<vec2<f32>, 6>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(1.0, 1.0),
        vec2(-1.0, -1.0), vec2(1.0, 1.0), vec2(-1.0, 1.0),
    );
    let c = corners[i];
    var out: Out;
    out.pos = vec4(c, 0.0, 1.0);
    // Screen offset from the centre in physical pixels, y down.
    out.local = vec2(c.x, -c.y) * p.half;
    return out;
}

// A texel of the window (transparent outside it).
fn texel_at(t: vec2<i32>) -> vec4<f32> {
    let size = i32(p.misc.x);
    if (t.x < 0 || t.y < 0 || t.x >= size || t.y >= size) {
        return vec4(0.0);
    }
    return textureLoad(map, (t + vec2<i32>(p.wrap.xy)) % size, 0);
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    if (p.misc.y > 0.5 && length(in.local) > p.half) {
        discard;
    }
    let world = (p.right * in.local.x - p.forward * in.local.y) * p.texels_per_px + p.center;
    // Bilinear between the four nearest texels, as the reference's minimap
    // looks (soft, not blocky); unexplored texels blend in the backdrop.
    let w = world - vec2(0.5);
    let base = vec2<i32>(floor(w));
    let f = fract(w);
    let t00 = texel_at(base);
    let t10 = texel_at(base + vec2(1, 0));
    let t01 = texel_at(base + vec2(0, 1));
    let t11 = texel_at(base + vec2(1, 1));
    let w00 = (1.0 - f.x) * (1.0 - f.y) * step(0.001, t00.a);
    let w10 = f.x * (1.0 - f.y) * step(0.001, t10.a);
    let w01 = (1.0 - f.x) * f.y * step(0.001, t01.a);
    let w11 = f.x * f.y * step(0.001, t11.a);
    let cover = w00 + w10 + w01 + w11;
    var color = p.backdrop;
    if (cover > 0.0) {
        let rgb = (t00.rgb * w00 + t10.rgb * w10 + t01.rgb * w01 + t11.rgb * w11) / cover;
        color = mix(p.backdrop, vec4(rgb, 1.0), cover);
    }
    if (p.grid.x > 0.5) {
        let f = fract((world * p.misc.w + p.grid.zw) / 16.0) * 16.0;
        let d = min(f, vec2(16.0) - f);
        if (min(d.x, d.y) < p.grid.y) {
            color = vec4(mix(color.rgb, vec3(0.1, 0.1, 0.1), 0.45), max(color.a, 0.6));
        }
    }
    return vec4(color.rgb, color.a * p.misc.z);
}
"#;

struct Gpu {
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniforms: wgpu::Buffer,
    texture: Option<(wgpu::Texture, wgpu::BindGroup, u32)>,
    /// The texture holds a whole image.
    ready: bool,
}

/// The render-thread half; `Hooks::render` hands it to Horizon.
#[derive(Default)]
pub struct MapRenderer {
    gpu: Mutex<Option<Gpu>>,
    world_map: crate::gpu_map::WorldMapRenderer,
}

/// Texel format so the map's sRGB colours come out as the HUD's do: an
/// sRGB view decodes them for linear (sRGB or float) targets.
pub(crate) fn texel_format(target: wgpu::TextureFormat) -> wgpu::TextureFormat {
    let gamma_target = matches!(
        target,
        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgb10a2Unorm
    );
    if gamma_target { wgpu::TextureFormat::Rgba8Unorm } else { wgpu::TextureFormat::Rgba8UnormSrgb }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("meridian minimap"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meridian minimap"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("meridian minimap"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("meridian minimap"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("meridian minimap"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self { format, pipeline, layout, uniforms, texture: None, ready: false }
    }

    /// Uploads what the game thread sent; whether the texture holds a
    /// whole image of `size` to draw from.
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
        let (size, full, cells) = {
            let mut u = UPLOADS.lock();
            if u.size == 0 {
                return false;
            }
            if self.texture.as_ref().is_some_and(|(_, _, size)| *size != u.size) {
                self.ready = false;
            }
            let full = u.full.take();
            if full.is_none() && !self.ready {
                // Cells alone can't fill a new texture: ask for all of it.
                u.want_full = true;
                u.cells.clear();
                return false;
            }
            (u.size, full, std::mem::take(&mut u.cells))
        };
        if self.texture.as_ref().is_none_or(|(_, _, s)| *s != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("meridian minimap"),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: texel_format(self.format),
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("meridian minimap"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                ],
            });
            self.texture = Some((texture, bind, size));
        }
        let (texture, _, _) = self.texture.as_ref().expect("made above");
        let write = |at: [u32; 2], side: u32, pixels: &[u32]| {
            // ARGB -> RGBA bytes.
            let mut bytes = Vec::with_capacity(pixels.len() * 4);
            for p in pixels {
                bytes.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: at[0], y: at[1], z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &bytes,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(side) },
                wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
            );
        };
        if let Some(full) = full {
            write([0, 0], size, &full);
            self.ready = true;
        }
        for (at, pixels) in cells {
            if at[0] + CELL <= size && at[1] + CELL <= size {
                write(at, CELL, &pixels);
            }
        }
        true
    }
}

impl RenderHooks for MapRenderer {
    fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>) {
        if self.world_map.draw(event) {
            return;
        }
        let Some(frame) = FRAME.lock().clone() else { return };
        if frame.at.elapsed() > std::time::Duration::from_millis(200) {
            return;
        }
        let world = &mut *event.world;
        // Uploads first: what the game thread sent must not pile up while
        // the map can't be drawn.
        let mut guard = self.gpu.lock();
        if guard.as_ref().is_none_or(|g| g.format != world.color_format) {
            *guard = Some(Gpu::new(world.device, world.color_format));
        }
        let gpu = guard.as_mut().expect("made above");
        if !gpu.upload(world.device, world.queue) || gpu.texture.as_ref().is_none_or(|(_, _, s)| *s != frame.size) {
            return;
        }
        let [w, h] = world.size;
        if w == 0 || h == 0 || frame.gui[0] <= 0.0 {
            return;
        }
        // Physical pixels per GUI pixel on this target.
        let scale = h as f32 / frame.gui[1];
        let [x, y, size, _] = frame.rect.map(|v| v * scale);
        let (x0, y0) = (x.round().max(0.0), y.round().max(0.0));
        let side = size.round();
        if side < 2.0 || x0 + side > w as f32 || y0 + side > h as f32 {
            return;
        }
        let params: [f32; 24] = [
            frame.right[0],
            frame.right[1],
            frame.forward[0],
            frame.forward[1],
            frame.center[0],
            frame.center[1],
            frame.blocks_per_gui_px / frame.scale / scale,
            side / 2.0,
            frame.backdrop[0],
            frame.backdrop[1],
            frame.backdrop[2],
            frame.backdrop[3],
            frame.size as f32,
            if frame.round { 1.0 } else { 0.0 },
            frame.opacity,
            frame.scale,
            if frame.grid.is_some() { 1.0 } else { 0.0 },
            frame.grid.unwrap_or(0.0),
            frame.origin[0].rem_euclid(16) as f32,
            frame.origin[1].rem_euclid(16) as f32,
            frame.wrap[0] as f32,
            frame.wrap[1] as f32,
            0.0,
            0.0,
        ];
        let bytes: Vec<u8> = params.iter().flat_map(|v| v.to_le_bytes()).collect();
        world.queue.write_buffer(&gpu.uniforms, 0, &bytes);
        let Some((_, bind, _)) = &gpu.texture else { return };
        let mut pass = world.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("meridian minimap"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: world.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_viewport(x0, y0, side, side, 0.0, 1.0);
        pass.set_scissor_rect(x0 as u32, y0 as u32, side as u32, side as u32);
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..6, 0..1);
    }
}

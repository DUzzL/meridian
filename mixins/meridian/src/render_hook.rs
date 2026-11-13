// Pinned native renderer callback. The GPU types stay in the function-replacement
// mirror; the mod crate shares only its own images and layout data with it.
fn draw(world: &mut WorldDrawn<'_>) {
    mod renderer {
        use super::{WorldDrawnEvent, wgpu};
        pub mod gpu {
            use super::{WorldDrawnEvent, wgpu};
            use meridian_horizon::core::minimap_image::CELL;
            use meridian_horizon::gpu::*;
            use parking_lot::Mutex;
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
                world_map: super::gpu_map::WorldMapRenderer,
            }

            /// Texel format so the map's sRGB colours come out as the HUD's do: an
            /// sRGB view decodes them for linear (sRGB or float) targets.
            pub(crate) fn texel_format(target: wgpu::TextureFormat) -> wgpu::TextureFormat {
                let gamma_target = matches!(
                    target,
                    wgpu::TextureFormat::Rgba8Unorm
                        | wgpu::TextureFormat::Bgra8Unorm
                        | wgpu::TextureFormat::Rgb10a2Unorm
                );
                if gamma_target {
                    wgpu::TextureFormat::Rgba8Unorm
                } else {
                    wgpu::TextureFormat::Rgba8UnormSrgb
                }
            }

            impl Gpu {
                fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("meridian minimap"),
                        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
                    });
                    let layout =
                        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: Some("meridian minimap"),
                            entries: &[
                                wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT
                                        | wgpu::ShaderStages::VERTEX,
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
                                        sample_type: wgpu::TextureSampleType::Float {
                                            filterable: false,
                                        },
                                        view_dimension: wgpu::TextureViewDimension::D2,
                                        multisampled: false,
                                    },
                                    count: None,
                                },
                            ],
                        });
                    let pipeline_layout =
                        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
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
                    Self {
                        format,
                        pipeline,
                        layout,
                        uniforms,
                        texture: None,
                        ready: false,
                    }
                }

                /// Uploads what the game thread sent; whether the texture holds a
                /// whole image of `size` to draw from.
                fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> bool {
                    let (size, full, cells) = {
                        let mut u = UPLOADS.lock();
                        if u.size == 0 {
                            return false;
                        }
                        if self
                            .texture
                            .as_ref()
                            .is_some_and(|(_, _, size)| *size != u.size)
                        {
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
                            size: wgpu::Extent3d {
                                width: size,
                                height: size,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: texel_format(self.format),
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        let view = texture.create_view(&Default::default());
                        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("meridian minimap"),
                            layout: &self.layout,
                            entries: &[
                                wgpu::BindGroupEntry {
                                    binding: 0,
                                    resource: self.uniforms.as_entire_binding(),
                                },
                                wgpu::BindGroupEntry {
                                    binding: 1,
                                    resource: wgpu::BindingResource::TextureView(&view),
                                },
                            ],
                        });
                        self.texture = Some((texture, bind, size));
                    }
                    let (texture, _, _) = self.texture.as_ref().expect("made above");
                    let write = |at: [u32; 2], side: u32, pixels: &[u32]| {
                        // ARGB -> RGBA bytes.
                        let mut bytes = Vec::with_capacity(pixels.len() * 4);
                        for p in pixels {
                            bytes.extend_from_slice(&[
                                (p >> 16) as u8,
                                (p >> 8) as u8,
                                *p as u8,
                                (p >> 24) as u8,
                            ]);
                        }
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d {
                                    x: at[0],
                                    y: at[1],
                                    z: 0,
                                },
                                aspect: wgpu::TextureAspect::All,
                            },
                            &bytes,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(side * 4),
                                rows_per_image: Some(side),
                            },
                            wgpu::Extent3d {
                                width: side,
                                height: side,
                                depth_or_array_layers: 1,
                            },
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

            impl MapRenderer {
                pub fn world_drawn(&self, event: &mut WorldDrawnEvent<'_, '_>) {
                    if self.world_map.draw(event) {
                        return;
                    }
                    let Some(frame) = FRAME.lock().clone() else {
                        return;
                    };
                    if frame.at.elapsed() > std::time::Duration::from_millis(200) {
                        return;
                    }
                    let world = &mut *event.world;
                    // Uploads first: what the game thread sent must not pile up while
                    // the map can't be drawn.
                    let mut guard = self.gpu.lock();
                    if guard
                        .as_ref()
                        .is_none_or(|g| g.format != world.color_format)
                    {
                        *guard = Some(Gpu::new(world.device, world.color_format));
                    }
                    let gpu = guard.as_mut().expect("made above");
                    if !gpu.upload(world.device, world.queue)
                        || gpu
                            .texture
                            .as_ref()
                            .is_none_or(|(_, _, s)| *s != frame.size)
                    {
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
                    let Some((_, bind, _)) = &gpu.texture else {
                        return;
                    };
                    let mut pass = world
                        .encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("meridian minimap"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: world.color_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
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
        }
        pub mod gpu_map {
            use super::{WorldDrawnEvent, wgpu};
            use meridian_horizon::gpu_map::*;
            use parking_lot::Mutex;
            use std::collections::HashMap;
            use std::sync::Arc;
            const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
}

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) alpha: f32) -> Out {
    var out: Out;
    out.pos = vec4(pos, 0.0, 1.0);
    out.uv = uv;
    out.alpha = alpha;
    return out;
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(tex));
    let t = vec2<i32>(clamp(floor(in.uv * size), vec2(0.0), size - vec2(1.0)));
    let c = textureLoad(tex, t, 0);
    if (c.a <= 0.0) {
        discard;
    }
    return vec4(c.rgb, c.a * in.alpha);
}
"#;

            /// How long after the map's last frame its Seed Atlas textures are dropped
            /// (as long as `meridian_horizon::core::seedatlas::RELEASE_AFTER` keeps the tiles).
            const RELEASE_AFTER: std::time::Duration =
                meridian_horizon::core::seedatlas::RELEASE_AFTER;
            /// Texture bytes uploaded per frame at most: six full regions, or many
            /// more small overview and seed textures.
            const UPLOAD_BYTES_PER_FRAME: usize = 6 * 512 * 512 * 4;
            /// Texture memory kept before the least recently drawn are dropped.
            const CACHE_BYTES: usize = 256 << 20;
            const LODS: [u32; 3] = [512, 128, 32];

            type Key = (u64, i32, i32, u32);

            struct Entry {
                bind: wgpu::BindGroup,
                _texture: wgpu::Texture,
                revision: u64,
                last_frame: u64,
                bytes: usize,
                /// A Seed Atlas tile (dropped when the map closes).
                seed: bool,
            }

            struct Gpu {
                format: wgpu::TextureFormat,
                pipeline: wgpu::RenderPipeline,
                layout: wgpu::BindGroupLayout,
                vertices: Option<(wgpu::Buffer, u64)>,
                background: wgpu::BindGroup,
                _background_texture: wgpu::Texture,
                entries: HashMap<Key, Entry>,
                bytes: usize,
                frame: u64,
                icons: Vec<(wgpu::BindGroup, wgpu::Texture)>,
                /// Whether any Seed Atlas tile is held.
                has_seed: bool,
            }

            #[derive(Default)]
            pub struct WorldMapRenderer {
                gpu: Mutex<Option<Gpu>>,
            }

            fn level_for(side_px: f32) -> u32 {
                if side_px >= 300.0 {
                    512
                } else if side_px >= 75.0 {
                    128
                } else {
                    32
                }
            }

            /// RGBA bytes of a square region image (512 texels a side, or an overview
            /// of 128) at `lod` texels a side: each texel the mean of the explored
            /// pixels it covers, transparent if none are.
            fn downsample(pixels: &[u32], lod: u32) -> Vec<u8> {
                let side = (pixels.len() as f64).sqrt() as usize;
                let lod = (lod as usize).min(side.max(1));
                let mut out = vec![0u8; lod * lod * 4];
                if side == 0 || side * side != pixels.len() || side % lod != 0 {
                    return out;
                }
                if lod == side {
                    for (o, p) in out.chunks_exact_mut(4).zip(pixels) {
                        if p >> 24 != 0 {
                            o.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
                        }
                    }
                    return out;
                }
                let f = side / lod;
                for ty in 0..lod {
                    for tx in 0..lod {
                        let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
                        for y in ty * f..(ty + 1) * f {
                            for p in &pixels[y * side + tx * f..y * side + (tx + 1) * f] {
                                if p >> 24 != 0 {
                                    r += (p >> 16) & 0xFF;
                                    g += (p >> 8) & 0xFF;
                                    b += p & 0xFF;
                                    n += 1;
                                }
                            }
                        }
                        if let (Some(r), Some(g), Some(b)) =
                            (r.checked_div(n), g.checked_div(n), b.checked_div(n))
                        {
                            let o = (ty * lod + tx) * 4;
                            out[o..o + 4].copy_from_slice(&[r as u8, g as u8, b as u8, 255]);
                        }
                    }
                }
                out
            }

            impl Gpu {
                fn new(
                    device: &wgpu::Device,
                    queue: &wgpu::Queue,
                    format: wgpu::TextureFormat,
                ) -> Self {
                    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("meridian world map"),
                        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
                    });
                    let layout =
                        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            label: Some("meridian world map"),
                            entries: &[wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: false,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                count: None,
                            }],
                        });
                    let pipeline_layout =
                        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            label: Some("meridian world map"),
                            bind_group_layouts: &[Some(&layout)],
                            ..Default::default()
                        });
                    let attributes =
                        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32];
                    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("meridian world map"),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &module,
                            entry_point: Some("vs"),
                            compilation_options: Default::default(),
                            buffers: &[Some(wgpu::VertexBufferLayout {
                                array_stride: 20,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &attributes,
                            })],
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
                    let (background_texture, background) =
                        Self::texture(device, &layout, format, 1);
                    Self::write(queue, &background_texture, 1, &[8, 8, 8, 255]);
                    Self {
                        format,
                        pipeline,
                        layout,
                        vertices: None,
                        background,
                        _background_texture: background_texture,
                        entries: HashMap::new(),
                        has_seed: false,
                        bytes: 0,
                        frame: 0,
                        icons: Vec::new(),
                    }
                }

                fn texture(
                    device: &wgpu::Device,
                    layout: &wgpu::BindGroupLayout,
                    target: wgpu::TextureFormat,
                    side: u32,
                ) -> (wgpu::Texture, wgpu::BindGroup) {
                    Self::texture_wh(device, layout, target, side, side)
                }

                fn texture_wh(
                    device: &wgpu::Device,
                    layout: &wgpu::BindGroupLayout,
                    target: wgpu::TextureFormat,
                    width: u32,
                    height: u32,
                ) -> (wgpu::Texture, wgpu::BindGroup) {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("meridian world map region"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: super::gpu::texel_format(target),
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&Default::default());
                    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("meridian world map region"),
                        layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        }],
                    });
                    (texture, bind)
                }

                fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, side: u32, bytes: &[u8]) {
                    Self::write_wh(queue, texture, side, side, bytes);
                }

                fn write_wh(
                    queue: &wgpu::Queue,
                    texture: &wgpu::Texture,
                    width: u32,
                    height: u32,
                    bytes: &[u8],
                ) {
                    let side = width;
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        bytes,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(side * 4),
                            rows_per_image: Some(height),
                        },
                        wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                    );
                }

                /// Makes sure `key` holds `pixels` at `revision`; false when the frame's
                /// upload budget (bytes) is spent.
                fn upload(
                    &mut self,
                    device: &wgpu::Device,
                    queue: &wgpu::Queue,
                    key: Key,
                    pixels: &[u32],
                    revision: u64,
                    fixed: Option<u8>,
                    budget: &mut usize,
                ) -> bool {
                    if self
                        .entries
                        .get(&key)
                        .is_some_and(|e| e.revision == revision)
                    {
                        return true;
                    }
                    let side = key.3;
                    let cost = (side * side * 4) as usize;
                    if *budget < cost {
                        return false;
                    }
                    *budget -= cost;
                    let bytes = match fixed {
                        None => downsample(pixels, side),
                        Some(alpha) => pixels
                            .iter()
                            .flat_map(|p| {
                                [
                                    (p >> 16) as u8,
                                    (p >> 8) as u8,
                                    *p as u8,
                                    if p >> 24 == 0 { 0 } else { alpha },
                                ]
                            })
                            .collect(),
                    };
                    if let Some(e) = self.entries.get_mut(&key) {
                        Self::write(queue, &e._texture, side, &bytes);
                        e.revision = revision;
                        return true;
                    }
                    let (texture, bind) = Self::texture(device, &self.layout, self.format, side);
                    Self::write(queue, &texture, side, &bytes);
                    let size = (side * side * 4) as usize;
                    self.bytes += size;
                    self.entries.insert(
                        key,
                        Entry {
                            bind,
                            _texture: texture,
                            revision,
                            last_frame: self.frame,
                            bytes: size,
                            seed: fixed.is_some(),
                        },
                    );
                    self.has_seed |= fixed.is_some();
                    true
                }

                /// Drops the Seed Atlas tiles: they are recomputed (or made from the
                /// tile cache) when the map opens again, and take the most memory.
                fn release_seed(&mut self) {
                    if !self.has_seed {
                        return;
                    }
                    let bytes = &mut self.bytes;
                    self.entries.retain(|_, e| {
                        if e.seed {
                            *bytes -= e.bytes;
                        }
                        !e.seed
                    });
                    self.has_seed = false;
                }

                /// Drops the terrain textures (not the Seed Atlas tiles).
                fn release_regions(&mut self) {
                    let bytes = &mut self.bytes;
                    self.entries.retain(|_, e| {
                        if !e.seed {
                            *bytes -= e.bytes;
                        }
                        e.seed
                    });
                }

                fn evict(&mut self) {
                    if self.bytes <= CACHE_BYTES {
                        return;
                    }
                    let mut by_age: Vec<(u64, Key)> = self
                        .entries
                        .iter()
                        .map(|(k, e)| (e.last_frame, *k))
                        .collect();
                    by_age.sort_unstable();
                    for (last, key) in by_age {
                        if self.bytes <= CACHE_BYTES * 3 / 4 || last == self.frame {
                            break;
                        }
                        if let Some(e) = self.entries.remove(&key) {
                            self.bytes -= e.bytes;
                        }
                    }
                }
            }

            impl WorldMapRenderer {
                /// Draws the map when its screen is open; true if it drew.
                pub fn draw(&self, event: &mut WorldDrawnEvent<'_, '_>) -> bool {
                    if FORGET_REGIONS.swap(false, std::sync::atomic::Ordering::Relaxed)
                        && let Some(gpu) = self.gpu.lock().as_mut()
                    {
                        gpu.release_regions();
                    }
                    let frame = FRAME.lock().clone();
                    let age = frame.as_ref().map(|f| f.at.elapsed());
                    let Some(frame) = frame.filter(|_| {
                        age.is_some_and(|a| a <= std::time::Duration::from_millis(200))
                    }) else {
                        // The map is closed (not just a slow frame): its Seed Atlas
                        // textures go.
                        if age.is_none_or(|a| a >= RELEASE_AFTER)
                            && let Some(gpu) = self.gpu.lock().as_mut()
                        {
                            gpu.release_seed();
                        }
                        return false;
                    };
                    let world = &mut *event.world;
                    let [w, h] = world.size;
                    if w == 0 || h == 0 || frame.gui[0] <= 0.0 || frame.gui[1] <= 0.0 {
                        return false;
                    }
                    let mut guard = self.gpu.lock();
                    if guard
                        .as_ref()
                        .is_none_or(|g| g.format != world.color_format)
                    {
                        *guard = Some(Gpu::new(world.device, world.queue, world.color_format));
                    }
                    let gpu = guard.as_mut().expect("made above");
                    gpu.frame += 1;
                    let scale = [w as f32 / frame.gui[0], h as f32 / frame.gui[1]];
                    let ndc = |x: f32, y: f32| {
                        let (px, py) = ((x * scale[0]).round(), (y * scale[1]).round());
                        [px / w as f32 * 2.0 - 1.0, 1.0 - py / h as f32 * 2.0]
                    };
                    let mut vertices: Vec<f32> = Vec::with_capacity(
                        (frame.regions.len() + frame.background.len() + frame.icons.len() + 1) * 30,
                    );
                    let mut quad = |l: [f32; 2], r: [f32; 2], alpha: f32| {
                        let (a, b, c, d) = (
                            [l[0], l[1], 0.0, 0.0, alpha],
                            [r[0], l[1], 1.0, 0.0, alpha],
                            [r[0], r[1], 1.0, 1.0, alpha],
                            [l[0], r[1], 0.0, 1.0, alpha],
                        );
                        for v in [a, b, c, a, c, d] {
                            vertices.extend_from_slice(&v);
                        }
                    };
                    quad([-1.0, 1.0], [1.0, -1.0], 1.0);
                    enum Draw {
                        Entry(Key),
                        Icon(usize),
                    }
                    let mut draws: Vec<Draw> = Vec::new();
                    let mut budget = UPLOAD_BYTES_PER_FRAME;
                    let mid = [frame.gui[0] / 2.0, frame.gui[1] / 2.0];
                    let near = |a: &RegionQuad, b: &RegionQuad| {
                        let d = |r: &RegionQuad| {
                            ((r.rect[0] + r.rect[2]) / 2.0 - mid[0]).abs()
                                + ((r.rect[1] + r.rect[3]) / 2.0 - mid[1]).abs()
                        };
                        d(a).total_cmp(&d(b))
                    };
                    // The Seed Atlas layer, at the size it comes in: under the terrain,
                    // or moved over it below.
                    let mut background = frame.background.clone();
                    background.sort_by(near);
                    let layer = frame.world ^ 0x5EED_A71A5;
                    for r in &background {
                        let key = (layer ^ ((r.layer as u64) << 40), r.pos[0], r.pos[1], r.size);
                        let ready = match &r.pixels {
                            Some(p) => gpu.upload(
                                world.device,
                                world.queue,
                                key,
                                p,
                                r.revision,
                                Some(r.alpha),
                                &mut budget,
                            ),
                            None => gpu.entries.contains_key(&key),
                        };
                        let chosen = if ready { Some(key) } else { None };
                        let Some(key) = chosen else { continue };
                        if let Some(e) = gpu.entries.get_mut(&key) {
                            e.last_frame = gpu.frame;
                        }
                        quad(ndc(r.rect[0], r.rect[1]), ndc(r.rect[2], r.rect[3]), 1.0);
                        draws.push(Draw::Entry(key));
                    }
                    let seed_quads = draws.len();
                    // Nearest regions first, so uploads fill in from the middle.
                    let mut regions = frame.regions.clone();
                    regions.sort_by(near);
                    for r in &regions {
                        let side = (r.rect[2] - r.rect[0]) * scale[0];
                        // An overview has no more detail than its own size.
                        let want = level_for(side).min(r.size);
                        let key = (frame.world, r.pos[0], r.pos[1], want);
                        let ready = match &r.pixels {
                            Some(p) => gpu.upload(
                                world.device,
                                world.queue,
                                key,
                                p,
                                r.revision,
                                None,
                                &mut budget,
                            ),
                            None => gpu.entries.contains_key(&key),
                        };
                        let chosen = if ready {
                            Some(key)
                        } else {
                            // Meanwhile any other level of detail already there.
                            LODS.iter()
                                .map(|&l| (frame.world, r.pos[0], r.pos[1], l))
                                .find(|k| gpu.entries.contains_key(k))
                        };
                        let Some(key) = chosen else { continue };
                        if let Some(e) = gpu.entries.get_mut(&key) {
                            e.last_frame = gpu.frame;
                        }
                        quad(ndc(r.rect[0], r.rect[1]), ndc(r.rect[2], r.rect[3]), 1.0);
                        draws.push(Draw::Entry(key));
                    }
                    if gpu.icons.is_empty()
                        && let Some(icons) = ICONS.get()
                    {
                        for (w, h, rgba) in icons {
                            let (texture, bind) =
                                Gpu::texture_wh(world.device, &gpu.layout, gpu.format, *w, *h);
                            Gpu::write_wh(world.queue, &texture, *w, *h, rgba);
                            gpu.icons.push((bind, texture));
                        }
                    }
                    for icon in &frame.icons {
                        if icon.icon < gpu.icons.len() {
                            quad(
                                ndc(icon.rect[0], icon.rect[1]),
                                ndc(icon.rect[0] + icon.rect[2], icon.rect[1] + icon.rect[3]),
                                icon.alpha,
                            );
                            draws.push(Draw::Icon(icon.icon));
                        }
                    }
                    if frame.seed_above {
                        // The seed layer's quads after the terrain's (quad 0 is the
                        // backdrop, icons stay last).
                        let terrain_quads =
                            draws.iter().filter(|d| matches!(d, Draw::Entry(_))).count()
                                - seed_quads;
                        let floats = 30;
                        vertices[floats..floats * (1 + seed_quads + terrain_quads)]
                            .rotate_left(floats * seed_quads);
                        draws[..seed_quads + terrain_quads].rotate_left(seed_quads);
                    }
                    gpu.evict();
                    let bytes: Vec<u8> = vertices.iter().flat_map(|v| v.to_le_bytes()).collect();
                    let needed = bytes.len() as u64;
                    if gpu.vertices.as_ref().is_none_or(|(_, cap)| *cap < needed) {
                        let cap = needed.next_power_of_two().max(4096);
                        let buffer = world.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("meridian world map"),
                            size: cap,
                            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        gpu.vertices = Some((buffer, cap));
                    }
                    let (buffer, _) = gpu.vertices.as_ref().expect("made above");
                    world.queue.write_buffer(buffer, 0, &bytes);
                    let mut pass = world
                        .encoder
                        .begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("meridian world map"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: world.color_view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            ..Default::default()
                        });
                    pass.set_pipeline(&gpu.pipeline);
                    pass.set_vertex_buffer(0, buffer.slice(..));
                    pass.set_bind_group(0, &gpu.background, &[]);
                    pass.draw(0..6, 0..1);
                    for (i, draw) in draws.iter().enumerate() {
                        let bind = match draw {
                            Draw::Entry(key) => match gpu.entries.get(key) {
                                Some(e) => &e.bind,
                                None => continue,
                            },
                            Draw::Icon(i) => &gpu.icons[*i].0,
                        };
                        let first = (i as u32 + 1) * 6;
                        pass.set_bind_group(0, bind, &[]);
                        pass.draw(first..first + 6, 0..1);
                    }
                    true
                }
            }

            #[cfg(test)]
            mod tests {
                use super::*;

                #[test]
                fn downsampling_averages_explored_pixels() {
                    let mut pixels = vec![0u32; 512 * 512];
                    // The first 16x16 block: half red, half unexplored.
                    for y in 0..16 {
                        for x in 0..8 {
                            pixels[y * 512 + x] = 0xFFFF_0000;
                        }
                    }
                    let out = downsample(&pixels, 32);
                    assert_eq!(&out[0..4], &[255, 0, 0, 255]);
                    assert_eq!(
                        &out[4..8],
                        &[0, 0, 0, 0],
                        "nothing explored stays transparent"
                    );
                    assert_eq!(downsample(&pixels, 512)[0..4], [255, 0, 0, 255]);
                    assert_eq!(level_for(400.0), 512);
                    assert_eq!(level_for(100.0), 128);
                    assert_eq!(level_for(10.0), 32);
                }
            }
        }
    }
    static RENDERER: std::sync::OnceLock<renderer::gpu::MapRenderer> = std::sync::OnceLock::new();
    RENDERER
        .get_or_init(renderer::gpu::MapRenderer::default)
        .world_drawn(&mut WorldDrawnEvent { world });
}

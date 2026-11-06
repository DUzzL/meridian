//! The world map's terrain on the GPU.
//!
//! The map screen hands over the regions it shows, each with its rectangle
//! on screen and (when the region is in memory) its pixels. The render
//! thread keeps one texture per region and level of detail (512, 128 or 32
//! texels a side, picked by how big the region is on screen), uploads a few
//! per frame when they are missing or redrawn, and draws them as quads over
//! a black background. Textures stay cached after their regions leave
//! memory, so zooming out shows what was seen without loading it again.

use std::collections::HashMap;
use std::sync::Arc;

use horizon_client::mod_api::{WorldDrawnEvent, wgpu};
use parking_lot::Mutex;

/// One region on screen.
#[derive(Debug, Clone)]
pub struct RegionQuad {
    pub pos: [i32; 2],
    /// Left, top, right, bottom in GUI pixels.
    pub rect: [f32; 4],
    /// 512 x 512 ARGB when the region is in memory and drawn.
    pub pixels: Option<Arc<Vec<u32>>>,
    pub revision: u64,
}

#[derive(Debug, Clone)]
pub struct WorldMapFrame {
    pub gui: [f32; 2],
    /// Which map the regions belong to (world, dimension, cave layer).
    pub world: u64,
    pub regions: Vec<RegionQuad>,
    pub at: std::time::Instant,
}

static FRAME: Mutex<Option<WorldMapFrame>> = Mutex::new(None);

pub fn set_frame(frame: Option<WorldMapFrame>) {
    *FRAME.lock() = frame;
}

const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> Out {
    var out: Out;
    out.pos = vec4(pos, 0.0, 1.0);
    out.uv = uv;
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
    return vec4(c.rgb, 1.0);
}
"#;

/// Region textures uploaded per frame at most.
const UPLOADS_PER_FRAME: usize = 6;
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

/// RGBA bytes of a 512-texel region at `lod` texels a side: each texel the
/// mean of the explored pixels it covers, transparent if none are.
fn downsample(pixels: &[u32], lod: u32) -> Vec<u8> {
    const SIDE: usize = 512;
    let lod = lod as usize;
    let mut out = vec![0u8; lod * lod * 4];
    if pixels.len() < SIDE * SIDE {
        return out;
    }
    if lod == SIDE {
        for (o, p) in out.chunks_exact_mut(4).zip(pixels) {
            if p >> 24 != 0 {
                o.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
            }
        }
        return out;
    }
    let f = SIDE / lod;
    for ty in 0..lod {
        for tx in 0..lod {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in ty * f..(ty + 1) * f {
                for p in &pixels[y * SIDE + tx * f..y * SIDE + (tx + 1) * f] {
                    if p >> 24 != 0 {
                        r += (p >> 16) & 0xFF;
                        g += (p >> 8) & 0xFF;
                        b += p & 0xFF;
                        n += 1;
                    }
                }
            }
            if n > 0 {
                let o = (ty * lod + tx) * 4;
                out[o..o + 4].copy_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8, 255]);
            }
        }
    }
    out
}

impl Gpu {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("meridian world map"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("meridian world map"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("meridian world map"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("meridian world map"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
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
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let (background_texture, background) = Self::texture(device, &layout, format, 1);
        Self::write(queue, &background_texture, 1, &[8, 8, 8, 255]);
        Self {
            format,
            pipeline,
            layout,
            vertices: None,
            background,
            _background_texture: background_texture,
            entries: HashMap::new(),
            bytes: 0,
            frame: 0,
        }
    }

    fn texture(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        target: wgpu::TextureFormat,
        side: u32,
    ) -> (wgpu::Texture, wgpu::BindGroup) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("meridian world map region"),
            size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::gpu::texel_format(target),
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("meridian world map region"),
            layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }],
        });
        (texture, bind)
    }

    fn write(queue: &wgpu::Queue, texture: &wgpu::Texture, side: u32, bytes: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(side) },
            wgpu::Extent3d { width: side, height: side, depth_or_array_layers: 1 },
        );
    }

    /// Makes sure `key` holds `pixels` at `revision`; false when the frame's
    /// upload budget is spent.
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: Key,
        pixels: &[u32],
        revision: u64,
        budget: &mut usize,
    ) -> bool {
        if self.entries.get(&key).is_some_and(|e| e.revision == revision) {
            return true;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        let side = key.3;
        let bytes = downsample(pixels, side);
        if let Some(e) = self.entries.get_mut(&key) {
            Self::write(queue, &e._texture, side, &bytes);
            e.revision = revision;
            return true;
        }
        let (texture, bind) = Self::texture(device, &self.layout, self.format, side);
        Self::write(queue, &texture, side, &bytes);
        let size = (side * side * 4) as usize;
        self.bytes += size;
        self.entries.insert(key, Entry { bind, _texture: texture, revision, last_frame: self.frame, bytes: size });
        true
    }

    fn evict(&mut self) {
        if self.bytes <= CACHE_BYTES {
            return;
        }
        let mut by_age: Vec<(u64, Key)> = self.entries.iter().map(|(k, e)| (e.last_frame, *k)).collect();
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
        let Some(frame) = FRAME.lock().clone() else { return false };
        if frame.at.elapsed() > std::time::Duration::from_millis(200) {
            return false;
        }
        let world = &mut *event.world;
        let [w, h] = world.size;
        if w == 0 || h == 0 || frame.gui[0] <= 0.0 || frame.gui[1] <= 0.0 {
            return false;
        }
        let mut guard = self.gpu.lock();
        if guard.as_ref().is_none_or(|g| g.format != world.color_format) {
            *guard = Some(Gpu::new(world.device, world.queue, world.color_format));
        }
        let gpu = guard.as_mut().expect("made above");
        gpu.frame += 1;
        let scale = [w as f32 / frame.gui[0], h as f32 / frame.gui[1]];
        let ndc = |x: f32, y: f32| {
            let (px, py) = ((x * scale[0]).round(), (y * scale[1]).round());
            [px / w as f32 * 2.0 - 1.0, 1.0 - py / h as f32 * 2.0]
        };
        let mut vertices: Vec<f32> = Vec::with_capacity((frame.regions.len() + 1) * 24);
        let mut quad = |l: [f32; 2], r: [f32; 2]| {
            let (a, b, c, d) = ([l[0], l[1], 0.0, 0.0], [r[0], l[1], 1.0, 0.0], [r[0], r[1], 1.0, 1.0], [l[0], r[1], 0.0, 1.0]);
            for v in [a, b, c, a, c, d] {
                vertices.extend_from_slice(&v);
            }
        };
        quad([-1.0, 1.0], [1.0, -1.0]);
        let mut draws: Vec<Key> = Vec::new();
        let mut budget = UPLOADS_PER_FRAME;
        // Nearest regions first, so uploads fill in from the middle.
        let mut regions = frame.regions.clone();
        let mid = [frame.gui[0] / 2.0, frame.gui[1] / 2.0];
        regions.sort_by(|a, b| {
            let d = |r: &RegionQuad| ((r.rect[0] + r.rect[2]) / 2.0 - mid[0]).abs() + ((r.rect[1] + r.rect[3]) / 2.0 - mid[1]).abs();
            d(a).total_cmp(&d(b))
        });
        for r in &regions {
            let side = (r.rect[2] - r.rect[0]) * scale[0];
            let want = level_for(side);
            let key = (frame.world, r.pos[0], r.pos[1], want);
            let ready = match &r.pixels {
                Some(p) => gpu.upload(world.device, world.queue, key, p, r.revision, &mut budget),
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
            quad(ndc(r.rect[0], r.rect[1]), ndc(r.rect[2], r.rect[3]));
            draws.push(key);
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
        let mut pass = world.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("meridian world map"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: world.color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.set_bind_group(0, &gpu.background, &[]);
        pass.draw(0..6, 0..1);
        for (i, key) in draws.iter().enumerate() {
            let Some(e) = gpu.entries.get(key) else { continue };
            let first = (i as u32 + 1) * 6;
            pass.set_bind_group(0, &e.bind, &[]);
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
        assert_eq!(&out[4..8], &[0, 0, 0, 0], "nothing explored stays transparent");
        assert_eq!(downsample(&pixels, 512)[0..4], [255, 0, 0, 255]);
        assert_eq!(level_for(400.0), 512);
        assert_eq!(level_for(100.0), 128);
        assert_eq!(level_for(10.0), 32);
    }
}

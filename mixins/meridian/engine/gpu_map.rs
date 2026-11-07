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
    /// `size` x `size` ARGB when the region is in memory and drawn.
    pub pixels: Option<Arc<Vec<u32>>>,
    pub revision: u64,
    /// Texels a side of `pixels`: 512 for terrain, which is drawn at a level
    /// of detail; other sizes (Seed Atlas tiles) are drawn as they are.
    pub size: u32,
    /// Opacity of the texels, 0..=255.
    pub alpha: u8,
    /// Which set the position belongs to (Seed Atlas tiles: their step), so
    /// equal positions of different sets keep their own textures.
    pub layer: u32,
}

/// A marker icon on screen.
#[derive(Debug, Clone, Copy)]
pub struct IconQuad {
    pub icon: usize,
    pub rect: [f32; 4],
    /// Opacity 0..=1 (completed structures fade).
    pub alpha: f32,
}

/// Icons the map can draw, RGBA, uploaded once.
pub static ICONS: std::sync::OnceLock<Vec<(u32, u32, Vec<u8>)>> = std::sync::OnceLock::new();

#[derive(Debug, Clone)]
pub struct WorldMapFrame {
    pub gui: [f32; 2],
    /// Which map the regions belong to (world, dimension, cave layer).
    pub world: u64,
    pub regions: Vec<RegionQuad>,
    /// Drawn first, under the terrain (the Seed Atlas biome layer).
    pub background: Vec<RegionQuad>,
    /// Drawn last, over the terrain.
    pub icons: Vec<IconQuad>,
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

/// How long after the map's last frame its Seed Atlas textures are dropped.
const RELEASE_AFTER: std::time::Duration = std::time::Duration::from_secs(2);
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
            if let (Some(r), Some(g), Some(b)) = (r.checked_div(n), g.checked_div(n), b.checked_div(n)) {
                let o = (ty * lod + tx) * 4;
                out[o..o + 4].copy_from_slice(&[r as u8, g as u8, b as u8, 255]);
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
        let attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32];
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
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
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
        Self::write_wh(queue, texture, side, side, bytes);
    }

    fn write_wh(queue: &wgpu::Queue, texture: &wgpu::Texture, width: u32, height: u32, bytes: &[u8]) {
        let side = width;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * 4), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
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
        if self.entries.get(&key).is_some_and(|e| e.revision == revision) {
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
                .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, if p >> 24 == 0 { 0 } else { alpha }])
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
        self.entries.insert(key, Entry { bind, _texture: texture, revision, last_frame: self.frame, bytes: size, seed: fixed.is_some() });
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
        let frame = FRAME.lock().clone();
        let age = frame.as_ref().map(|f| f.at.elapsed());
        let Some(frame) = frame.filter(|_| age.is_some_and(|a| a <= std::time::Duration::from_millis(200))) else {
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
        let mut vertices: Vec<f32> = Vec::with_capacity((frame.regions.len() + frame.background.len() + frame.icons.len() + 1) * 30);
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
            let d = |r: &RegionQuad| ((r.rect[0] + r.rect[2]) / 2.0 - mid[0]).abs() + ((r.rect[1] + r.rect[3]) / 2.0 - mid[1]).abs();
            d(a).total_cmp(&d(b))
        };
        // The Seed Atlas layer under the terrain, at the size it comes in.
        let mut background = frame.background.clone();
        background.sort_by(near);
        let layer = frame.world ^ 0x5EED_A71A5;
        for r in &background {
            let key = (layer ^ ((r.layer as u64) << 40), r.pos[0], r.pos[1], r.size);
            let ready = match &r.pixels {
                Some(p) => gpu.upload(world.device, world.queue, key, p, r.revision, Some(r.alpha), &mut budget),
                None => gpu.entries.contains_key(&key),
            };
            let chosen = if ready {
                Some(key)
            } else {
                None
            };
            let Some(key) = chosen else { continue };
            if let Some(e) = gpu.entries.get_mut(&key) {
                e.last_frame = gpu.frame;
            }
            quad(ndc(r.rect[0], r.rect[1]), ndc(r.rect[2], r.rect[3]), 1.0);
            draws.push(Draw::Entry(key));
        }
        // Nearest regions first, so uploads fill in from the middle.
        let mut regions = frame.regions.clone();
        regions.sort_by(near);
        for r in &regions {
            let side = (r.rect[2] - r.rect[0]) * scale[0];
            // An overview has no more detail than its own size.
            let want = level_for(side).min(r.size);
            let key = (frame.world, r.pos[0], r.pos[1], want);
            let ready = match &r.pixels {
                Some(p) => gpu.upload(world.device, world.queue, key, p, r.revision, None, &mut budget),
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
                let (texture, bind) = Gpu::texture_wh(world.device, &gpu.layout, gpu.format, *w, *h);
                Gpu::write_wh(world.queue, &texture, *w, *h, rgba);
                gpu.icons.push((bind, texture));
            }
        }
        for icon in &frame.icons {
            if icon.icon < gpu.icons.len() {
                quad(ndc(icon.rect[0], icon.rect[1]), ndc(icon.rect[0] + icon.rect[2], icon.rect[1] + icon.rect[3]), icon.alpha);
                draws.push(Draw::Icon(icon.icon));
            }
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
        assert_eq!(&out[4..8], &[0, 0, 0, 0], "nothing explored stays transparent");
        assert_eq!(downsample(&pixels, 512)[0..4], [255, 0, 0, 255]);
        assert_eq!(level_for(400.0), 512);
        assert_eq!(level_for(100.0), 128);
        assert_eq!(level_for(10.0), 32);
    }
}

//! The maps' pictures, composed on the CPU and shown through the mod API's
//! own image (`ClientHost::mod_image`, the HUD atlas slot `horizon/modimage`).
//!
//! The draw hooks lay out what to show ([`gpu::MapFrame`] for the minimap,
//! [`gpu_map::WorldMapFrame`] for the world map) and draw the image uploaded
//! for it; [`compose_and_upload`], called once a frame from
//! `render_frame_pre` (the hook with a mutable host), turns the latest layout
//! into pixels and uploads them when they changed. The composition follows
//! the former GPU passes: the minimap turns with the player, keeps the
//! player's sub-texel position, blends neighbouring texels (soft, as the
//! reference looks), cuts a round map to its circle and draws the chunk
//! grid; the world map draws each region at a level of detail picked by its
//! size on screen, the Seed Atlas layer under or over it, and the marker
//! icons last.
//!
//! The slot is one image of at most `SLOT` pixels a side, shared by the
//! minimap and the world map: the world map wins while it is shown (it
//! covers the screen). An image larger than the slot is composed smaller and
//! stretched on screen.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mod_api::{ClientHost, DrawContext};
use parking_lot::Mutex;

use crate::core::minimap_image::CELL;
use crate::{gpu, gpu_map};

/// The mod image's side (`MOD_IMAGE_SIZE`).
pub const SLOT: u32 = 1024;
/// The HUD atlas sprite holding the mod image.
const SPRITE: &str = "horizon/modimage";
/// A layout older than this is not shown (its screen stopped drawing).
const STALE: Duration = Duration::from_millis(200);
/// Region pictures kept for the world map before the least recently drawn go.
const CACHE_BYTES: usize = 192 << 20;
const LODS: [u32; 3] = [512, 128, 32];

/// What the uploaded image shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shown {
    Minimap { side: u32 },
    WorldMap { width: u32, height: u32 },
}

type Key = (u64, i32, i32, u32);

struct Entry {
    /// ARGB, `side` x `side`, alpha as drawn.
    pixels: Arc<Vec<u32>>,
    side: u32,
    revision: u64,
    last_frame: u64,
    seed: bool,
}

#[derive(Default)]
struct Canvas {
    /// The minimap's texture, as the game thread sends it (it wraps around).
    mini: Vec<u32>,
    mini_size: u32,
    mini_ready: bool,
    /// Region and Seed Atlas pictures at the levels of detail drawn.
    entries: HashMap<Key, Entry>,
    bytes: usize,
    frame: u64,
    has_seed: bool,
    /// When the world map was last shown, to let its Seed Atlas pictures go.
    map_seen: Option<Instant>,
    /// The image being composed (ARGB), and the RGBA bytes uploaded.
    image: Vec<u32>,
    bytes_out: Vec<u8>,
    /// What the last upload shows, and the fingerprint of its inputs.
    shown: Option<Shown>,
    signature: u64,
}

static CANVAS: Mutex<Option<Canvas>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Canvas) -> R) -> R {
    f(CANVAS.lock().get_or_insert_with(Canvas::default))
}

/// Composes the picture for the latest layout and uploads it when it
/// changed. `gui_scale` is physical pixels per GUI pixel.
pub fn compose_and_upload(host: &mut dyn ClientHost, gui_scale: f32) {
    let gui_scale = if gui_scale.is_finite() && gui_scale > 0.0 { gui_scale } else { 1.0 };
    let upload = with(|c| {
        c.frame += 1;
        c.take_minimap_uploads();
        if gpu_map::FORGET_REGIONS.swap(false, std::sync::atomic::Ordering::Relaxed) {
            c.release(|e| !e.seed);
        }
        let map = gpu_map::FRAME.lock().clone().filter(|f| f.at.elapsed() <= STALE);
        if let Some(map) = map {
            c.map_seen = Some(Instant::now());
            return c.world_map(&map, gui_scale);
        }
        if c.map_seen.is_some_and(|t| t.elapsed() >= crate::core::seedatlas::RELEASE_AFTER) {
            c.release(|e| e.seed);
            c.map_seen = None;
        }
        let mini = gpu::FRAME.lock().clone().filter(|f| f.at.elapsed() <= STALE);
        match mini {
            Some(frame) => c.minimap(&frame, gui_scale),
            None => None,
        }
    });
    if let Some((width, height)) = upload {
        let ok = with(|c| {
            let ok = host.mod_image(width, height, &c.bytes_out);
            if !ok {
                c.shown = None;
            }
            ok
        });
        if !ok {
            log::warn!("Meridian: the game refused the map image ({width} x {height})");
        }
    }
}

/// Draws the minimap's picture into `rect` (GUI pixels), if it is the one
/// uploaded.
pub fn draw_minimap(draw: &mut dyn DrawContext, rect: [f32; 4]) {
    if let Some(Shown::Minimap { side }) = with(|c| c.shown) {
        let s = side as f32;
        draw.sprite(SPRITE, [0.0, 0.0, s, s], rect, None);
    }
}

/// Draws the world map's picture over the whole GUI (`gui`, GUI pixels), if
/// it is the one uploaded.
pub fn draw_world_map(draw: &mut dyn DrawContext, gui: [f32; 2]) {
    if let Some(Shown::WorldMap { width, height }) = with(|c| c.shown) {
        draw.sprite(
            SPRITE,
            [0.0, 0.0, width as f32, height as f32],
            [0.0, 0.0, gui[0], gui[1]],
            None,
        );
    }
}

/// Image side after fitting `want` physical pixels into the slot.
fn fit(want: f32) -> u32 {
    (want.round() as i64).clamp(1, SLOT as i64) as u32
}

fn argb_to_rgba(image: &[u32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(image.len() * 4);
    for p in image {
        out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, (p >> 24) as u8]);
    }
}

/// `over` drawn over `under`, both ARGB (straight alpha).
fn blend(under: u32, over: u32, alpha: f32) -> u32 {
    let a = ((over >> 24) as f32 / 255.0) * alpha;
    if a <= 0.0 {
        return under;
    }
    if a >= 1.0 {
        return over | 0xFF00_0000;
    }
    let ua = (under >> 24) as f32 / 255.0;
    let oa = a + ua * (1.0 - a);
    let ch = |s: u32| {
        let o = ((over >> s) & 0xFF) as f32;
        let u = ((under >> s) & 0xFF) as f32;
        let v = if oa > 0.0 { (o * a + u * ua * (1.0 - a)) / oa } else { 0.0 };
        (v.round() as u32).min(255)
    };
    (((oa * 255.0).round() as u32).min(255) << 24) | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

impl Canvas {
    fn take_minimap_uploads(&mut self) {
        let (size, full, cells) = {
            let mut u = gpu::UPLOADS.lock();
            if u.size == 0 {
                return;
            }
            if self.mini_size != u.size || !self.mini_ready {
                u.want_full = u.full.is_none();
            }
            (u.size, u.full.take(), std::mem::take(&mut u.cells))
        };
        if self.mini_size != size {
            self.mini_size = size;
            self.mini = vec![0; (size * size) as usize];
            self.mini_ready = false;
        }
        if let Some(full) = full
            && full.len() == self.mini.len()
        {
            self.mini.copy_from_slice(&full);
            self.mini_ready = true;
        }
        for (at, pixels) in cells {
            if at[0] + CELL > size || at[1] + CELL > size || pixels.len() != (CELL * CELL) as usize {
                continue;
            }
            for row in 0..CELL {
                let to = ((at[1] + row) * size + at[0]) as usize;
                let from = (row * CELL) as usize;
                self.mini[to..to + CELL as usize]
                    .copy_from_slice(&pixels[from..from + CELL as usize]);
            }
            // Cells change the picture even when the layout is the same.
            self.signature = 0;
        }
    }

    /// The minimap's picture: the former fragment shader, pixel by pixel.
    fn minimap(&mut self, f: &gpu::MapFrame, gui_scale: f32) -> Option<(u32, u32)> {
        if !self.mini_ready || f.size != self.mini_size || f.size == 0 {
            return None;
        }
        let physical = f.rect[2] * gui_scale;
        if physical < 2.0 {
            return None;
        }
        let side = fit(physical);
        // Physical pixels per image pixel.
        let k = physical / side as f32;
        let half = physical / 2.0;
        let texels_per_px = f.blocks_per_gui_px / f.scale / gui_scale;
        let signature = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for v in [
                physical, f.right[0], f.right[1], f.forward[0], f.forward[1], f.center[0],
                f.center[1], texels_per_px, f.opacity, f.scale, f.backdrop[0], f.backdrop[1],
                f.backdrop[2], f.backdrop[3], f.grid.unwrap_or(-1.0),
            ] {
                v.to_bits().hash(&mut h);
            }
            (f.size, f.wrap, f.round, f.origin, side).hash(&mut h);
            h.finish() | 1
        };
        let shown = Shown::Minimap { side };
        if self.shown == Some(shown) && self.signature == signature {
            return None;
        }
        let size = f.size as i32;
        let texel = |t: [i32; 2]| -> [f32; 4] {
            if t[0] < 0 || t[1] < 0 || t[0] >= size || t[1] >= size {
                return [0.0; 4];
            }
            let x = (t[0] + f.wrap[0] as i32) % size;
            let y = (t[1] + f.wrap[1] as i32) % size;
            let p = self.mini[(y * size + x) as usize];
            [
                ((p >> 16) & 0xFF) as f32 / 255.0,
                ((p >> 8) & 0xFF) as f32 / 255.0,
                (p & 0xFF) as f32 / 255.0,
                (p >> 24) as f32 / 255.0,
            ]
        };
        let n = (side * side) as usize;
        self.image.clear();
        self.image.resize(n, 0);
        let origin = [f.origin[0].rem_euclid(16) as f32, f.origin[1].rem_euclid(16) as f32];
        for py in 0..side {
            for px in 0..side {
                let lx = (px as f32 + 0.5) * k - half;
                let ly = (py as f32 + 0.5) * k - half;
                if f.round && (lx * lx + ly * ly).sqrt() > half {
                    continue;
                }
                let world = [
                    (f.right[0] * lx - f.forward[0] * ly) * texels_per_px + f.center[0],
                    (f.right[1] * lx - f.forward[1] * ly) * texels_per_px + f.center[1],
                ];
                let w = [world[0] - 0.5, world[1] - 0.5];
                let base = [w[0].floor() as i32, w[1].floor() as i32];
                let fr = [w[0] - w[0].floor(), w[1] - w[1].floor()];
                let t00 = texel(base);
                let t10 = texel([base[0] + 1, base[1]]);
                let t01 = texel([base[0], base[1] + 1]);
                let t11 = texel([base[0] + 1, base[1] + 1]);
                let on = |t: &[f32; 4]| if t[3] > 0.001 { 1.0 } else { 0.0 };
                let w00 = (1.0 - fr[0]) * (1.0 - fr[1]) * on(&t00);
                let w10 = fr[0] * (1.0 - fr[1]) * on(&t10);
                let w01 = (1.0 - fr[0]) * fr[1] * on(&t01);
                let w11 = fr[0] * fr[1] * on(&t11);
                let cover = w00 + w10 + w01 + w11;
                let mut color = f.backdrop;
                if cover > 0.0 {
                    for c in 0..3 {
                        let rgb = (t00[c] * w00 + t10[c] * w10 + t01[c] * w01 + t11[c] * w11) / cover;
                        color[c] = f.backdrop[c] + (rgb - f.backdrop[c]) * cover;
                    }
                    color[3] = f.backdrop[3] + (1.0 - f.backdrop[3]) * cover;
                }
                if let Some(line) = f.grid {
                    let g = |v: f32, o: f32| {
                        let m = ((v * f.scale + o) / 16.0).rem_euclid(1.0) * 16.0;
                        m.min(16.0 - m)
                    };
                    if g(world[0], origin[0]).min(g(world[1], origin[1])) < line {
                        for c in color.iter_mut().take(3) {
                            *c += (0.1 - *c) * 0.45;
                        }
                        color[3] = color[3].max(0.6);
                    }
                }
                let a = (color[3] * f.opacity).clamp(0.0, 1.0);
                let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
                self.image[(py * side + px) as usize] =
                    (b(a) << 24) | (b(color[0]) << 16) | (b(color[1]) << 8) | b(color[2]);
            }
        }
        argb_to_rgba(&self.image, &mut self.bytes_out);
        self.shown = Some(shown);
        self.signature = signature;
        Some((side, side))
    }

    /// The world map's picture: a dark backdrop, the Seed Atlas layer, the
    /// regions and the icons, each texel the nearest of its picture.
    fn world_map(&mut self, f: &gpu_map::WorldMapFrame, gui_scale: f32) -> Option<(u32, u32)> {
        if f.gui[0] <= 0.0 || f.gui[1] <= 0.0 {
            return None;
        }
        let want = [f.gui[0] * gui_scale, f.gui[1] * gui_scale];
        let k = (SLOT as f32 / want[0]).min(SLOT as f32 / want[1]).min(1.0);
        let (width, height) = (fit(want[0] * k), fit(want[1] * k));
        // Image pixels per GUI pixel.
        let scale = [width as f32 / f.gui[0], height as f32 / f.gui[1]];
        let physical_per_gui = gui_scale;

        // Which picture each quad draws, made or refreshed as needed.
        enum Draw {
            Entry(Key, [f32; 4], f32),
            Icon(usize, [f32; 4], f32),
        }
        let mut draws: Vec<Draw> = Vec::new();
        let layer = f.world ^ 0x5EED_A71A5;
        for r in &f.background {
            let key = (layer ^ ((r.layer as u64) << 40), r.pos[0], r.pos[1], r.size);
            if self.ensure(key, r, Some(r.alpha)) {
                draws.push(Draw::Entry(key, r.rect, 1.0));
            }
        }
        let seed_quads = draws.len();
        for r in &f.regions {
            let side = (r.rect[2] - r.rect[0]) * physical_per_gui;
            let want = level_for(side).min(r.size);
            let key = (f.world, r.pos[0], r.pos[1], want);
            let chosen = if self.ensure(key, r, None) {
                Some(key)
            } else {
                LODS.iter()
                    .map(|&l| (f.world, r.pos[0], r.pos[1], l))
                    .find(|k| self.entries.contains_key(k))
            };
            if let Some(key) = chosen {
                draws.push(Draw::Entry(key, r.rect, 1.0));
            }
        }
        if f.seed_above {
            let terrain = draws.len() - seed_quads;
            draws.rotate_left(seed_quads);
            debug_assert_eq!(draws.len(), terrain + seed_quads);
        }
        for icon in &f.icons {
            let r = icon.rect;
            draws.push(Draw::Icon(icon.icon, [r[0], r[1], r[0] + r[2], r[1] + r[3]], icon.alpha));
        }
        for d in &draws {
            if let Draw::Entry(key, _, _) = d
                && let Some(e) = self.entries.get_mut(key)
            {
                e.last_frame = self.frame;
            }
        }
        self.evict();

        let signature = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (width, height, f.world, f.seed_above).hash(&mut h);
            for d in &draws {
                match d {
                    Draw::Entry(key, rect, _) => {
                        key.hash(&mut h);
                        self.entries.get(key).map(|e| e.revision).hash(&mut h);
                        rect.map(f32::to_bits).hash(&mut h);
                    }
                    Draw::Icon(i, rect, a) => {
                        i.hash(&mut h);
                        rect.map(f32::to_bits).hash(&mut h);
                        a.to_bits().hash(&mut h);
                    }
                }
            }
            h.finish() | 1
        };
        let shown = Shown::WorldMap { width, height };
        if self.shown == Some(shown) && self.signature == signature {
            return None;
        }

        let n = (width * height) as usize;
        self.image.clear();
        self.image.resize(n, 0xFF08_0808);
        let icons = gpu_map::ICONS.get();
        for d in &draws {
            match d {
                Draw::Entry(key, rect, alpha) => {
                    if let Some(e) = self.entries.get(key) {
                        let pixels = e.pixels.clone();
                        blit(&mut self.image, width, height, *rect, scale, &pixels, e.side, e.side, *alpha);
                    }
                }
                Draw::Icon(i, rect, alpha) => {
                    if let Some((w, h, rgba)) = icons.and_then(|v| v.get(*i)) {
                        let argb: Vec<u32> = rgba
                            .chunks_exact(4)
                            .map(|p| {
                                (u32::from(p[3]) << 24)
                                    | (u32::from(p[0]) << 16)
                                    | (u32::from(p[1]) << 8)
                                    | u32::from(p[2])
                            })
                            .collect();
                        blit(&mut self.image, width, height, *rect, scale, &argb, *w, *h, *alpha);
                    }
                }
            }
        }
        argb_to_rgba(&self.image, &mut self.bytes_out);
        self.shown = Some(shown);
        self.signature = signature;
        Some((width, height))
    }

    /// Makes (or refreshes) the picture `key` from `r`'s pixels; whether it
    /// is there to draw.
    fn ensure(&mut self, key: Key, r: &gpu_map::RegionQuad, fixed: Option<u8>) -> bool {
        if self.entries.get(&key).is_some_and(|e| e.revision == r.revision) {
            return true;
        }
        let Some(pixels) = &r.pixels else {
            return self.entries.contains_key(&key);
        };
        let side = key.3;
        let picture: Vec<u32> = match fixed {
            None => downsample(pixels, side),
            Some(alpha) => pixels
                .iter()
                .map(|p| if p >> 24 == 0 { 0 } else { (u32::from(alpha) << 24) | (p & 0xFF_FFFF) })
                .collect(),
        };
        if picture.len() != (side * side) as usize {
            return false;
        }
        let bytes = picture.len() * 4;
        if let Some(old) = self.entries.insert(
            key,
            Entry {
                pixels: Arc::new(picture),
                side,
                revision: r.revision,
                last_frame: self.frame,
                seed: fixed.is_some(),
            },
        ) {
            self.bytes -= old.pixels.len() * 4;
        }
        self.bytes += bytes;
        self.has_seed |= fixed.is_some();
        true
    }

    fn release(&mut self, drop: impl Fn(&Entry) -> bool) {
        let bytes = &mut self.bytes;
        self.entries.retain(|_, e| {
            let gone = drop(e);
            if gone {
                *bytes -= e.pixels.len() * 4;
            }
            !gone
        });
        self.has_seed = self.entries.values().any(|e| e.seed);
        if matches!(self.shown, Some(Shown::WorldMap { .. })) {
            self.signature = 0;
        }
    }

    fn evict(&mut self) {
        if self.bytes <= CACHE_BYTES {
            return;
        }
        let mut by_age: Vec<(u64, Key)> =
            self.entries.iter().map(|(k, e)| (e.last_frame, *k)).collect();
        by_age.sort_unstable();
        for (last, key) in by_age {
            if self.bytes <= CACHE_BYTES * 3 / 4 || last == self.frame {
                break;
            }
            if let Some(e) = self.entries.remove(&key) {
                self.bytes -= e.pixels.len() * 4;
            }
        }
    }
}

/// Draws `pixels` (ARGB, `pw` x `ph`) stretched into `rect` (left, top,
/// right, bottom in GUI pixels), nearest texel, alpha-blended.
#[allow(clippy::too_many_arguments)]
fn blit(
    image: &mut [u32],
    width: u32,
    height: u32,
    rect: [f32; 4],
    scale: [f32; 2],
    pixels: &[u32],
    pw: u32,
    ph: u32,
    alpha: f32,
) {
    if pw == 0 || ph == 0 || pixels.len() != (pw * ph) as usize || alpha <= 0.0 {
        return;
    }
    let (x0, y0) = (rect[0] * scale[0], rect[1] * scale[1]);
    let (x1, y1) = (rect[2] * scale[0], rect[3] * scale[1]);
    if !(x1 > x0 && y1 > y0) {
        return;
    }
    let px0 = x0.round().max(0.0) as u32;
    let py0 = y0.round().max(0.0) as u32;
    let px1 = (x1.round().max(0.0) as u32).min(width);
    let py1 = (y1.round().max(0.0) as u32).min(height);
    if px0 >= px1 || py0 >= py1 {
        return;
    }
    let columns: Vec<u32> = (px0..px1)
        .map(|x| {
            let u = (x as f32 + 0.5 - x0) / (x1 - x0);
            ((u * pw as f32).floor() as i64).clamp(0, pw as i64 - 1) as u32
        })
        .collect();
    for y in py0..py1 {
        let v = (y as f32 + 0.5 - y0) / (y1 - y0);
        let ty = ((v * ph as f32).floor() as i64).clamp(0, ph as i64 - 1) as u32;
        let src = &pixels[(ty * pw) as usize..((ty + 1) * pw) as usize];
        let dst = &mut image[(y * width + px0) as usize..(y * width + px1) as usize];
        for (d, &tx) in dst.iter_mut().zip(&columns) {
            let p = src[tx as usize];
            if p >> 24 == 0 {
                continue;
            }
            *d = if p >> 24 == 0xFF && alpha >= 1.0 { p } else { blend(*d, p, alpha) };
        }
    }
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

/// A square region image at `lod` texels a side: each texel the mean of the
/// explored pixels it covers (opaque), transparent if none are.
fn downsample(pixels: &[u32], lod: u32) -> Vec<u32> {
    let side = (pixels.len() as f64).sqrt() as usize;
    let lod = (lod as usize).min(side.max(1));
    let mut out = vec![0u32; lod * lod];
    if side == 0 || side * side != pixels.len() || !side.is_multiple_of(lod) {
        return out;
    }
    if lod == side {
        for (o, p) in out.iter_mut().zip(pixels) {
            if p >> 24 != 0 {
                *o = p | 0xFF00_0000;
            }
        }
        return out;
    }
    let f = side / lod;
    for ty in 0..lod {
        for tx in 0..lod {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            for y in ty * f..(ty + 1) * f {
                for &p in &pixels[y * side + tx * f..y * side + (tx + 1) * f] {
                    if p >> 24 != 0 {
                        r += (p >> 16) & 0xFF;
                        g += (p >> 8) & 0xFF;
                        b += p & 0xFF;
                        n += 1;
                    }
                }
            }
            if let (Some(r), Some(g), Some(b)) = (r.checked_div(n), g.checked_div(n), b.checked_div(n)) {
                out[ty * lod + tx] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsampling_averages_explored_pixels() {
        let mut pixels = vec![0u32; 512 * 512];
        pixels[0] = 0xFF_10_20_30;
        pixels[1] = 0xFF_30_40_50;
        let out = downsample(&pixels, 128);
        assert_eq!(out.len(), 128 * 128);
        assert_eq!(out[0], 0xFF_20_30_40);
        assert_eq!(out[1], 0);
    }

    #[test]
    fn blit_draws_nearest_and_skips_transparent_texels() {
        let mut image = vec![0xFF00_0000u32; 4 * 4];
        let pixels = [0xFFFF_0000, 0, 0xFF00_FF00, 0xFF00_00FF];
        blit(&mut image, 4, 4, [0.0, 0.0, 4.0, 4.0], [1.0, 1.0], &pixels, 2, 2, 1.0);
        assert_eq!(image[0], 0xFFFF_0000);
        assert_eq!(image[2], 0xFF00_0000, "transparent texel leaves the backdrop");
        assert_eq!(image[8], 0xFF00_FF00);
        assert_eq!(image[15], 0xFF00_00FF);
    }

    #[test]
    fn blend_mixes_by_alpha() {
        assert_eq!(blend(0xFF00_0000, 0xFFFF_FFFF, 0.5), 0xFF80_8080);
        assert_eq!(blend(0xFF12_3456, 0x00FF_FFFF, 1.0), 0xFF12_3456);
    }

    #[test]
    fn slot_fits_large_windows() {
        assert_eq!(fit(3000.0), SLOT);
        assert_eq!(fit(0.2), 1);
        assert_eq!(fit(300.4), 300);
    }
}

//! Seed Atlas marker icons as the Fabric version 2.0 makes them: Minecraft's
//! own item and block textures where one fits, composed images for the
//! villager and warden heads, the chest and cracked mossy stone bricks, and
//! Seed Atlas's own icons for the rest (`StructureIcons.java`).
//!
//! Every icon is cropped to its visible pixels (`IconLayout.measure`), and
//! animated textures show their first frame.

use std::collections::HashMap;
use std::path::Path;

/// Icon pixels per marker at the default size (`IconLayout.DISPLAY_SIZE`).
pub const DISPLAY_SIZE: f32 = 18.0;

/// One icon, cropped: RGBA pixels, and whether it is a block texture (drawn
/// with a dark frame).
#[derive(Clone, Debug)]
pub struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub block: bool,
}

impl Icon {
    /// Display size in icon pixels, the longer side `DISPLAY_SIZE`.
    pub fn display(&self) -> (f32, f32) {
        let m = self.width.max(self.height).max(1) as f32;
        let w = ((self.width as f32 * DISPLAY_SIZE / m).round()).max(1.0);
        let h = ((self.height as f32 * DISPLAY_SIZE / m).round()).max(1.0);
        (w, h)
    }
}

#[derive(Clone, Copy)]
enum Source {
    Own(&'static [u8]),
    Item(&'static str),
    Block(&'static str, bool),
    Composed(fn(&Path) -> Option<Image>),
}

/// The icon of every marker id (and `orevein_copper`, `camp_special`).
fn sources() -> Vec<(&'static str, Source)> {
    use Source::*;
    vec![
        ("camp", Own(include_bytes!("assets/seedatlas/camp.png"))),
        ("camp_special", Own(include_bytes!("assets/seedatlas/camp_special.png"))),
        ("ancient_city", Composed(warden_head)),
        ("bastion", Block("cracked_polished_blackstone_bricks", true)),
        ("chambers", Item("trial_key")),
        ("desert", Block("chiseled_sandstone", true)),
        ("endcity", Block("purpur_block", true)),
        ("end_ship", Item("elytra")),
        ("fortress", Block("nether_bricks", true)),
        ("gateway", Item("ender_pearl")),
        ("geode", Item("amethyst_shard")),
        ("hut", Own(include_bytes!("assets/seedatlas/hut.png"))),
        ("igloo", Own(include_bytes!("assets/seedatlas/igloo.png"))),
        ("jungle", Block("mossy_cobblestone", true)),
        ("mansion", Item("totem_of_undying")),
        // The rail is transparent between its sleepers: no frame.
        ("mineshaft", Block("rail", false)),
        ("monument", Block("prismarine_bricks", true)),
        ("orevein", Item("raw_iron")),
        ("orevein_copper", Item("raw_copper")),
        ("outpost", Item("crossbow_standby")),
        ("portal", Block("crying_obsidian", true)),
        ("ruins", Composed(cracked_mossy_stone_bricks)),
        ("shipwreck", Own(include_bytes!("assets/seedatlas/shipwreck.png"))),
        ("slime", Item("slime_ball")),
        ("spawn", Item("compass_00")),
        ("stronghold", Item("ender_eye")),
        ("trails", Item("brush")),
        ("treasure", Composed(chest)),
        ("village", Composed(villager_head)),
        ("well", Own(include_bytes!("assets/seedatlas/well.png"))),
    ]
}

/// An RGBA image.
#[derive(Clone)]
struct Image {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Image {
    fn new(w: u32, h: u32) -> Self {
        Self { w, h, px: vec![0; (w * h * 4) as usize] }
    }
    fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    fn set(&mut self, x: u32, y: u32, p: [u8; 4]) {
        let i = ((y * self.w + x) * 4) as usize;
        self.px[i..i + 4].copy_from_slice(&p);
    }
}

fn decode(bytes: &[u8]) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let px: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        _ => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Some(Image { w: info.width, h: info.height, px })
}

fn texture(root: &Path, path: &str) -> Option<Image> {
    decode(&std::fs::read(root.join("assets/minecraft/textures").join(format!("{path}.png"))).ok()?)
}

/// Copies a `w` x `h` rectangle in 1/k texture units, skipping transparent
/// pixels so overlays layer (`StructureIcons.copy`).
#[allow(clippy::too_many_arguments)]
fn copy(src: &Image, u: u32, v: u32, w: u32, h: u32, k: u32, flip: bool, dst: &mut Image, x: u32, y: u32) {
    for dy in 0..h * k {
        let sy = v * k + if flip { h * k - 1 - dy } else { dy };
        for dx in 0..w * k {
            let (sx, ty) = (u * k + dx, y * k + dy);
            if sx >= src.w || sy >= src.h || x * k + dx >= dst.w || ty >= dst.h {
                continue;
            }
            let p = src.get(sx, sy);
            if p[3] != 0 {
                dst.set(x * k + dx, ty, p);
            }
        }
    }
}

fn cracked_mossy_stone_bricks(root: &Path) -> Option<Image> {
    let cracked = texture(root, "block/cracked_stone_bricks")?;
    let mossy = texture(root, "block/mossy_stone_bricks");
    let plain = texture(root, "block/stone_bricks");
    let size = cracked.w;
    let mut out = Image::new(size, size);
    for y in 0..size.min(cracked.h) {
        for x in 0..size {
            let mut p = cracked.get(x, y);
            if let (Some(m), Some(s)) = (&mossy, &plain)
                && m.w == size
                && s.w == size
                && m.get(x, y) != s.get(x, y)
            {
                p = m.get(x, y);
            }
            out.set(x, y, p);
        }
    }
    Some(out)
}

fn villager_head(root: &Path) -> Option<Image> {
    let skin = texture(root, "entity/villager/villager")?;
    let k = (skin.w / 64).max(1);
    let mut out = Image::new(8 * k, 11 * k);
    copy(&skin, 8, 8, 8, 10, k, false, &mut out, 0, 0);
    copy(&skin, 26, 2, 2, 4, k, false, &mut out, 3, 7);
    Some(out)
}

fn warden_head(root: &Path) -> Option<Image> {
    let skin = texture(root, "entity/warden/warden")?;
    let k = (skin.w / 128).max(1);
    let mut out = Image::new(28 * k, 20 * k);
    copy(&skin, 10, 42, 16, 16, k, false, &mut out, 6, 4);
    copy(&skin, 58, 38, 10, 10, k, false, &mut out, 0, 0);
    copy(&skin, 58, 6, 10, 10, k, false, &mut out, 18, 0);
    Some(out)
}

fn chest(root: &Path) -> Option<Image> {
    let skin = texture(root, "entity/chest/normal")?;
    let k = (skin.w / 64).max(1);
    let mut out = Image::new(14 * k, 15 * k);
    copy(&skin, 42, 14, 14, 5, k, true, &mut out, 0, 0);
    copy(&skin, 42, 33, 14, 10, k, true, &mut out, 0, 5);
    copy(&skin, 1, 1, 2, 4, k, false, &mut out, 6, 3);
    Some(out)
}

/// The first frame of an animated texture, cropped to its visible pixels
/// (alpha 8 and up, as `IconLayout.measure`).
fn crop(img: &Image) -> Image {
    let frame_h = if img.h > img.w && img.h % img.w == 0 { img.w } else { img.h };
    let (mut l, mut t, mut r, mut b) = (img.w, frame_h, 0u32, 0u32);
    let mut any = false;
    for y in 0..frame_h {
        for x in 0..img.w {
            if img.get(x, y)[3] >= 8 {
                any = true;
                l = l.min(x);
                t = t.min(y);
                r = r.max(x);
                b = b.max(y);
            }
        }
    }
    if !any {
        (l, t, r, b) = (0, 0, img.w - 1, frame_h - 1);
    }
    let mut out = Image::new(r - l + 1, b - t + 1);
    for y in t..=b {
        for x in l..=r {
            out.set(x - l, y - t, img.get(x, y));
        }
    }
    out
}

/// Loads every icon from the game's resources (`root`, the folder holding
/// `assets/`), Seed Atlas's own icons from the mod. A texture the game
/// lacks falls back to an empty icon.
pub fn load(root: Option<&Path>) -> HashMap<&'static str, Icon> {
    let mut out = HashMap::new();
    for (id, source) in sources() {
        let (img, block) = match source {
            Source::Own(bytes) => (decode(bytes), false),
            Source::Item(name) => (root.and_then(|r| texture(r, &format!("item/{name}"))), false),
            Source::Block(name, frame) => (root.and_then(|r| texture(r, &format!("block/{name}"))), frame),
            Source::Composed(f) => (root.and_then(f), false),
        };
        let img = img.map(|i| crop(&i)).unwrap_or_else(|| Image::new(1, 1));
        out.insert(id, Icon { width: img.w, height: img.h, rgba: img.px, block });
    }
    let done = decode(include_bytes!("assets/seedatlas/completed.png")).map(|i| crop(&i)).unwrap_or_else(|| Image::new(1, 1));
    out.insert("completed", Icon { width: done.w, height: done.h, rgba: done.px, block: false });
    out.insert("frame", Icon { width: 1, height: 1, rgba: vec![0x14, 0x14, 0x14, 0xC0], block: false });
    out
}

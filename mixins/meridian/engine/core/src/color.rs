//! Colour maths for map pixels.
//!
//! Colours are `0xRRGGBB` (`u32`) until the final pixel, which is
//! `0xAARRGGBB`. Everything here is a pure function so it is easy to test
//! and cheap to call per pixel.

use meridian_host::{BiomeAppearance, BlockAppearance, Tint};

use crate::config::{BlockColors, Slopes, TerrainStyle};

pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

pub const fn channels(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// Multiplies two colours channel by channel (a texture by a biome tint).
pub fn multiply(a: u32, b: u32) -> u32 {
    let [ar, ag, ab] = channels(a);
    let [br, bg, bb] = channels(b);
    let m = |x: u8, y: u8| ((x as u32 * y as u32 + 127) / 255) as u8;
    rgb(m(ar, br), m(ag, bg), m(ab, bb))
}

/// Scales brightness; `factor` is clamped to 0..=2 and channels saturate.
pub fn scale(c: u32, factor: f32) -> u32 {
    let f = factor.clamp(0.0, 2.0);
    let [r, g, b] = channels(c);
    let s = |x: u8| (x as f32 * f).round().clamp(0.0, 255.0) as u8;
    rgb(s(r), s(g), s(b))
}

/// `top` over `bottom` with opacity `alpha` (0..=1).
pub fn blend(bottom: u32, top: u32, alpha: f32) -> u32 {
    let a = alpha.clamp(0.0, 1.0);
    let [br, bg, bb] = channels(bottom);
    let [tr, tg, tb] = channels(top);
    let mix = |b: u8, t: u8| (b as f32 + (t as f32 - b as f32) * a).round() as u8;
    rgb(mix(br, tr), mix(bg, tg), mix(bb, tb))
}

/// The tint colour a block takes in a biome.
pub fn tint_color(tint: Tint, biome: &BiomeAppearance) -> Option<u32> {
    match tint {
        Tint::None => None,
        Tint::Grass => Some(biome.grass),
        Tint::Foliage => Some(biome.foliage),
        Tint::DryFoliage => Some(biome.dry_foliage),
        Tint::Water => Some(biome.water),
        Tint::Fixed(c) => Some(c),
    }
}

/// A block's flat colour in the "accurate" style: its texture, tinted.
pub fn accurate_color(block: &BlockAppearance, biome: &BiomeAppearance) -> u32 {
    match tint_color(block.tint, biome) {
        Some(t) => multiply(block.texture_color, t),
        None => block.texture_color,
    }
}

/// A block's flat colour in the vanilla map style; tinted blocks may take
/// their biome colour instead (`biome_colors_in_vanilla`).
pub fn vanilla_color(block: &BlockAppearance, biome: &BiomeAppearance, biome_colors: bool) -> u32 {
    if biome_colors && let Some(t) = tint_color(block.tint, biome) {
        // Keep the map colour's brightness, take the biome's hue.
        let luma = luminance(block.map_color) / luminance(t).max(1.0);
        return scale(t, luma.min(1.5));
    }
    block.map_color
}

fn luminance(c: u32) -> f32 {
    let [r, g, b] = channels(c);
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// How a surface pixel is shaded, from the settings and the time of day.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shading {
    pub colors: BlockColors,
    pub slopes: Slopes,
    pub depth: bool,
    pub lighting: bool,
    pub transparency: bool,
    /// Sky brightness 0..=1 (1 at noon).
    pub daylight: f32,
    /// A cave layer.
    pub cave: bool,
    /// Cave floors drawn bright enough to read (otherwise only torches
    /// light them).
    pub legible: bool,
    /// Ground below this Y is left out (a cave layer's depth).
    pub floor: Option<i32>,
    /// The dimension's light.
    pub sky: Sky,
    /// The cave layer's top: ground nearer to it is drawn brighter.
    pub ceiling: Option<i32>,
}

impl Shading {
    pub fn new(style: &TerrainStyle, day_time: Option<i64>) -> Self {
        Self {
            colors: style.block_colors,
            slopes: style.slopes,
            depth: style.terrain_depth,
            lighting: style.lighting,
            transparency: style.block_transparency,
            daylight: day_time.map_or(1.0, daylight),
            cave: false,
            legible: true,
            floor: None,
            sky: Sky::Overworld,
            ceiling: None,
        }
    }

    /// The cave layer's top (see [`Shading::ceiling`]).
    pub fn with_ceiling(mut self, ceiling: Option<i32>) -> Self {
        self.ceiling = ceiling;
        self
    }

    /// Brightening of cave ground by its depth below the layer: half again
    /// at the layer, none at the cave depth (as the reference's cave maps).
    pub fn cave_depth_factor(&self, height: i16) -> f32 {
        match (self.cave, self.ceiling, self.floor) {
            (true, Some(top), Some(floor)) if top > floor => {
                let near = (i32::from(height) - floor) as f32 / (top - floor) as f32;
                1.0 + 0.5 * near.clamp(0.0, 1.0)
            }
            _ => 1.0,
        }
    }

    /// The same shading in a dimension's light.
    pub fn in_sky(mut self, sky: Sky) -> Self {
        self.sky = sky;
        self
    }

    /// The same shading for a cave layer, legible or lit by its torches.
    pub fn for_cave(mut self, cave: bool, legible: bool) -> Self {
        self.cave = cave;
        self.legible = legible;
        self
    }

    /// Leaves out ground below `floor` (cave mode's depth below its layer).
    pub fn with_floor(mut self, floor: Option<i32>) -> Self {
        self.floor = floor;
        self
    }

    /// Whether two shadings give different pixels (and the map must be
    /// redrawn). Daylight counts in steps of 1/32 so dusk redraws gradually,
    /// not every tick.
    pub fn differs(&self, other: &Self) -> bool {
        let step = |d: f32| (d * 32.0).round() as i32;
        self.colors != other.colors
            || self.slopes != other.slopes
            || self.depth != other.depth
            || self.lighting != other.lighting
            || self.transparency != other.transparency
            || self.cave != other.cave
            || (self.cave && self.legible != other.legible)
            || self.floor != other.floor
            || self.sky != other.sky
            || self.ceiling != other.ceiling
            || (self.lighting && step(self.daylight) != step(other.daylight))
    }
}

/// Sky brightness for a time of day in ticks: 1 at noon (6000), 0 at
/// midnight (18000), following the sun's height.
pub fn daylight(day_time: i64) -> f32 {
    // The sun's angle as the game computes it (0 at noon, 0.5 at midnight),
    // then the sky's darkening from it; the map stays fully lit while the
    // sky is at least half bright and dims from there into the night.
    let d = (day_time.rem_euclid(24_000) as f32 / 24_000.0 - 0.25).rem_euclid(1.0);
    let angle = (d * 2.0 + (0.5 - (d * std::f32::consts::PI).cos() / 2.0)) / 3.0;
    let darken = (1.0 - ((angle * std::f32::consts::TAU).cos() * 2.0 + 0.2)).clamp(0.0, 1.0);
    ((1.0 - darken) * 2.0).clamp(0.0, 1.0)
}

/// Which light a dimension's map is drawn in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sky {
    #[default]
    Overworld,
    Nether,
    End,
}

impl Sky {
    pub fn of_dimension(id: &str) -> Self {
        match id {
            "minecraft:the_nether" => Sky::Nether,
            "minecraft:the_end" => Sky::End,
            _ => Sky::Overworld,
        }
    }
}

/// Light on the map at noon in full sky light, per channel: a little
/// brighter than the texture and slightly cool (measured on the reference
/// map, O3; see DECISIONS D-042).
pub const DAY_LIGHT: [f32; 3] = [1.0725, 1.1033, 1.1682];
/// The same at midnight; slopes no longer show.
pub const NIGHT_LIGHT: [f32; 3] = [0.227, 0.239, 0.262];
/// The End's light: neutral, as bright as the day's blue channel.
pub const END_LIGHT: [f32; 3] = [1.168, 1.168, 1.168];
/// Unlit Nether ground: dark and red.
pub const NETHER_DARK: [f32; 3] = [0.316, 0.237, 0.237];
/// Brightness one slope level adds or takes, in units of the texture colour.
pub const SLOPE_STEP: f32 = 0.059;
/// The level of ground lower than its north neighbour: it lies in shadow.
pub const SHADOW_LEVEL: i32 = -8;

/// The slope level of a column from its height and its north and
/// north-west neighbours' (`Smooth`): the light comes from the north,
/// 44.5 degrees up, and lands on the plane through the three columns;
/// ground lower than its north neighbour lies in shadow. `Strong` takes
/// half again as many levels. Unknown neighbours count as level with the
/// column. `Off` and `Vanilla` have no levels.
pub fn slope_level(
    slopes: Slopes,
    height: i16,
    north: Option<i16>,
    north_west: Option<i16>,
) -> i32 {
    let h = i32::from(height);
    let n = north.map_or(h, i32::from);
    let nw = north_west.map_or(n, i32::from);
    let level = || {
        let (rise, across) = (h - n, n - nw);
        if rise < 0 {
            SHADOW_LEVEL
        } else {
            lambert(across as f32, rise as f32).floor() as i32
        }
    };
    match slopes {
        Slopes::Off | Slopes::Vanilla => 0,
        Slopes::Smooth => level(),
        Slopes::Strong => (level() * 3).div_euclid(2),
    }
}

/// The light on a plane rising `q` blocks per block to the south and `p`
/// to the east, in slope levels before rounding (0 on flat ground).
fn lambert(p: f32, q: f32) -> f32 {
    const SIN: f32 = 0.700_909;
    const COS: f32 = 0.713_250;
    10.1 * ((SIN + q * COS) / (1.0 + p * p + q * q).sqrt() - SIN)
}

/// [`slope_level`] for planes between blocks (coarse map samples): not
/// rounded, and ground falling to the south shades toward the shadow level
/// over its first block, as the mix of shaded and lit blocks a coarse
/// sample stands for.
pub fn slope_light(p: f32, q: f32) -> f32 {
    if q >= 0.0 {
        lambert(p, q)
    } else {
        let t = (-q).min(1.0);
        lambert(p, 0.0) * (1.0 - t) + SHADOW_LEVEL as f32 * t
    }
}

/// The paper map's three steps against the north neighbour: higher is
/// brightest (`Vanilla` slopes).
pub fn vanilla_slope(height: i16, north: Option<i16>) -> f32 {
    match north
        .map_or(0, |n| i32::from(height) - i32::from(n))
        .signum()
    {
        1 => 1.0,
        0 => 220.0 / 255.0,
        _ => 180.0 / 255.0,
    }
}

/// Ground below y 63 is darker, by up to a tenth six blocks down.
pub fn depth_factor(height: i16) -> f32 {
    1.0 - 0.1 * ((63.0 - height as f32) / 6.0).clamp(0.0, 1.0)
}

/// The light on a column (per channel) and how strongly its slopes show
/// (0..=1), from its block and sky light, the dimension and the time.
pub fn column_light(sh: &Shading, block_light: u8, sky_light: u8) -> ([f32; 3], f32) {
    let lerp = |a: [f32; 3], b: [f32; 3], t: f32| {
        let t = t.clamp(0.0, 1.0);
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
    };
    let block = block_light.min(15) as f32 / 15.0;
    match (sh.lighting, sh.sky) {
        (_, Sky::End) => (END_LIGHT, 1.0),
        (false, _) => (DAY_LIGHT, 1.0),
        (true, sky) if sh.cave || sky == Sky::Nether => {
            let dark = if sky == Sky::Nether {
                NETHER_DARK
            } else {
                NIGHT_LIGHT
            };
            // Legible caves are drawn as if mostly lit; torches still
            // brighten them.
            let level = if sh.cave && sh.legible {
                block.max(0.75)
            } else {
                block
            };
            (lerp(dark, DAY_LIGHT, level), 0.5 + 0.5 * level)
        }
        (true, _) => {
            let sun = sky_light.min(15) as f32 / 15.0 * sh.daylight;
            let level = sun.max(block);
            (lerp(NIGHT_LIGHT, DAY_LIGHT, level), level)
        }
    }
}

/// `base` in `light`, `levels` slope levels brighter (or darker) and
/// scaled by `depth`; channels saturate.
pub fn lit(base: u32, light: [f32; 3], levels: f32, depth: f32) -> u32 {
    let [r, g, b] = channels(base);
    let c = |x: u8, i: usize| {
        (x as f32 * (light[i] + SLOPE_STEP * levels) * depth)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    rgb(c(r, 0), c(g, 1), c(b, 2))
}

/// How much of the ground shows through `depth` blocks of water (1 at one
/// block, less and less below).
pub fn water_floor_visibility(depth: u8) -> f32 {
    let d = depth.max(1) as f32;
    if d <= 13.0 {
        1.0 - 0.044 * (d - 1.0)
    } else {
        (0.472 - 0.012 * (d - 13.0)).max(0.0)
    }
}

/// A fluid's pixel: water is mostly its own colour, dimmed with the light,
/// with the lit ground showing through less the deeper it lies (only with
/// `transparency`); lava glows in its own light.
pub fn fluid(ground_lit: u32, fluid: u32, light: [f32; 3], depth: u8, transparency: bool) -> u32 {
    let [fr, fg, fb] = channels(fluid);
    if fr > fb {
        return lit(fluid, DAY_LIGHT, 0.0, 1.0);
    }
    let floor = if transparency {
        ground_lit
    } else {
        lit(fluid, light, 0.0, 1.0)
    };
    let seen = 0.28 * water_floor_visibility(depth);
    let [gr, gg, gb] = channels(floor);
    let c = |w: u8, g: u8, i: usize| {
        (0.70 * w as f32 * light[i] / DAY_LIGHT[i] + seen * g as f32)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    rgb(c(fr, gr, 0), c(fg, gg, 1), c(fb, gb, 2))
}

/// Packs an RGB colour into an opaque ARGB pixel.
pub const fn opaque(c: u32) -> u32 {
    0xFF00_0000 | (c & 0x00FF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAINS: BiomeAppearance = BiomeAppearance {
        grass: 0x91BD59,
        foliage: 0x77AB2F,
        dry_foliage: 0xA0A69C,
        water: 0x3F76E4,
    };

    #[test]
    fn multiply_by_white_and_black() {
        assert_eq!(multiply(0x123456, 0xFFFFFF), 0x123456);
        assert_eq!(multiply(0x123456, 0x000000), 0x000000);
        assert_eq!(multiply(0x808080, 0x808080), 0x404040);
    }

    #[test]
    fn scale_saturates() {
        assert_eq!(scale(0x808080, 2.0), 0xFFFFFF);
        assert_eq!(scale(0x808080, 0.5), 0x404040);
        assert_eq!(scale(0x808080, -3.0), 0);
    }

    #[test]
    fn blend_ends() {
        assert_eq!(blend(0x000000, 0xFFFFFF, 0.0), 0x000000);
        assert_eq!(blend(0x000000, 0xFFFFFF, 1.0), 0xFFFFFF);
        assert_eq!(blend(0x000000, 0xFFFFFF, 0.5), 0x808080);
    }

    #[test]
    fn grass_takes_biome_tint() {
        let grass = BlockAppearance {
            map_color: 0x7FB238,
            texture_color: 0x9D9D9D,
            tint: Tint::Grass,
            ..Default::default()
        };
        assert_eq!(
            accurate_color(&grass, &PLAINS),
            multiply(0x9D9D9D, 0x91BD59)
        );
        assert_eq!(vanilla_color(&grass, &PLAINS, false), 0x7FB238);
        assert_ne!(vanilla_color(&grass, &PLAINS, true), 0x7FB238);
        let stone = BlockAppearance {
            map_color: 0x707070,
            texture_color: 0x7D7D7D,
            ..Default::default()
        };
        assert_eq!(accurate_color(&stone, &PLAINS), 0x7D7D7D);
        assert_eq!(vanilla_color(&stone, &PLAINS, true), 0x707070);
    }

    #[test]
    fn vanilla_slopes_have_three_steps() {
        assert_eq!(vanilla_slope(70, Some(69)), 1.0);
        assert_eq!(vanilla_slope(70, Some(70)), 220.0 / 255.0);
        assert_eq!(vanilla_slope(70, Some(71)), 180.0 / 255.0);
        assert_eq!(slope_level(Slopes::Off, 70, Some(0), Some(0)), 0);
        assert_eq!(slope_level(Slopes::Vanilla, 70, Some(0), Some(0)), 0);
    }

    /// Levels measured on the reference map: planes rising `q` blocks per
    /// block to the south and `p` to the east (O3, D-042).
    #[test]
    fn smooth_slopes_match_the_reference() {
        let level = |p: i16, q: i16| {
            // The north neighbour is q lower, the north-west one p + q lower.
            slope_level(Slopes::Smooth, 100, Some(100 - q), Some(100 - q - p))
        };
        let table: [(i16, [i32; 5]); 5] = [
            (0, [0, -3, -4, -5, -6]),
            (1, [3, 1, -2, -3, -4]),
            (2, [2, 1, 0, -2, -3]),
            (3, [1, 1, 0, -1, -2]),
            (4, [1, 1, 0, -1, -1]),
        ];
        for (q, row) in table {
            for (p, want) in row.into_iter().enumerate() {
                assert_eq!(level(p as i16, q), want, "p {p} q {q}");
                assert_eq!(level(-(p as i16), q), want, "east and west alike");
            }
        }
        let steep: [(i16, [(i16, i32); 4]); 4] = [
            (0, [(5, -6), (8, -7), (16, -7), (0, 0)]),
            (5, [(5, -2), (10, -4), (16, -5), (0, 1)]),
            (8, [(0, 0), (6, -1), (12, -3), (16, -4)]),
            (12, [(0, 0), (6, -1), (10, -2), (16, -3)]),
        ];
        for (q, cells) in steep {
            for (p, want) in cells {
                assert_eq!(level(p, q), want, "p {p} q {q}");
            }
        }
        for q in 1..20 {
            assert_eq!(
                level(0, -q),
                SHADOW_LEVEL,
                "lower than the north lies in shadow"
            );
            assert_eq!(level(7, -q), SHADOW_LEVEL);
        }
        assert_eq!(
            slope_level(Slopes::Smooth, 64, None, None),
            0,
            "unknown is level"
        );
        assert!(slope_level(Slopes::Strong, 66, Some(64), Some(64)) < level(0, 2) * 2);
        assert_eq!(slope_level(Slopes::Strong, 65, Some(64), Some(64)), 4);
    }

    #[test]
    fn noon_light_is_the_measured_one() {
        let sh = Shading::new(&TerrainStyle::default(), Some(6_000));
        let (light, slopes) = column_light(&sh, 0, 15);
        assert_eq!(light, DAY_LIGHT);
        assert_eq!(slopes, 1.0);
        // White concrete (207, 213, 214): flat, one level up, in shadow.
        assert_eq!(lit(rgb(207, 213, 214), light, 0.0, 1.0), rgb(222, 235, 250));
        assert_eq!(lit(rgb(207, 213, 214), light, 1.0, 1.0), rgb(234, 248, 255));
        assert_eq!(
            lit(rgb(207, 213, 214), light, -8.0, 1.0),
            rgb(124, 134, 149)
        );
        // Midnight: dim and flat.
        let night = Shading::new(&TerrainStyle::default(), Some(18_000));
        let (light, slopes) = column_light(&night, 0, 15);
        assert!(slopes < 0.05 && light[0] < 0.3);
        // The End: neutral and bright, whatever the clock.
        let end = Shading::new(&TerrainStyle::default(), Some(18_000)).in_sky(Sky::End);
        assert_eq!(column_light(&end, 0, 0), (END_LIGHT, 1.0));
        assert_eq!(
            lit(rgb(220, 223, 158), END_LIGHT, 0.0, 1.0),
            rgb(255, 255, 185)
        );
        // Unlit Nether caves are dark, torches light them.
        let nether = Shading::new(&TerrainStyle::default(), None)
            .in_sky(Sky::Nether)
            .for_cave(true, false);
        assert_eq!(column_light(&nether, 0, 0).0, NETHER_DARK);
        assert!(column_light(&nether, 15, 0).0[0] > 1.0);
    }

    #[test]
    fn ground_below_sea_level_darkens_a_little() {
        assert_eq!(depth_factor(64), 1.0);
        assert_eq!(depth_factor(63), 1.0);
        assert!((depth_factor(60) - 0.95).abs() < 1e-6);
        assert!((depth_factor(57) - 0.9).abs() < 1e-6);
        assert!((depth_factor(-40) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn water_like_the_reference() {
        // Ocean water (water_still x the ocean tint) one block over white
        // concrete at noon: about (93, 123, 182) on the reference.
        let water = multiply(rgb(177, 177, 177), 0x3F76E4);
        let floor = lit(rgb(207, 213, 214), DAY_LIGHT, 0.0, 1.0);
        let [r, g, b] = channels(fluid(floor, water, DAY_LIGHT, 1, true));
        assert!(
            r.abs_diff(93) <= 2 && g.abs_diff(123) <= 2 && b.abs_diff(182) <= 2,
            "{r} {g} {b}"
        );
        // Over black concrete hardly anything shows: (33, 60, 116).
        let black = lit(rgb(8, 10, 15), DAY_LIGHT, 0.0, 1.0);
        let [r, g, b] = channels(fluid(black, water, DAY_LIGHT, 8, true));
        assert!(
            r.abs_diff(33) <= 2 && g.abs_diff(60) <= 2 && b.abs_diff(116) <= 2,
            "{r} {g} {b}"
        );
        // Deeper water shows less ground.
        assert!(
            luminance(fluid(floor, water, DAY_LIGHT, 13, true))
                < luminance(fluid(floor, water, DAY_LIGHT, 2, true))
        );
        // Lava glows.
        let lava = rgb(217, 105, 26);
        assert_eq!(
            fluid(0, lava, NIGHT_LIGHT, 3, true),
            lit(lava, DAY_LIGHT, 0.0, 1.0)
        );
    }

    #[test]
    fn daylight_curve() {
        assert!((daylight(6_000) - 1.0).abs() < 1e-6, "noon");
        assert_eq!(daylight(0), 1.0, "sunrise is already bright");
        assert!(daylight(18_000) < 0.5, "midnight is dark");
        assert!(
            daylight(13_000) < 1.0 && daylight(13_000) > daylight(18_000),
            "dusk dims"
        );
        assert_eq!(daylight(6_000 + 24_000 * 5), daylight(6_000));
        assert_eq!(daylight(-18_000), daylight(6_000));
    }

    #[test]
    fn shading_changes_by_daylight_steps() {
        let style = TerrainStyle::default();
        let noon = Shading::new(&style, Some(6_000));
        assert!(!noon.differs(&Shading::new(&style, Some(6_010))));
        assert!(noon.differs(&Shading::new(&style, Some(13_000))));
        let unlit = TerrainStyle {
            lighting: false,
            ..style
        };
        assert!(!Shading::new(&unlit, Some(0)).differs(&Shading::new(&unlit, Some(12_000))));
        assert!(
            noon.differs(&noon.in_sky(Sky::End)),
            "another dimension is redrawn"
        );
    }
}

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
        }
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

/// Brightness from a slope: `height` against its north and west neighbours.
pub fn slope_factor(slopes: Slopes, height: i16, north: Option<i16>, west: Option<i16>) -> f32 {
    let diff = |n: Option<i16>| n.map_or(0, |n| height as i32 - n as i32);
    match slopes {
        Slopes::Off => 1.0,
        // The paper map's three steps: higher than north is brightest.
        Slopes::Vanilla => match diff(north).signum() {
            1 => 1.0,
            0 => 220.0 / 255.0,
            _ => 180.0 / 255.0,
        },
        Slopes::Smooth | Slopes::Strong => {
            let d = (diff(north) + diff(west)) as f32 / 2.0;
            let gain = if slopes == Slopes::Strong {
                0.08
            } else {
                0.045
            };
            // Flat ground keeps its colour; rises brighten, drops darken.
            1.0 + (d * gain).clamp(-0.3, 0.15)
        }
    }
}

/// Slight brightness by absolute height, so plateaus read apart from
/// valleys: ±12 % over 128 blocks around sea level.
pub fn depth_factor(height: i16) -> f32 {
    1.0 + ((height as f32 - 64.0) / 128.0).clamp(-1.0, 1.0) * 0.12
}

/// Brightness from light: the brighter of block light and sky light, sky
/// light dimmed by the time of day; never fully black.
pub fn light_factor(block_light: u8, sky_light: u8, daylight: f32) -> f32 {
    let sky = sky_light.min(15) as f32 * daylight;
    let level = (block_light.min(15) as f32).max(sky) / 15.0;
    0.25 + 0.75 * level
}

/// Water over ground: opacity and darkening grow with depth.
pub fn water_over(ground: u32, water: u32, depth: u8, transparency: bool) -> u32 {
    let d = depth.min(16) as f32 / 16.0;
    let water = scale(water, 1.0 - 0.2 * d);
    if transparency {
        blend(ground, water, 0.5 + 0.35 * d)
    } else {
        water
    }
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
        assert_eq!(slope_factor(Slopes::Vanilla, 70, Some(69), None), 1.0);
        assert_eq!(
            slope_factor(Slopes::Vanilla, 70, Some(70), None),
            220.0 / 255.0
        );
        assert_eq!(
            slope_factor(Slopes::Vanilla, 70, Some(71), None),
            180.0 / 255.0
        );
        assert_eq!(slope_factor(Slopes::Off, 70, Some(0), Some(0)), 1.0);
    }

    #[test]
    fn smooth_slopes_are_bounded_and_monotonic() {
        let mut last = 0.0;
        for d in -40..=40 {
            let f = slope_factor(Slopes::Smooth, 64 + d, Some(64), Some(64));
            assert!((0.7 - 1e-6..=1.15 + 1e-6).contains(&f), "{f}");
            if d == 0 {
                assert_eq!(f, 1.0, "flat ground keeps its colour");
            }
            assert!(f >= last);
            last = f;
        }
        assert!(
            slope_factor(Slopes::Strong, 66, Some(64), Some(64))
                > slope_factor(Slopes::Smooth, 66, Some(64), Some(64))
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
    fn light_never_black() {
        assert_eq!(light_factor(0, 0, 1.0), 0.25);
        assert_eq!(light_factor(0, 15, 1.0), 1.0);
        assert_eq!(light_factor(14, 15, 0.0), 0.25 + 0.75 * 14.0 / 15.0);
    }

    #[test]
    fn deeper_water_is_darker() {
        let shallow = water_over(0xC2B280, 0x3F76E4, 1, true);
        let deep = water_over(0xC2B280, 0x3F76E4, 12, true);
        assert!(luminance(deep) < luminance(shallow));
        assert_eq!(
            water_over(0xC2B280, 0x3F76E4, 3, false),
            scale(0x3F76E4, 1.0 - 0.2 * 3.0 / 16.0)
        );
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
    }
}

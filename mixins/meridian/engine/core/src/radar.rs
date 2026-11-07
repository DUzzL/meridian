//! The entity radar on the minimap: which entities show, in what colour and
//! order, and their diamond-shaped dots.
//!
//! Defaults follow the reference mod's first-start categories (O2): living
//! things yellow, players white, items red, other entities purple; dots
//! two units across, entities more than 20 blocks above or below left out
//! (players almost never), darker the further up or down they are; items
//! drawn first, then living things, hostile ones, players last.

use crate::config::{NameMode, RadarSettings};
use crate::minimap::{Axes, DrawOp, Layout};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    Player,
    Hostile,
    Friendly,
    Item,
    Other,
}

impl Class {
    /// The chat colour index the reference's categories use.
    pub fn rgb(self) -> u32 {
        match self {
            Class::Player => 0xFFFFFF,
            Class::Hostile | Class::Friendly => 0xFFFF55,
            Class::Item => 0xFF5555,
            Class::Other => 0xAA00AA,
        }
    }

    /// Drawing order: lower first.
    fn order(self) -> u8 {
        match self {
            Class::Other => 0,
            Class::Item => 1,
            Class::Friendly => 2,
            Class::Hostile => 3,
            Class::Player => 6,
        }
    }

    fn height_limit(self, settings: &RadarSettings) -> f64 {
        if self == Class::Player {
            2050.0
        } else {
            settings.height_limit as f64
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub id: u32,
    pub class: Class,
    pub pos: [f64; 3],
    pub invisible: bool,
    /// `minecraft:zombie`...
    pub kind: String,
    /// A player's name or a custom name.
    pub name: Option<String>,
}

/// Where a mob's face is in its texture: the front of its model's head
/// cube (texture path under `textures/`, and u, v, width, height in texels
/// of a 64-wide texture).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub texture: &'static str,
    pub uv: [u16; 4],
}

/// The face of an entity kind, for kinds whose model has a plain head.
pub fn face(kind: &str) -> Option<Face> {
    let id = kind.strip_prefix("minecraft:").unwrap_or(kind);
    let f = |texture, uv| Some(Face { texture, uv });
    // Humanoid heads: 8x8x8 at (0, 0); villagers and illagers 8x10x8.
    const HEAD: [u16; 4] = [8, 8, 8, 8];
    const LONG: [u16; 4] = [8, 8, 8, 10];
    match id {
        "zombie" => f("entity/zombie/zombie", HEAD),
        "husk" => f("entity/zombie/husk", HEAD),
        "drowned" => f("entity/zombie/drowned", HEAD),
        "skeleton" => f("entity/skeleton/skeleton", HEAD),
        "stray" => f("entity/skeleton/stray", HEAD),
        "wither_skeleton" => f("entity/skeleton/wither_skeleton", HEAD),
        "bogged" => f("entity/skeleton/bogged", HEAD),
        "creeper" => f("entity/creeper/creeper", HEAD),
        "enderman" => f("entity/enderman/enderman", HEAD),
        "blaze" => f("entity/blaze", HEAD),
        "slime" => f("entity/slime/slime", HEAD),
        "spider" => f("entity/spider/spider", [40, 12, 8, 8]),
        "cave_spider" => f("entity/spider/cave_spider", [40, 12, 8, 8]),
        "pig" => f("entity/pig/pig_temperate", HEAD),
        "cow" => f("entity/cow/cow_temperate", [6, 6, 8, 8]),
        "sheep" => f("entity/sheep/sheep", [8, 8, 6, 6]),
        "chicken" => f("entity/chicken/chicken_temperate", [3, 3, 4, 6]),
        "villager" => f("entity/villager/villager", LONG),
        "zombie_villager" => f("entity/zombie_villager/zombie_villager", LONG),
        "witch" => f("entity/witch", LONG),
        "pillager" => f("entity/illager/pillager", LONG),
        "vindicator" => f("entity/illager/vindicator", LONG),
        "evoker" => f("entity/illager/evoker", LONG),
        _ => None,
    }
}

/// Whether the radar shows an entity, before position.
pub fn shown(e: &Entity, s: &RadarSettings) -> bool {
    if (s.hide_invisible && e.invisible) || s.excluded.contains(&e.kind) {
        return false;
    }
    let wanted = match e.class {
        Class::Player => s.players,
        Class::Hostile => s.hostile,
        Class::Friendly => s.friendly,
        Class::Item => s.items,
        Class::Other => s.other,
    };
    wanted != s.reversed
}

/// A colour darkened toward black by the height difference `dy` against
/// `limit` (at the limit, a third of its brightness remains).
pub fn faded(rgb: u32, dy: f64, limit: f64) -> u32 {
    let f = 1.0 - (dy.abs() / limit.max(1.0)).min(1.0) * 0.67;
    crate::color::scale(rgb, f as f32)
}

/// The radar's dots for `entities` around the player at `center`.
pub fn dots(
    layout: &Layout,
    center: [f64; 3],
    yaw: f32,
    entities: &[Entity],
    s: &RadarSettings,
) -> Vec<DrawOp> {
    if !s.enabled {
        return Vec::new();
    }
    let axes = Axes::new(layout.rotate, yaw);
    let u = layout.unit;
    let r = s.dot_size.max(1) as f32 * u;
    let half = layout.size as f64 / 2.0 - r as f64;
    let [cx, cy] = layout.center();
    let mut visible: Vec<&Entity> = entities
        .iter()
        .filter(|e| shown(e, s))
        .filter(|e| (e.pos[1] - center[1]).abs() <= e.class.height_limit(s))
        .collect();
    visible.sort_by_key(|e| (e.class.order(), e.id));
    visible.truncate(s.max_entities as usize);
    let mut ops = Vec::with_capacity(visible.len() * 2);
    for e in visible {
        let [dx, dy] = axes.to_screen(
            e.pos[0] - center[0],
            e.pos[2] - center[2],
            layout.blocks_per_pixel as f64,
        );
        let inside = if layout.round {
            (dx * dx + dy * dy).sqrt() <= half
        } else {
            dx.abs() <= half && dy.abs() <= half
        };
        if !inside {
            continue;
        }
        let (x, y) = (cx + dx as f32, cy + dy as f32);
        let height = e.pos[1] - center[1];
        let rgb = if s.height_fade {
            faded(e.class.rgb(), height, e.class.height_limit(s))
        } else {
            e.class.rgb()
        };
        let face = if s.faces { face(&e.kind) } else { None };
        if let Some(face) = face {
            // The face, a little larger than a dot, on a dark rim.
            let half = r * 1.6;
            let rim = 0.6 * u;
            ops.push(DrawOp::Fill {
                rect: [
                    x - half - rim,
                    y - half - rim,
                    2.0 * (half + rim),
                    2.0 * (half + rim),
                ],
                argb: 0xFF00_0000,
            });
            let shade = rgb_to_shade(e.class.rgb(), rgb);
            ops.push(DrawOp::Blit {
                texture: face.texture.to_owned(),
                rect: [x - half, y - half, 2.0 * half, 2.0 * half],
                uv: face.uv,
                argb: 0xFF00_0000 | shade,
            });
        } else {
            let diamond = |r: f32| [[x, y - r], [x + r, y], [x, y + r], [x - r, y]];
            ops.push(DrawOp::Quad {
                points: diamond(r + u * 0.75),
                argb: 0xFF00_0000,
            });
            ops.push(DrawOp::Quad {
                points: diamond(r),
                argb: 0xFF00_0000 | rgb,
            });
        }
        let named = match s.names {
            NameMode::Off => false,
            NameMode::Players => e.class == Class::Player,
            NameMode::All => true,
        };
        if named && let Some(name) = &e.name {
            let scale = 0.5 * s.name_scale.clamp(0.25, 4.0) * (u / 1.5).max(0.5);
            ops.push(DrawOp::Text {
                x,
                y: y + r + u,
                text: name.clone(),
                argb: 0xFF00_0000 | rgb,
                align: crate::minimap::Align::Center,
                scale,
            });
        }
    }
    ops
}

/// The grey a face is tinted with to show the same height fade as a dot
/// (`faded` against the class colour).
fn rgb_to_shade(base: u32, faded: u32) -> u32 {
    let lum = |c: u32| ((c >> 16) & 0xFF) + ((c >> 8) & 0xFF) + (c & 0xFF);
    let k = if lum(base) == 0 {
        255
    } else {
        (lum(faded) * 255 / lum(base)).min(255)
    };
    k << 16 | k << 8 | k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            x: 0.0,
            y: 0.0,
            size: 100.0,
            blocks_per_pixel: 1.0,
            rotate: false,
            round: false,
            info_below: true,
            unit: 1.0,
        }
    }

    fn e(id: u32, class: Class, pos: [f64; 3]) -> Entity {
        Entity {
            id,
            class,
            pos,
            invisible: false,
            kind: "minecraft:zombie".into(),
            name: None,
        }
    }

    #[test]
    fn colours_like_the_reference() {
        assert_eq!(Class::Friendly.rgb(), 0xFFFF55);
        assert_eq!(
            Class::Hostile.rgb(),
            0xFFFF55,
            "hostile mobs share the living colour"
        );
        assert_eq!(Class::Player.rgb(), 0xFFFFFF);
        assert_eq!(Class::Item.rgb(), 0xFF5555);
    }

    #[test]
    fn filters_and_reverse() {
        let s = RadarSettings::default();
        let zombie = e(1, Class::Hostile, [0.0; 3]);
        assert!(shown(&zombie, &s));
        let hidden = RadarSettings {
            hostile: false,
            ..s.clone()
        };
        assert!(!shown(&zombie, &hidden));
        let reversed = RadarSettings {
            reversed: true,
            ..hidden
        };
        assert!(
            shown(&zombie, &reversed),
            "reversing shows what the filter hides"
        );
        let ghost = Entity {
            invisible: true,
            ..zombie
        };
        assert!(!shown(&ghost, &s));
    }

    #[test]
    fn height_limit_and_fade() {
        let s = RadarSettings::default();
        let ents = [
            e(1, Class::Friendly, [5.0, 64.0, 5.0]),
            e(2, Class::Friendly, [5.0, 90.0, 5.0]),
            e(3, Class::Player, [5.0, 200.0, 5.0]),
        ];
        let ops = dots(&layout(), [0.0, 64.0, 0.0], 0.0, &ents, &s);
        assert_eq!(
            ops.len(),
            4,
            "the cow 26 blocks up is left out, the player is not"
        );
        assert!(faded(0xFFFF55, 10.0, 20.0) < 0xFFFF55);
        assert_eq!(faded(0xFFFF55, 0.0, 20.0), 0xFFFF55);
    }

    #[test]
    fn order_and_clipping() {
        let s = RadarSettings::default();
        let ents = [
            e(1, Class::Player, [1.0, 64.0, 0.0]),
            e(2, Class::Item, [2.0, 64.0, 0.0]),
            e(3, Class::Friendly, [500.0, 64.0, 0.0]),
        ];
        let ops = dots(&layout(), [0.0, 64.0, 0.0], 0.0, &ents, &s);
        assert_eq!(ops.len(), 4, "the far one is off the map");
        let DrawOp::Quad { argb, .. } = ops[1] else {
            panic!()
        };
        assert_eq!(argb, 0xFFFF_5555, "items first, players over them");
        let disabled = RadarSettings {
            enabled: false,
            ..s
        };
        assert!(dots(&layout(), [0.0; 3], 0.0, &ents, &disabled).is_empty());
    }

    #[test]
    fn excluded_kinds_never_show() {
        let mut s = RadarSettings::default();
        let z = e(1, Class::Hostile, [0.0; 3]);
        assert!(shown(&z, &s));
        s.excluded.insert("minecraft:zombie".into());
        assert!(!shown(&z, &s));
        s.reversed = true;
        assert!(!shown(&z, &s), "not even reversed");
    }

    #[test]
    fn faces_and_names() {
        let mut s = RadarSettings::default();
        let mut zombie = e(1, Class::Hostile, [5.0, 64.0, 0.0]);
        let mut steve = e(2, Class::Player, [-5.0, 64.0, 0.0]);
        steve.kind = "minecraft:player".into();
        steve.name = Some("Steve".into());
        zombie.name = Some("Bob".into());
        let ops =
            |s: &RadarSettings, list: &[Entity]| dots(&layout(), [0.0, 64.0, 0.0], 0.0, list, s);
        let list = [zombie.clone(), steve.clone()];
        let texts = |ops: &[DrawOp]| {
            ops.iter()
                .filter_map(|o| {
                    if let DrawOp::Text { text, .. } = o {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            texts(&ops(&s, &list)),
            ["Steve"],
            "players' names by default"
        );
        s.names = NameMode::All;
        assert_eq!(texts(&ops(&s, &list)).len(), 2);
        s.names = NameMode::Off;
        assert!(texts(&ops(&s, &list)).is_empty());
        assert!(
            !ops(&s, &list)
                .iter()
                .any(|o| matches!(o, DrawOp::Blit { .. })),
            "dots by default"
        );
        s.faces = true;
        let blits: Vec<_> = ops(&s, &list)
            .into_iter()
            .filter(|o| matches!(o, DrawOp::Blit { .. }))
            .collect();
        assert_eq!(blits.len(), 1, "the zombie's face; the player stays a dot");
        assert!(
            matches!(&blits[0], DrawOp::Blit { texture, uv: [8, 8, 8, 8], .. } if texture == "entity/zombie/zombie")
        );
        assert!(face("minecraft:ender_dragon").is_none());
    }
}

//! The entity radar on the minimap: which entities show, in what colour and
//! order, and their diamond-shaped dots.
//!
//! Defaults follow the reference mod's first-start categories (O2): living
//! things yellow, players white, items red, other entities purple; dots
//! two units across, entities more than 20 blocks above or below left out
//! (players almost never), darker the further up or down they are; items
//! drawn first, then living things, hostile ones, players last.

use crate::config::RadarSettings;
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
}

/// Whether the radar shows an entity, before position.
pub fn shown(e: &Entity, s: &RadarSettings) -> bool {
    if s.hide_invisible && e.invisible {
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
    ops
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
}

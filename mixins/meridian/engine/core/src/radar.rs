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
    /// Its face, when the radar shows faces (see [`face_for`]).
    pub face: Option<Face>,
    /// A tamed animal: drawn over the wild ones.
    pub tamed: bool,
}

impl Entity {
    /// Drawing order, lower first (the reference's category render order:
    /// other 0, items 1, friendly 2, hostile 3, tamed friendly 4, tamed
    /// hostile 5, players 6).
    fn order(&self) -> u8 {
        match (self.class, self.tamed) {
            (Class::Friendly, true) => 4,
            (Class::Hostile, true) => 5,
            (c, _) => c.order(),
        }
    }
}

/// Where a mob's face is in its texture: the front of its model's head
/// cube (texture path under `textures/`, and u, v, width, height in texels
/// of a 64-wide texture).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Face {
    pub texture: String,
    /// The head's front face: u, v, width, height in texels of the vanilla
    /// texture, `base` texels wide (a higher-resolution pack scales it).
    pub uv: [u16; 4],
    pub base: u16,
}

/// Humanoid heads: 8x8x8 at (0, 0).
const HEAD: [u16; 4] = [8, 8, 8, 8];

/// The face of an entity: a player's from their skin (the default skin of
/// their id when the game has no other texture for it), else its kind's.
pub fn face_for(kind: &str, skin: Option<&str>) -> Option<Face> {
    if kind == "minecraft:player" {
        return Some(Face {
            texture: skin.unwrap_or("entity/player/wide/steve").to_owned(),
            uv: HEAD,
            base: 64,
        });
    }
    face(kind)
}

/// Where a mob's face is in its texture: the front of its model's head
/// cube (the body's front where there is no head), checked against the
/// game's model export. Kinds whose face is no single rectangle (magma
/// cube) or that never reach the radar (the dragon) have none.
pub fn face(kind: &str) -> Option<Face> {
    let id = kind.strip_prefix("minecraft:").unwrap_or(kind);
    // `f`: a 64-wide texture; `w`: another width.
    let w = |texture: &str, base: u16, uv: [u16; 4]| {
        Some(Face {
            texture: format!("entity/{texture}"),
            uv,
            base,
        })
    };
    let f = |texture: &str, uv| w(texture, 64, uv);
    // Villagers and illagers 8x10x8; piglins 10x8x8; horses 6x5.
    const LONG: [u16; 4] = [8, 8, 8, 10];
    const WIDE: [u16; 4] = [8, 8, 10, 8];
    const HORSE: [u16; 4] = [7, 20, 6, 5];
    match id {
        // Undead and other humanoids.
        "zombie" | "giant" => f("zombie/zombie", HEAD),
        "husk" => f("zombie/husk", HEAD),
        "drowned" => f("zombie/drowned", HEAD),
        "skeleton" => f("skeleton/skeleton", HEAD),
        "stray" => f("skeleton/stray", HEAD),
        "wither_skeleton" => f("skeleton/wither_skeleton", HEAD),
        "bogged" => f("skeleton/bogged", HEAD),
        "parched" => f("skeleton/parched", HEAD),
        "creeper" => f("creeper/creeper", HEAD),
        "enderman" => f("enderman/enderman", HEAD),
        "blaze" => f("blaze/blaze", HEAD),
        "breeze" => w("breeze/breeze", 32, HEAD),
        "wither" => f("wither/wither", HEAD),
        "slime" => f("slime/slime", HEAD),
        "spider" => f("spider/spider", [40, 12, 8, 8]),
        "cave_spider" => f("spider/cave_spider", [40, 12, 8, 8]),
        "villager" => f("villager/villager", LONG),
        "zombie_villager" => f("zombie_villager/zombie_villager", LONG),
        "wandering_trader" => f("wandering_trader/wandering_trader", LONG),
        "witch" => f("witch/witch", LONG),
        "pillager" => f("illager/pillager", LONG),
        "vindicator" => f("illager/vindicator", LONG),
        "evoker" => f("illager/evoker", LONG),
        "illusioner" => f("illager/illusioner", LONG),
        "vex" => w("illager/vex", 32, [5, 5, 5, 5]),
        "ravager" => w("illager/ravager", 128, [16, 16, 16, 20]),
        "piglin" => f("piglin/piglin", WIDE),
        "piglin_brute" => f("piglin/piglin_brute", WIDE),
        "zombified_piglin" => f("piglin/zombified_piglin", WIDE),
        "hoglin" => w("hoglin/hoglin", 128, [80, 20, 14, 6]),
        "zoglin" => w("hoglin/zoglin", 128, [80, 20, 14, 6]),
        "creaking" => f("creaking/creaking", [6, 6, 6, 10]),
        "warden" => w("warden/warden", 128, [10, 42, 16, 16]),
        "phantom" => f("phantom/phantom", [5, 5, 7, 3]),
        "ghast" => w("ghast/ghast", 128, [32, 32, 32, 32]),
        "happy_ghast" => w("ghast/happy_ghast", 128, [32, 32, 32, 32]),
        "guardian" => f("guardian/guardian", [16, 16, 12, 12]),
        "elder_guardian" => f("guardian/guardian_elder", [16, 16, 12, 12]),
        // The head inside the shell.
        "shulker" => f("shulker/shulker", [6, 58, 6, 6]),
        "endermite" => f("endermite/endermite", [2, 2, 4, 3]),
        "silverfish" => f("silverfish/silverfish", [2, 2, 3, 2]),
        "sulfur_cube" => w("sulfur_cube/sulfur_cube_outer", 128, [18, 18, 18, 18]),
        // Golems.
        "snow_golem" => f("snow_golem/snow_golem", HEAD),
        "iron_golem" => w("iron_golem/iron_golem", 128, [8, 8, 8, 10]),
        "copper_golem" => f("copper_golem/copper_golem", [10, 10, 8, 5]),
        // Farm animals.
        "pig" => f("pig/pig_temperate", HEAD),
        "cow" => f("cow/cow_temperate", [6, 6, 8, 8]),
        "mooshroom" => f("cow/mooshroom_red", [6, 6, 8, 8]),
        "sheep" => f("sheep/sheep", [8, 8, 6, 6]),
        "chicken" => f("chicken/chicken_temperate", [3, 3, 4, 6]),
        "goat" => f("goat/goat", [44, 56, 5, 7]),
        "rabbit" => f("rabbit/rabbit_brown", [5, 21, 5, 5]),
        // Bees have no head: the front of the body.
        "bee" => f("bee/bee", [10, 10, 7, 7]),
        "horse" => f("horse/horse_brown", HORSE),
        "donkey" => f("horse/donkey", HORSE),
        "mule" => f("horse/mule", HORSE),
        "skeleton_horse" => f("horse/horse_skeleton", HORSE),
        "zombie_horse" => f("horse/horse_zombie", HORSE),
        // The top of the head front, with the eyes.
        "llama" | "trader_llama" => w("llama/llama_creamy", 128, [6, 20, 8, 8]),
        "camel" => w("camel/camel", 128, [28, 7, 7, 14]),
        "camel_husk" => w("camel/camel_husk", 128, [28, 7, 7, 14]),
        // The lower beak (the head's front is see-through).
        "sniffer" => w("sniffer/sniffer", 192, [19, 66, 13, 12]),
        // Pets and wild animals.
        "wolf" => w("wolf/wolf", 64, [4, 4, 6, 6]),
        "cat" => f("cat/cat_tabby", [5, 5, 5, 4]),
        "ocelot" => f("cat/ocelot", [5, 5, 5, 4]),
        "fox" => w("fox/fox", 48, [7, 11, 8, 6]),
        "frog" => w("frog/frog_temperate", 48, [9, 22, 7, 3]),
        "panda" => f("panda/panda", [9, 15, 13, 10]),
        "polar_bear" => w("bear/polarbear", 128, [7, 7, 7, 7]),
        "armadillo" => f("armadillo/armadillo", [45, 17, 3, 5]),
        "parrot" => w("parrot/parrot_red_blue", 32, [4, 4, 2, 3]),
        "allay" => w("allay/allay", 32, [5, 5, 5, 5]),
        "bat" => w("bat/bat", 32, [2, 9, 4, 3]),
        // The face is the body's front.
        "strider" => f("strider/strider", [16, 16, 16, 14]),
        // Water.
        "turtle" => w("turtle/turtle", 128, [9, 6, 6, 5]),
        "axolotl" => f("axolotl/axolotl_lucy", [5, 6, 8, 5]),
        "dolphin" => f("dolphin/dolphin", [6, 6, 8, 7]),
        "squid" => f("squid/squid", [12, 12, 12, 16]),
        "glow_squid" => f("squid/glow_squid", [12, 12, 12, 16]),
        "cod" => w("fish/cod", 32, [14, 3, 2, 4]),
        "salmon" => w("fish/salmon", 32, [25, 3, 2, 4]),
        "pufferfish" => w("fish/pufferfish", 32, [8, 8, 8, 8]),
        "tropical_fish" => w("fish/tropical_a", 32, [6, 6, 2, 3]),
        "tadpole" => w("tadpole/tadpole", 16, [3, 3, 3, 2]),
        // The side of the shell, with the eye.
        "nautilus" => w("nautilus/nautilus", 128, [24, 68, 14, 8]),
        "zombie_nautilus" => w("nautilus/zombie_nautilus", 128, [24, 68, 14, 8]),
        // Mannequins wear the player model.
        "mannequin" => f("player/wide/steve", HEAD),
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
    visible.sort_by_key(|e| (e.order(), e.id));
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
        let face = if s.faces { e.face.as_ref() } else { None };
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
                texture: face.texture.clone(),
                rect: [x - half, y - half, 2.0 * half, 2.0 * half],
                uv: face.uv,
                base: face.base,
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
            face: face_for("minecraft:zombie", None),
            tamed: false,
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
    fn tamed_animals_over_wild_ones() {
        let s = RadarSettings::default();
        let mut dog = e(1, Class::Friendly, [1.0, 64.0, 0.0]);
        dog.tamed = true;
        let zombie = e(2, Class::Hostile, [2.0, 64.0, 0.0]);
        let ops = dots(&layout(), [0.0, 64.0, 0.0], 0.0, &[dog, zombie], &s);
        // Dots come in pairs (rim, colour); the tamed friend is drawn last.
        let DrawOp::Quad { points, .. } = &ops[3] else {
            panic!()
        };
        assert_eq!(
            points[0][0],
            1.0 * layout().blocks_per_pixel + 50.0,
            "the dog over the zombie"
        );
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
    fn faces_lie_inside_their_textures() {
        let kinds = [
            "bee",
            "wolf",
            "ghast",
            "iron_golem",
            "polar_bear",
            "sniffer",
            "fox",
            "tadpole",
            "llama",
            "warden",
            "camel",
            "turtle",
            "parrot",
            "zombie",
        ];
        for k in kinds {
            let f = face(k).unwrap_or_else(|| panic!("{k} has a face"));
            let [u, _, w, _] = f.uv;
            assert!(u + w <= f.base, "{k}: {:?} outside {} wide", f.uv, f.base);
            assert!(f.texture.starts_with("entity/"));
        }
        assert!(face("magma_cube").is_none() && face("ender_dragon").is_none());
    }

    #[test]
    fn faces_and_names() {
        let mut s = RadarSettings::default();
        let mut zombie = e(1, Class::Hostile, [5.0, 64.0, 0.0]);
        let mut steve = e(2, Class::Player, [-5.0, 64.0, 0.0]);
        steve.kind = "minecraft:player".into();
        steve.name = Some("Steve".into());
        steve.face = face_for("minecraft:player", Some("entity/player/slim/alex"));
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
        assert_eq!(blits.len(), 2, "the zombie's face and the player's");
        assert!(
            matches!(&blits[0], DrawOp::Blit { texture, uv: [8, 8, 8, 8], .. } if texture == "entity/zombie/zombie")
        );
        assert!(
            matches!(&blits[1], DrawOp::Blit { texture, uv: [8, 8, 8, 8], .. } if texture == "entity/player/slim/alex")
        );
        // A kind without a known face (or whose texture is missing) stays a dot.
        zombie.face = None;
        let blits = ops(&s, &[zombie])
            .into_iter()
            .filter(|o| matches!(o, DrawOp::Blit { .. }))
            .count();
        assert_eq!(blits, 0);
        assert!(face("minecraft:ender_dragon").is_none());
        assert_eq!(
            face_for("minecraft:player", None).unwrap().texture,
            "entity/player/wide/steve"
        );
    }
}

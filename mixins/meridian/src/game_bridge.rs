// Game-side sampling for the pinned fork; shared data belongs to the mod.
use meridian_horizon::bridge::*;
use std::collections::HashMap;
use std::time::Instant;
/// The frame injection's entry.
pub(super) fn frame(game: &mut super::Game<'_>) {
    super::meridian_terrain_sampler::install(game);
    let _ = meridian_horizon::terrain_sampler::SAMPLERS.set((
        super::meridian_terrain_sampler::sample_overworld,
        super::meridian_terrain_sampler::nether_cave_mask,
        super::meridian_terrain_sampler::release_caches,
    ));
    let _ = PRIORITY.set(horizon_threads::far_terrain_current);
    let mut guard = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    let Some(s) = guard.as_mut().filter(|s| s.enabled) else {
        return;
    };
    s.held_keys = game
        .physical_keys
        .iter()
        .filter_map(|&key| horizon_client::keybinds::key_name(key))
        .map(str::to_owned)
        .collect();
    for button in &game.physical_buttons {
        use winit::event::MouseButton;
        let key = match button {
            MouseButton::Left => "key.mouse.left".to_owned(),
            MouseButton::Right => "key.mouse.right".to_owned(),
            MouseButton::Middle => "key.mouse.middle".to_owned(),
            MouseButton::Back => "key.mouse.4".to_owned(),
            MouseButton::Forward => "key.mouse.5".to_owned(),
            MouseButton::Other(n) => format!("key.mouse.{}", u32::from(*n) + 1),
        };
        s.held_keys.push(key);
    }
    for text in std::mem::take(&mut s.local_chat) {
        if let Ok(text) = horizon_client::api_text::to_engine(&text) {
            game.chat_line(text);
        }
    }
    report_membership(game, s);
    let (written, overflowed) = meridian_horizon::bridge::changes::take();
    if overflowed {
        s.membership = 0;
        report_membership(game, s);
    }
    for (cx, cz) in written {
        let c = ChunkChange::Changed([cx, cz]);
        if s.changes.last() != Some(&c) {
            s.changes.push(c);
        }
    }
    answer_requests(game, s);
    s.window = game.native_physical_window_size();
    let (w, h) = game.gui_size();
    s.gui_size = [w, h];
    if let Some((text, animate)) = s.action_bar.take() {
        if let Ok(text) = horizon_client::api_text::to_engine(&text) {
            game.titles.set_overlay(text, animate);
        }
    }
    s.debug_shown = game.debug;
    if let Some(text) = s.clipboard.take() {
        use horizon_client::clipboard::Clipboard;
        game.clipboard.set(&text);
    }
    s.world_spawn = game
        .spawn
        .map(|p| [p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32]);
    if s.want_entities {
        s.entities = snapshot_entities(game).into();
    }
    if let Some(r) = s.roof_radius {
        s.under_roof = under_roof_check(game, r);
    }
    if s.resources.is_none() {
        s.resources = Some(game.options.resources.clone());
    }
    s.player_biome = game.native_player().and_then(|p| {
        let (x, y, z) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        let chunk = game.world.chunks.get(&(x >> 4, z >> 4))?;
        Some(
            game.res
                .biomes()
                .get(chunk.biome(x & 15, y, z & 15))
                .name
                .clone(),
        )
    });
}

fn report_membership(game: &super::Game<'_>, s: &mut Shared) {
    let chunks = &game.world.chunks;
    let now = chunks.membership_revision();
    if now == s.membership {
        return;
    }
    let deltas = (s.membership != 0)
        .then(|| chunks.deltas_since(s.membership))
        .flatten();
    match deltas {
        Some(deltas) => {
            use horizon_client::chunk_map::ChunkMembershipDelta as D;
            for (_, delta) in deltas {
                s.changes.push(match delta {
                    D::Added(k) => ChunkChange::Loaded([k.0, k.1]),
                    D::Removed(k) => ChunkChange::Unloaded([k.0, k.1]),
                    D::Reset => ChunkChange::Reset,
                });
            }
        }
        None => {
            s.changes.push(ChunkChange::Reset);
            for key in chunks.keys() {
                s.changes.push(ChunkChange::Loaded([key.0, key.1]));
            }
        }
    }
    s.membership = now;
}

fn answer_requests(game: &super::Game<'_>, s: &mut Shared) {
    if s.requests.is_empty() {
        return;
    }
    let started = Instant::now();
    let dimension = game
        .native_player()
        .map(|p| p.dimension.to_owned())
        .unwrap_or_default();
    let facts = game.res.facts();
    let biomes = game.res.biomes();
    let mut done = 0;
    for &(pos, cave_from) in &s.requests {
        done += 1;
        let Some(chunk) = game.world.chunks.get(&(pos[0], pos[1])) else {
            continue;
        };
        let mut snapshot = read_chunk(
            chunk,
            facts,
            biomes,
            &dimension,
            pos,
            cave_from,
            s.hide,
            &mut s.blocks,
            &mut s.biomes,
        );
        read_biome_border(game, &mut snapshot, &mut s.biomes);
        s.results.push(snapshot);
        if started.elapsed() >= READ_BUDGET {
            break;
        }
    }
    s.requests.drain(..done);
}

#[allow(clippy::too_many_arguments)]
fn read_chunk(
    chunk: &horizon_client::chunk::Chunk,
    facts: &horizon_client::blocks::BlockFacts,
    biome_table: &horizon_client::biome::Biomes,
    dimension: &str,
    pos: [i32; 2],
    cave_from: Option<i32>,
    hide: Hide,
    block_looks: &mut HashMap<String, BlockLook>,
    biome_looks: &mut HashMap<String, BiomeLook>,
) -> Snapshot {
    use horizon_client::blocks::{Fluid, MAP_COLORS};
    use horizon_world::registry::BlockStateId;

    let mut blocks: Vec<String> = Vec::new();
    let mut block_index: HashMap<u32, u16> = HashMap::new();
    let mut biomes: Vec<String> = Vec::new();
    let mut biome_index: HashMap<u16, u16> = HashMap::new();
    // Plain water, for the water over waterlogged blocks (kelp, seagrass,
    // waterlogged stairs...): the map draws their water, not the plant.
    const PLAIN_WATER: u32 = u32::MAX;
    let mut intern_block = |state: u32| -> u16 {
        *block_index.entry(state).or_insert_with(|| {
            let (name, look) = if state == PLAIN_WATER {
                let name = "minecraft:water".to_owned();
                // The vanilla map's water colour (map colour 12).
                let look = BlockLook {
                    map_color: MAP_COLORS.get(12).copied().unwrap_or(0),
                    fluid: 1,
                    air: false,
                };
                (name, look)
            } else {
                let f = facts.get(BlockStateId(state));
                let look = BlockLook {
                    map_color: MAP_COLORS.get(f.map_color as usize).copied().unwrap_or(0),
                    fluid: match f.fluid {
                        Fluid::None => 0,
                        Fluid::Water => 1,
                        Fluid::Lava => 2,
                        Fluid::Modded => 3,
                    },
                    air: facts.is_air(BlockStateId(state)),
                };
                (f.name.to_string(), look)
            };
            block_looks.entry(name.clone()).or_insert(look);
            blocks.push(name);
            (blocks.len() - 1) as u16
        })
    };
    let mut intern_biome = |id: u16| -> u16 {
        *biome_index.entry(id).or_insert_with(|| {
            let b = biome_table.get(id);
            biome_looks.entry(b.name.clone()).or_insert(BiomeLook {
                grass: b.grass,
                foliage: b.foliage,
                dry_foliage: b.dry_foliage,
                water: b.water,
            });
            biomes.push(b.name.clone());
            (biomes.len() - 1) as u16
        })
    };

    let air = facts.air.0;
    let bottom = chunk.min_y();
    // The highest section that holds anything but air.
    let surface_top = (0..chunk.sections.len())
        .rev()
        .find(|&i| chunk.sections.get(i).is_some_and(|s| !s.is_uniform(air)))
        .map_or(bottom, |i| bottom + i as i32 * 16 + 15);
    let hidden: std::cell::RefCell<HashMap<u32, bool>> = Default::default();
    let short: std::cell::RefCell<HashMap<u32, bool>> = Default::default();
    let shows = |state: u32| {
        let f = facts.get(BlockStateId(state));
        let visible = f.map_color != 0 || f.fluid != Fluid::None;
        visible
            && !(hide.any()
                && *hidden
                    .borrow_mut()
                    .entry(state)
                    .or_insert_with(|| hide.hides(&f.name.to_string())))
    };
    let is_fluid = |state: u32| facts.get(BlockStateId(state)).fluid != Fluid::None;

    let mut columns = Vec::with_capacity(256);
    for z in 0..16 {
        for x in 0..16 {
            let mut y = match cave_from {
                None => surface_top,
                Some(ceiling) => {
                    // Under a roof: skip the ceiling down to the first air.
                    let mut y = ceiling.min(surface_top);
                    while y > bottom && shows(chunk.get(x, y, z)) {
                        y -= 1;
                    }
                    y
                }
            };
            while y > bottom && !shows(chunk.get(x, y, z)) {
                y -= 1;
            }
            if y <= bottom && !shows(chunk.get(x, y, z)) {
                columns.push(Column {
                    empty: true,
                    height: bottom as i16,
                    ..Column::default()
                });
                continue;
            }
            let mut fluid = None;
            let top_state = chunk.get(x, y, z);
            if is_fluid(top_state) {
                let surface = y;
                while y > bottom && is_fluid(chunk.get(x, y, z)) {
                    y -= 1;
                }
                let depth = (surface - y).clamp(1, 255) as u8;
                let top = facts.get(BlockStateId(top_state));
                let waterlogged =
                    top.fluid == Fluid::Water && top.name.to_string() != "minecraft:water";
                fluid = Some((
                    intern_block(if waterlogged { PLAIN_WATER } else { top_state }),
                    depth,
                ));
                while y > bottom && !shows(chunk.get(x, y, z)) {
                    y -= 1;
                }
            }
            let state = chunk.get(x, y, z);
            let light_y = fluid.map_or(y, |(_, d)| y + d as i32) + 1;
            let lowered = hide.lower_short
                && fluid.is_none()
                && *short.borrow_mut().entry(state).or_insert_with(|| {
                    Hide::short(&facts.get(BlockStateId(state)).name.to_string())
                });
            columns.push(Column {
                block: intern_block(state),
                height: y as i16 - i16::from(lowered),
                fluid,
                biome: intern_biome(chunk.biome(x, y, z)),
                light: chunk.light(x, light_y, z),
                empty: false,
            });
        }
    }
    Snapshot {
        dimension: dimension.to_owned(),
        pos,
        cave_from,
        blocks,
        biomes,
        columns,
        biome_border: Vec::new(),
    }
}

/// Read the tint halo at the surface height of the nearest centre column.
/// Missing neighbours fall back to the edge until their load triggers a reread.
fn read_biome_border(
    game: &super::Game<'_>,
    snapshot: &mut Snapshot,
    looks: &mut HashMap<String, BiomeLook>,
) {
    let mut palette: HashMap<String, u16> = snapshot
        .biomes
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u16))
        .collect();
    // The neighbours' chunks, looked up once instead of per halo cell.
    let mut around = Vec::with_capacity(8);
    snapshot.biome_border.reserve(20 * 20);
    for z in -2i32..18 {
        for x in -2i32..18 {
            let col = snapshot.columns[z.clamp(0, 15) as usize * 16 + x.clamp(0, 15) as usize];
            // Inside the chunk the column already carries its biome.
            if (0..16).contains(&x) && (0..16).contains(&z) {
                snapshot.biome_border.push(col.biome);
                continue;
            }
            let wx = snapshot.pos[0] * 16 + x;
            let wz = snapshot.pos[1] * 16 + z;
            let key = (wx >> 4, wz >> 4);
            let chunk = match around.iter().find(|(k, _)| *k == key) {
                Some(&(_, chunk)) => chunk,
                None => {
                    let chunk = game.world.chunks.get(&key);
                    around.push((key, chunk));
                    chunk
                }
            };
            let id = if let Some(chunk) = chunk {
                let b = game
                    .res
                    .biomes()
                    .get(chunk.biome(wx & 15, i32::from(col.height), wz & 15));
                if !looks.contains_key(&b.name) {
                    looks.insert(
                        b.name.clone(),
                        BiomeLook {
                            grass: b.grass,
                            foliage: b.foliage,
                            dry_foliage: b.dry_foliage,
                            water: b.water,
                        },
                    );
                }
                match palette.get(b.name.as_str()) {
                    Some(&i) => i,
                    None => {
                        snapshot.biomes.push(b.name.clone());
                        let i = (snapshot.biomes.len() - 1) as u16;
                        palette.insert(b.name.clone(), i);
                        i
                    }
                }
            } else {
                col.biome
            };
            snapshot.biome_border.push(id);
        }
    }
}

fn snapshot_entities(game: &super::Game<'_>) -> Vec<EntitySnap> {
    let t = game.clock.fraction();
    let lerp = |a: f64, b: f64| a + (b - a) * t;
    let mut out = Vec::new();
    for m in game.mobs.iter() {
        if m.removed || !m.alive() {
            continue;
        }
        let uuid = game.remote_player_uuid(m.id);
        let player = uuid.is_some();
        // Armor stands are no living things to the radar: "other" entities.
        let stand = m.kind.id.ends_with("armor_stand");
        let skin = uuid.map(|uuid| match &m.skin {
            Some(skin) if skin.starts_with("entity/") => skin.clone(),
            _ => horizon_client::tab_list::default_skin(uuid).to_owned(),
        });
        let class = if player {
            EntityClass::Player
        } else if stand {
            EntityClass::Other
        } else if m.hostile() {
            EntityClass::Hostile
        } else {
            EntityClass::Friendly
        };
        let name = if player {
            game.remote_player_name(m.id).map(str::to_owned)
        } else {
            m.custom_name.clone()
        };
        out.push(EntitySnap {
            id: m.id,
            kind: m.kind.id.clone(),
            class,
            pos: [
                lerp(m.previous.x, m.position.x),
                lerp(m.previous.y, m.position.y),
                lerp(m.previous.z, m.position.z),
            ],
            tick_pos: [m.position.x, m.position.y, m.position.z],
            invisible: m.invisible || m.has_effect("minecraft:invisibility"),
            name,
            skin,
            tamed: m.extra.tame,
        });
    }
    for item in game.dropped.iter() {
        out.push(EntitySnap {
            id: item.id,
            kind: "minecraft:item".to_owned(),
            class: EntityClass::Item,
            pos: [
                lerp(item.previous.x, item.position.x),
                lerp(item.previous.y, item.position.y),
                lerp(item.previous.z, item.position.z),
            ],
            tick_pos: [item.position.x, item.position.y, item.position.z],
            invisible: false,
            name: None,
            skin: None,
            tamed: false,
        });
    }
    // Vehicles: the radar's "other" entities.
    let vehicles = game
        .boats
        .iter()
        .map(|b| (b.id, "minecraft:boat", b.previous, b.position))
        .chain(
            game.carts
                .iter()
                .map(|c| (c.id, "minecraft:minecart", c.previous, c.position)),
        );
    for (id, kind, previous, position) in vehicles {
        out.push(EntitySnap {
            id,
            kind: kind.to_owned(),
            class: EntityClass::Other,
            pos: [
                lerp(previous.x, position.x),
                lerp(previous.y, position.y),
                lerp(previous.z, position.z),
            ],
            tick_pos: [position.x, position.y, position.z],
            invisible: false,
            name: None,
            skin: None,
            tamed: false,
        });
    }
    out
}

/// Every column within `r` of the player's has a block that blocks light
/// above the player's eyes (`can_occlude`), up to the top of its chunk.
fn under_roof_check(game: &super::Game<'_>, r: u8) -> bool {
    use horizon_world::registry::BlockStateId;
    let Some(p) = game.native_player() else {
        return false;
    };
    let (px, pz) = (p.x.floor() as i32, p.z.floor() as i32);
    let eye = (p.y + 1.62).floor() as i32;
    let facts = game.res.facts();
    let r = r as i32;
    for dx in -r..=r {
        for dz in -r..=r {
            let (x, z) = (px + dx, pz + dz);
            let Some(chunk) = game.world.chunks.get(&(x >> 4, z >> 4)) else {
                return false;
            };
            let top = chunk.max_y();
            let covered = (eye + 1..top).any(|y| {
                facts
                    .get(BlockStateId(chunk.get(x & 15, y, z & 15)))
                    .can_occlude
            });
            if !covered {
                return false;
            }
        }
    }
    true
}

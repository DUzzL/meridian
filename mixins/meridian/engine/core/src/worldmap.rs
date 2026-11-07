//! The full-screen world map: its view (centre and zoom), input (drag,
//! scroll, keys, right-click menu) and everything drawn over the terrain
//! (coordinates and biome, zoom, compass, player, waypoints, entities,
//! footsteps, buttons, the menu), laid out like the reference's map screen
//! (`docs/DECISIONS.md` D-023).
//!
//! The terrain itself is drawn by the host on the GPU from the regions
//! [`WorldMap::visible_regions`] names.

use crate::geom::{REGION_BLOCKS, RegionPos};
use crate::minimap::{Align, DrawOp};
use crate::radar;
use crate::waypoint::Waypoint;

/// The zoom a map opens at, as the reference shows it ("3.0x").
pub const DEFAULT_ZOOM: f64 = 3.0;
pub const MIN_ZOOM: f64 = 0.0625;
pub const MAX_ZOOM: f64 = 50.0;
/// The limits with "unlimited zoom" on (the draw radius still caps how much
/// is drawn when zoomed far out).
pub const MIN_ZOOM_UNLIMITED: f64 = 1.0 / 64.0;
pub const MAX_ZOOM_UNLIMITED: f64 = 1000.0;
/// How long the opening animation runs, seconds.
const OPENING_SECONDS: f32 = 0.25;
/// The zoom the opening animation starts from, relative to the target.
const OPENING_FROM: f64 = 0.5;
/// Physical pixels per block per zoom step, in minimap units (a unit is
/// about three physical pixels on a 1080p-class screen): 3.0x is about 1.9
/// units per block, as measured on the reference.
const UNITS_PER_BLOCK_PER_ZOOM: f64 = 0.633;
/// Zoom factor per scroll notch.
const ZOOM_STEP: f64 = 1.2;
/// Half the side of an entity's face on the map, in units.
const ENTITY_FACE_HALF: f32 = 3.0;
/// Most footstep points kept.
const FOOTSTEPS: usize = 2048;

/// What the map's right-click menu offers.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    /// The header: the clicked block's coordinates (not clickable).
    Header(String),
    CreateWaypoint([i32; 3]),
    TeleportHere([i32; 3]),
    ShareLocation([i32; 3]),
    EditWaypoint(u64),
    TeleportToWaypoint(u64),
    DeleteWaypoint(u64),
    ExportPng,
    /// An entry the host adds (Seed Atlas markers): its id and label.
    Extra(u32, String),
}

impl MenuItem {
    fn label(&self) -> String {
        match self {
            MenuItem::Header(s) => s.clone(),
            MenuItem::CreateWaypoint(_) => "Create Waypoint".into(),
            MenuItem::TeleportHere(_) => "Teleport Here".into(),
            MenuItem::ShareLocation(_) => "Share Location".into(),
            MenuItem::EditWaypoint(_) => "Edit Waypoint".into(),
            MenuItem::TeleportToWaypoint(_) => "Teleport To Waypoint".into(),
            MenuItem::DeleteWaypoint(_) => "Delete Waypoint".into(),
            MenuItem::ExportPng => "Export Map as PNG".into(),
            MenuItem::Extra(_, label) => label.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub at: [f32; 2],
    pub items: Vec<MenuItem>,
    /// GUI pixels per font pixel.
    pub scale: f32,
}

impl Menu {
    fn row(&self) -> f32 {
        11.0 * self.scale
    }

    fn rect(&self, gui: [f32; 2]) -> [f32; 4] {
        // Wide enough for the longest entry, at least the usual width.
        let longest = self
            .items
            .iter()
            .map(|i| crate::ui::text_width(&i.label()))
            .max_by(f32::total_cmp)
            .unwrap_or(0.0);
        let width = 104f32.max(longest + 6.0) * self.scale;
        let h = self.items.len() as f32 * self.row() + 2.0;
        let x = self.at[0].min(gui[0] - width - 2.0).max(0.0);
        let y = self.at[1].min(gui[1] - h - 2.0).max(0.0);
        [x, y, width, h]
    }

    fn item_at(&self, gui: [f32; 2], p: [f32; 2]) -> Option<&MenuItem> {
        let [x, y, w, h] = self.rect(gui);
        if p[0] < x || p[0] >= x + w || p[1] < y + 1.0 || p[1] >= y + h - 1.0 {
            return None;
        }
        self.items.get(((p[1] - y - 1.0) / self.row()) as usize)
    }
}

/// What a click on the map asks the host for.
#[derive(Clone, Debug, PartialEq)]
pub enum MapAction {
    None,
    Close,
    Menu(MenuItem),
    /// A toolbar button the host carries out (settings, layers, lists).
    Button(Button),
}

/// What the overlay draws from the game this frame.
#[derive(Debug)]
pub struct Overlay<'a> {
    pub gui: [f32; 2],
    /// GUI pixels per minimap unit (`minimap::unit`).
    pub unit: f32,
    pub mouse: [f32; 2],
    pub player: [f64; 3],
    pub yaw: f32,
    /// The surface height under the cursor, if explored.
    pub hovered_height: Option<i16>,
    pub hovered_biome: Option<String>,
    pub waypoints: &'a [&'a Waypoint],
    pub entities: &'a [radar::Entity],
    /// Entities as their faces (where known) instead of dots.
    pub faces: bool,
    /// The Seed Atlas layer is shown (the toolbar's layer button).
    pub seed_layer: bool,
    pub show_coordinates: bool,
    pub show_zoom: bool,
    pub show_biome: bool,
    pub show_footsteps: bool,
    /// The player as a red arrow; otherwise a white dot.
    pub player_as_arrow: bool,
    pub cave_layer: Option<i32>,
    /// The dimension shown, when it isn't the player's.
    pub other_dimension: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorldMap {
    /// The block column at the screen's centre.
    pub center: [f64; 2],
    pub zoom: f64,
    pub menu: Option<Menu>,
    /// Where the left button went down, to tell a click from a drag.
    pub press: Option<[f32; 2]>,
    pub dragged: bool,
    pub footsteps: Vec<[i32; 2]>,
    /// The zoom range (see [`WorldMap::set_unlimited`]).
    pub limits: (f64, f64),
    /// The opening animation: its target zoom and seconds run so far.
    pub opening: Option<(f64, f32)>,
    /// The toolbar has the Seed Atlas buttons (Seed Atlas is on).
    pub seed_atlas: bool,
    /// The player's last position (x, z), for the centre button.
    pub player_at: Option<[f64; 2]>,
}

impl WorldMap {
    pub fn new(player: [f64; 3]) -> Self {
        Self {
            center: [player[0], player[2]],
            zoom: DEFAULT_ZOOM,
            menu: None,
            press: None,
            dragged: false,
            footsteps: Vec::new(),
            limits: (MIN_ZOOM, MAX_ZOOM),
            opening: None,
            seed_atlas: false,
            player_at: Some([player[0], player[2]]),
        }
    }

    /// Lifts the zoom limits in either direction (the world map settings).
    pub fn set_unlimited(&mut self, zoom_in: bool, zoom_out: bool) {
        self.limits = (
            if zoom_out {
                MIN_ZOOM_UNLIMITED
            } else {
                MIN_ZOOM
            },
            if zoom_in {
                MAX_ZOOM_UNLIMITED
            } else {
                MAX_ZOOM
            },
        );
        if self.opening.is_none() {
            self.zoom = self.zoom.clamp(self.limits.0, self.limits.1);
        }
    }

    /// The map opens on the player; with `animate` it zooms in from further
    /// out over a quarter second.
    pub fn open(&mut self, player: [f64; 3], animate: bool) {
        self.recenter(player);
        self.menu = None;
        self.press = None;
        if let Some((target, _)) = self.opening.take() {
            self.zoom = target;
        }
        if animate {
            self.opening = Some((self.zoom, 0.0));
            self.zoom = (self.zoom * OPENING_FROM).clamp(self.limits.0, self.limits.1);
        }
    }

    /// Advances the opening animation by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        let Some((target, t)) = self.opening else {
            return;
        };
        let t = t + dt.max(0.0);
        if t >= OPENING_SECONDS {
            self.zoom = target;
            self.opening = None;
            return;
        }
        // Ease out, in log space so the zoom feels even.
        let k = 1.0 - (1.0 - t / OPENING_SECONDS).powi(3);
        let from = (target * OPENING_FROM).ln();
        self.zoom = (from + (target.ln() - from) * k as f64).exp();
        self.opening = Some((target, t));
    }

    /// GUI pixels per block.
    pub fn scale(&self, unit: f32) -> f64 {
        self.zoom * UNITS_PER_BLOCK_PER_ZOOM * unit as f64
    }

    pub fn to_world(&self, p: [f32; 2], gui: [f32; 2], unit: f32) -> [f64; 2] {
        let s = self.scale(unit);
        [
            self.center[0] + (p[0] as f64 - gui[0] as f64 / 2.0) / s,
            self.center[1] + (p[1] as f64 - gui[1] as f64 / 2.0) / s,
        ]
    }

    pub fn to_screen(&self, w: [f64; 2], gui: [f32; 2], unit: f32) -> [f32; 2] {
        let s = self.scale(unit);
        [
            (gui[0] as f64 / 2.0 + (w[0] - self.center[0]) * s) as f32,
            (gui[1] as f64 / 2.0 + (w[1] - self.center[1]) * s) as f32,
        ]
    }

    /// Zooms by `notches` (positive in) keeping the point under `cursor`.
    pub fn scroll(&mut self, notches: f64, cursor: [f32; 2], gui: [f32; 2], unit: f32) {
        if let Some((target, _)) = self.opening.take() {
            self.zoom = target;
        }
        let before = self.to_world(cursor, gui, unit);
        self.zoom = (self.zoom * ZOOM_STEP.powf(notches)).clamp(self.limits.0, self.limits.1);
        let after = self.to_world(cursor, gui, unit);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }

    /// Moves the map with the mouse: `delta` in GUI pixels.
    pub fn drag(&mut self, delta: [f32; 2], unit: f32) {
        let s = self.scale(unit);
        self.center[0] -= delta[0] as f64 / s;
        self.center[1] -= delta[1] as f64 / s;
        if delta[0].abs() + delta[1].abs() > 0.0 {
            self.dragged = true;
        }
    }

    /// The regions the screen shows (inclusive corners).
    pub fn visible_regions(&self, gui: [f32; 2], unit: f32) -> (RegionPos, RegionPos) {
        let a = self.to_world([0.0, 0.0], gui, unit);
        let b = self.to_world(gui, gui, unit);
        (
            RegionPos::of_block(a[0].floor() as i32, a[1].floor() as i32),
            RegionPos::of_block(b[0].floor() as i32, b[1].floor() as i32),
        )
    }

    /// A region's rectangle on screen (GUI pixels).
    pub fn region_rect(&self, r: RegionPos, gui: [f32; 2], unit: f32) -> [f32; 4] {
        let (x, z) = r.min_block();
        let [sx, sy] = self.to_screen([x as f64, z as f64], gui, unit);
        let side = (REGION_BLOCKS as f64 * self.scale(unit)) as f32;
        [sx, sy, side, side]
    }

    /// Notes where the player walked (every other block).
    pub fn step(&mut self, player: [f64; 3]) {
        self.player_at = Some([player[0], player[2]]);
        let p = [player[0].floor() as i32, player[2].floor() as i32];
        let moved = self
            .footsteps
            .last()
            .is_none_or(|l| (l[0] - p[0]).abs() + (l[1] - p[1]).abs() >= 2);
        if moved {
            if self.footsteps.len() >= FOOTSTEPS {
                self.footsteps.remove(0);
            }
            self.footsteps.push(p);
        }
    }

    /// The waypoint drawn under `p`, if any.
    fn waypoint_at(
        &self,
        p: [f32; 2],
        gui: [f32; 2],
        unit: f32,
        waypoints: &[&Waypoint],
    ) -> Option<u64> {
        let r = 6.0 * unit;
        waypoints
            .iter()
            .rev()
            .find(|w| {
                let [x, y] =
                    self.to_screen([w.pos[0] as f64 + 0.5, w.pos[2] as f64 + 0.5], gui, unit);
                (p[0] - x).abs() <= r && (p[1] - (y - 7.0 * unit)).abs() <= 7.0 * unit
            })
            .map(|w| w.id)
    }

    /// A right click: opens the menu for the block (and waypoint) there.
    pub fn right_click(
        &mut self,
        p: [f32; 2],
        gui: [f32; 2],
        unit: f32,
        height: Option<i16>,
        waypoints: &[&Waypoint],
    ) {
        let w = self.to_world(p, gui, unit);
        let (x, z) = (w[0].floor() as i32, w[1].floor() as i32);
        let y = height.map_or(64, |h| h as i32 + 1);
        let mut items = vec![
            MenuItem::Header(format!(
                "X: {x}, Y: {}, Z: {z}",
                height.map_or("~".to_owned(), |h| h.to_string())
            )),
            MenuItem::CreateWaypoint([x, y, z]),
            MenuItem::TeleportHere([x, y, z]),
            MenuItem::ShareLocation([x, y, z]),
        ];
        if let Some(id) = self.waypoint_at(p, gui, unit, waypoints) {
            items.extend([
                MenuItem::EditWaypoint(id),
                MenuItem::TeleportToWaypoint(id),
                MenuItem::DeleteWaypoint(id),
            ]);
        }
        items.push(MenuItem::ExportPng);
        self.menu = Some(Menu {
            at: p,
            items,
            scale: 1.6 * unit,
        });
    }

    /// The menu of a map element (a Seed Atlas structure): its name as the
    /// header, then the element's own entries (host ids and labels).
    pub fn element_menu(
        &mut self,
        p: [f32; 2],
        unit: f32,
        title: &str,
        entries: Vec<(u32, String)>,
    ) {
        let mut items = vec![MenuItem::Header(title.to_owned())];
        items.extend(
            entries
                .into_iter()
                .map(|(id, label)| MenuItem::Extra(id, label)),
        );
        self.menu = Some(Menu {
            at: p,
            items,
            scale: 1.6 * unit,
        });
    }

    /// Left button down at `p`.
    pub fn press(&mut self, p: [f32; 2]) {
        self.press = Some(p);
        self.dragged = false;
    }

    /// Left button up: a click (not a drag) picks a menu item or closes the
    /// menu; the side buttons zoom or close.
    pub fn release(&mut self, p: [f32; 2], gui: [f32; 2], unit: f32) -> MapAction {
        let was_click = self.press.take().is_some() && !self.dragged;
        if !was_click {
            return MapAction::None;
        }
        if let Some(menu) = self.menu.take() {
            return match menu.item_at(gui, p) {
                Some(MenuItem::Header(_)) | None => MapAction::None,
                Some(item) => MapAction::Menu(item.clone()),
            };
        }
        match button_at(p, gui, self.seed_atlas) {
            Some(Button::ZoomIn) => self.scroll(1.0, [gui[0] / 2.0, gui[1] / 2.0], gui, unit),
            Some(Button::ZoomOut) => self.scroll(-1.0, [gui[0] / 2.0, gui[1] / 2.0], gui, unit),
            Some(Button::Center) => {
                if let Some(p) = self.player_at {
                    self.center = p;
                }
            }
            Some(Button::Help) | None => {}
            Some(b) => return MapAction::Button(b),
        }
        MapAction::None
    }

    /// Recentres on the player (the centre button, Space).
    pub fn recenter(&mut self, player: [f64; 3]) {
        self.center = [player[0], player[2]];
    }

    /// Everything drawn over the terrain, in order.
    pub fn overlay(&self, o: &Overlay<'_>) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        let (gui, u) = (o.gui, o.unit);
        if o.show_footsteps {
            for f in self.footsteps.iter().step_by(2) {
                let [x, y] = self.to_screen([f[0] as f64 + 0.5, f[1] as f64 + 0.5], gui, u);
                ops.push(DrawOp::Fill {
                    rect: [x - 0.5 * u, y - 0.5 * u, u, u],
                    argb: 0xA0FF_FFFF,
                });
            }
        }
        // Entities: small dots with a dark rim, or their faces. Both keep
        // their size in GUI units whatever the zoom.
        for e in o.entities {
            let [x, y] = self.to_screen([e.pos[0], e.pos[2]], gui, u);
            if o.faces
                && let Some(face) = &e.face
            {
                let (half, rim) = (ENTITY_FACE_HALF * u, 0.6 * u);
                ops.push(DrawOp::Fill {
                    rect: [
                        x - half - rim,
                        y - half - rim,
                        2.0 * (half + rim),
                        2.0 * (half + rim),
                    ],
                    argb: 0xFF00_0000,
                });
                ops.push(DrawOp::Blit {
                    texture: face.texture.clone(),
                    rect: [x - half, y - half, 2.0 * half, 2.0 * half],
                    uv: face.uv,
                    base: face.base,
                    argb: 0xFFFF_FFFF,
                });
                continue;
            }
            let r = 1.2 * u;
            let diamond = |r: f32| [[x, y - r], [x + r, y], [x, y + r], [x - r, y]];
            ops.push(DrawOp::Quad {
                points: diamond(r + 0.7 * u),
                argb: 0xFF00_0000,
            });
            ops.push(DrawOp::Quad {
                points: diamond(r),
                argb: 0xFF00_0000 | e.class.rgb(),
            });
        }
        // Waypoints: a shield in the waypoint's colour with its initial, its
        // point on the block.
        for w in o.waypoints {
            let [x, y] = self.to_screen([w.pos[0] as f64 + 0.5, w.pos[2] as f64 + 0.5], gui, u);
            ops.extend(pin([x, y], u, 0xFF00_0000 | w.rgb(), &w.initials));
        }
        // The player: a red arrow along their facing, or a white dot.
        let [px, py] = self.to_screen([o.player[0], o.player[2]], gui, u);
        if o.player_as_arrow {
            ops.extend(arrow([px, py], o.yaw, 6.5 * u));
        } else {
            let diamond = |r: f32| [[px, py - r], [px + r, py], [px, py + r], [px - r, py]];
            ops.push(DrawOp::Quad {
                points: diamond(3.6 * u),
                argb: 0xFF00_0000,
            });
            ops.push(DrawOp::Quad {
                points: diamond(2.6 * u),
                argb: 0xFFFF_FFFF,
            });
        }
        // Top: the hovered block and its biome, on dark plates.
        let hovered = self.to_world(o.mouse, gui, u);
        let mut top = Vec::new();
        if o.show_coordinates {
            let y = o.hovered_height.map_or("~".to_owned(), |h| h.to_string());
            top.push(format!(
                "X: {} Y: {y} Z: {}",
                hovered[0].floor(),
                hovered[1].floor()
            ));
        }
        if o.show_biome
            && let Some(b) = &o.hovered_biome
        {
            top.push(crate::info::title_case(b));
        }
        let ts = TEXT_SCALE * u;
        for (i, line) in top.iter().enumerate() {
            let y = 2.0 * u + i as f32 * 9.0 * ts;
            ops.push(plate(gui[0] / 2.0, y, line, ts));
            ops.push(DrawOp::Text {
                x: gui[0] / 2.0,
                y,
                text: line.clone(),
                argb: 0xFFFF_FFFF,
                align: Align::Center,
                scale: ts,
            });
        }
        // Bottom: the zoom.
        if o.show_zoom {
            let text = zoom_label(self.zoom);
            let y = gui[1] - 8.0 * ts - 4.0 * u;
            ops.push(plate(gui[0] / 2.0, y, &text, ts));
            ops.push(DrawOp::Text {
                x: gui[0] / 2.0,
                y,
                text,
                argb: 0xFFFF_FFFF,
                align: Align::Center,
                scale: ts,
            });
        }
        if let Some(dim) = &o.other_dimension {
            let text = crate::info::title_case(dim);
            let at = 2.0 * u + 2.0 * 9.0 * ts;
            ops.push(plate(gui[0] / 2.0, at, &text, ts));
            ops.push(DrawOp::Text {
                x: gui[0] / 2.0,
                y: at,
                text,
                argb: 0xFFFF_FF55,
                align: Align::Center,
                scale: ts,
            });
        }
        if let Some(y) = o.cave_layer {
            let text = format!("Cave layer: Y {y}");
            let at = gui[1] - 8.0 * ts - 4.0 * u;
            ops.push(DrawOp::Text {
                x: 4.0 * u,
                y: at,
                text,
                argb: 0xFFFF_FFFF,
                align: Align::Left,
                scale: u * 1.5,
            });
        }
        ops.extend(compass(gui, u));
        // The toolbar on the right edge, its tooltips to the left.
        let buttons = toolbar(self.seed_atlas);
        let hovered = if self.menu.is_none() {
            button_at(o.mouse, gui, self.seed_atlas)
        } else {
            None
        };
        for (i, b) in buttons.iter().enumerate() {
            let rect = toolbar_rect(i, buttons.len(), gui);
            let hover = hovered == Some(*b);
            ops.push(DrawOp::NineSlice {
                sprite: if hover {
                    "gui/sprites/widget/button_highlighted"
                } else {
                    "gui/sprites/widget/button"
                },
                rect,
                argb: 0xFFFF_FFFF,
            });
            let (cx, cy) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
            match b.icon() {
                Icon::Text(t) => ops.push(DrawOp::Text {
                    x: cx + 0.5,
                    y: cy - 4.0,
                    text: t.into(),
                    argb: if hover { 0xFFFF_FFA0 } else { 0xFFFF_FFFF },
                    align: Align::Center,
                    scale: 1.0,
                }),
                Icon::Item(texture) => {
                    let off = *b == Button::SeedLayer && !o.seed_layer;
                    ops.push(DrawOp::Blit {
                        texture: texture.into(),
                        rect: [cx - 8.0, cy - 8.0, 16.0, 16.0],
                        uv: [0, 0, 16, 16],
                        base: 16,
                        argb: if off { 0xFF60_6060 } else { 0xFFFF_FFFF },
                    });
                }
            }
        }
        if let Some(b) = hovered {
            let i = buttons.iter().position(|x| *x == b).unwrap_or(0);
            let rect = toolbar_rect(i, buttons.len(), gui);
            let lines: Vec<String> = if b == Button::Help {
                CONTROLS.iter().map(|l| (*l).to_owned()).collect()
            } else {
                vec![b.tooltip(o.seed_layer).to_owned()]
            };
            ops.extend(side_tooltip(&lines, rect, gui, b == Button::Help));
        }
        // The right-click menu last, over everything.
        if let Some(menu) = &self.menu {
            let [x, y, w, h] = menu.rect(gui);
            ops.push(DrawOp::Fill {
                rect: [x, y, w, h],
                argb: 0xF010_1010,
            });
            ops.push(DrawOp::Fill {
                rect: [x, y, w, 1.0],
                argb: 0xFF80_8080,
            });
            for (i, item) in menu.items.iter().enumerate() {
                let row_y = y + 1.0 + i as f32 * menu.row();
                let header = matches!(item, MenuItem::Header(_));
                let hover = !header && menu.item_at(gui, o.mouse) == Some(item);
                if hover {
                    ops.push(DrawOp::Fill {
                        rect: [x, row_y, w, menu.row()],
                        argb: 0x40FF_FFFF,
                    });
                }
                ops.push(DrawOp::Text {
                    x: x + 3.0 * menu.scale,
                    y: row_y + 1.5 * menu.scale,
                    text: item.label(),
                    argb: if header { 0xFFA0_A0A0 } else { 0xFFFF_FFFF },
                    align: Align::Left,
                    scale: menu.scale,
                });
            }
        }
        ops
    }
}

/// Font pixels per minimap unit of the map screen's texts (measured on the
/// reference: about seven physical pixels per font pixel at three per unit).
const TEXT_SCALE: f32 = 2.3;

fn plate(x: f32, y: f32, text: &str, scale: f32) -> DrawOp {
    DrawOp::Plate {
        x,
        y,
        text: text.to_owned(),
        align: Align::Center,
        scale,
        pad: scale * 0.5,
        argb: 0x9000_0000,
    }
}

/// A waypoint's shield: a square in its colour, a point below it on
/// `tip`, a dark rim and the initial in white.
pub fn pin(tip: [f32; 2], u: f32, argb: u32, initials: &str) -> Vec<DrawOp> {
    let [x, y] = tip;
    let (w, body) = (10.0 * u, 9.0 * u);
    let top = y - body - 3.5 * u;
    let shield = |g: f32| {
        [
            DrawOp::Fill {
                rect: [x - w / 2.0 - g, top - g, w + 2.0 * g, body + g],
                argb: 0,
            },
            DrawOp::Quad {
                points: [
                    [x - w / 2.0 - g, top + body - 0.01],
                    [x + w / 2.0 + g, top + body - 0.01],
                    [x, y + g],
                    [x, y + g],
                ],
                argb: 0,
            },
        ]
    };
    let mut ops = Vec::new();
    for (g, color) in [(0.8 * u, 0xC000_0000), (0.0, argb)] {
        for mut op in shield(g) {
            match &mut op {
                DrawOp::Fill { argb, .. } | DrawOp::Quad { argb, .. } => *argb = color,
                _ => {}
            }
            ops.push(op);
        }
    }
    let ts = 1.6 * u;
    ops.push(DrawOp::Text {
        x: x + 0.5 * u,
        y: top + (body - 7.0 * ts) / 2.0 + 0.5 * u,
        text: initials.chars().take(1).collect(),
        argb: 0xFFFF_FFFF,
        align: Align::Center,
        scale: ts,
    });
    ops
}

/// The player's red arrow (tip along `yaw`), with a dark rim.
pub fn arrow(at: [f32; 2], yaw: f32, len: f32) -> Vec<DrawOp> {
    let [px, py] = at;
    let (sn, cs) = yaw.to_radians().sin_cos();
    let (dx, dy) = (-sn, cs);
    let (nx, ny) = (-dy, dx);
    let points = |g: f32| {
        let l = len * g;
        [
            [px + dx * l, py + dy * l],
            [px - dx * l - nx * l * 0.8, py - dy * l - ny * l * 0.8],
            [px - dx * l * 0.35, py - dy * l * 0.35],
            [px - dx * l + nx * l * 0.8, py - dy * l + ny * l * 0.8],
        ]
    };
    vec![
        DrawOp::Quad {
            points: points(1.3),
            argb: 0xFF00_0000,
        },
        DrawOp::Quad {
            points: points(1.0),
            argb: 0xFFD8_1E1E,
        },
    ]
}

/// The compass at the top right: a thin cross of diagonals with N, E, S, W
/// on the axes.
fn compass(gui: [f32; 2], u: f32) -> Vec<DrawOp> {
    let c = [gui[0] - 41.0 * u, 42.0 * u];
    let arm = 19.0 * u;
    let mut ops = Vec::new();
    for (a, b) in [
        ([-1.0f32, -1.0f32], [1.0f32, 1.0f32]),
        ([-1.0, 1.0], [1.0, -1.0]),
    ] {
        let p0 = [c[0] + a[0] * arm, c[1] + a[1] * arm];
        let p1 = [c[0] + b[0] * arm, c[1] + b[1] * arm];
        let len = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
        for (half, color) in [(0.9 * u, 0x8000_0000u32), (0.35 * u, 0xFFFF_FFFF)] {
            let n = [-(p1[1] - p0[1]) / len * half, (p1[0] - p0[0]) / len * half];
            ops.push(DrawOp::Quad {
                points: [
                    [p0[0] + n[0], p0[1] + n[1]],
                    [p1[0] + n[0], p1[1] + n[1]],
                    [p1[0] - n[0], p1[1] - n[1]],
                    [p0[0] - n[0], p0[1] - n[1]],
                ],
                argb: color,
            });
        }
    }
    let ts = 2.2 * u;
    for (label, off) in [
        ("N", [0.0, -1.0]),
        ("E", [1.0, 0.0]),
        ("S", [0.0, 1.0]),
        ("W", [-1.0, 0.0]),
    ] {
        ops.push(DrawOp::Text {
            x: c[0] + off[0] * 19.0 * u,
            y: c[1] + off[1] * 19.0 * u - 3.5 * ts,
            text: label.into(),
            argb: 0xFFFF_FFFF,
            align: Align::Center,
            scale: ts,
        });
    }
    ops
}

/// The zoom as the map shows it: "3.0x", and enough digits below 0.1x
/// (unlimited zoom out) not to read "0.0x".
pub fn zoom_label(zoom: f64) -> String {
    if zoom >= 0.095 {
        format!("{zoom:.1}x")
    } else {
        let digits = (-zoom.log10()).ceil() as usize + 1;
        format!("{zoom:.digits$}x")
    }
}

/// The controls the help button lists.
pub const CONTROLS: &[&str] = &[
    "Controls",
    "Drag: move the map",
    "Mouse wheel: zoom",
    "Arrow keys / WASD: move",
    "Space: centre on you",
    "Right click: menu (waypoints, teleport, share, export)",
    "B: new waypoint at the cursor",
    "M or Esc: close the map",
    "Switch map dimension key: Overworld, Nether, End",
];

/// The toolbar's buttons, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    ZoomIn,
    ZoomOut,
    Center,
    SeedLayer,
    SeedSettings,
    Waypoints,
    Settings,
    Help,
}

/// What a toolbar button shows.
enum Icon {
    Text(&'static str),
    /// An item texture (16x16) of the game.
    Item(&'static str),
}

impl Button {
    fn icon(self) -> Icon {
        match self {
            Button::ZoomIn => Icon::Text("+"),
            Button::ZoomOut => Icon::Text("-"),
            Button::Center => Icon::Item("item/compass_00"),
            Button::SeedLayer => Icon::Item("item/filled_map"),
            Button::SeedSettings => Icon::Item("item/ender_eye"),
            Button::Waypoints => Icon::Item("item/name_tag"),
            Button::Settings => Icon::Item("item/writable_book"),
            Button::Help => Icon::Text("?"),
        }
    }

    fn tooltip(self, seed_layer: bool) -> &'static str {
        match self {
            Button::ZoomIn => "Zoom in",
            Button::ZoomOut => "Zoom out",
            Button::Center => "Centre on the player (Space)",
            Button::SeedLayer if seed_layer => "Seed map layer: On",
            Button::SeedLayer => "Seed map layer: Off",
            Button::SeedSettings => "Seed Atlas settings",
            Button::Waypoints => "Waypoints",
            Button::Settings => "Map settings",
            Button::Help => "Controls",
        }
    }
}

/// The toolbar, top to bottom (the Seed Atlas buttons while it is on).
pub fn toolbar(seed_atlas: bool) -> Vec<Button> {
    let mut b = vec![Button::ZoomIn, Button::ZoomOut, Button::Center];
    if seed_atlas {
        b.extend([Button::SeedLayer, Button::SeedSettings]);
    }
    b.extend([Button::Waypoints, Button::Settings, Button::Help]);
    b
}

/// Side of a toolbar button and the gap between two, GUI pixels.
const TOOL: f32 = 20.0;
const TOOL_GAP: f32 = 2.0;

/// Button `i` of `n`: a column at the right edge, centred vertically.
fn toolbar_rect(i: usize, n: usize, gui: [f32; 2]) -> [f32; 4] {
    let height = n as f32 * (TOOL + TOOL_GAP) - TOOL_GAP;
    let top = ((gui[1] - height) / 2.0).max(TOOL + 4.0).floor();
    [
        gui[0] - TOOL - 4.0,
        top + i as f32 * (TOOL + TOOL_GAP),
        TOOL,
        TOOL,
    ]
}

/// The toolbar button under `p`.
pub fn button_at(p: [f32; 2], gui: [f32; 2], seed_atlas: bool) -> Option<Button> {
    let buttons = toolbar(seed_atlas);
    buttons.iter().enumerate().find_map(|(i, b)| {
        let [x, y, w, h] = toolbar_rect(i, buttons.len(), gui);
        (p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h).then_some(*b)
    })
}

/// A tooltip left of a toolbar button, kept on screen.
fn side_tooltip(
    lines: &[String],
    button: [f32; 4],
    gui: [f32; 2],
    first_is_title: bool,
) -> Vec<DrawOp> {
    let width = lines
        .iter()
        .map(|l| crate::ui::text_width(l))
        .fold(0.0, f32::max);
    let h = lines.len() as f32 * 10.0 + 6.0;
    let x = (button[0] - width - 10.0).max(2.0);
    let y = (button[1] + (button[3] - h) / 2.0).clamp(2.0, (gui[1] - h - 2.0).max(2.0));
    let mut ops = vec![DrawOp::Fill {
        rect: [x - 4.0, y, width + 8.0, h],
        argb: 0xF010_0010,
    }];
    for (i, line) in lines.iter().enumerate() {
        ops.push(DrawOp::Text {
            x,
            y: y + 3.0 + i as f32 * 10.0,
            text: line.clone(),
            argb: if first_is_title && i == 0 {
                0xFFFF_FF55
            } else {
                0xFFFF_FFFF
            },
            align: Align::Left,
            scale: 1.0,
        });
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUI: [f32; 2] = [1000.0, 600.0];

    #[test]
    fn view_round_trip_and_zoom_about_cursor() {
        let mut m = WorldMap::new([100.0, 64.0, -50.0]);
        let p = [700.0, 100.0];
        let w = m.to_world(p, GUI, 1.0);
        assert!((m.to_screen(w, GUI, 1.0)[0] - p[0]).abs() < 1e-3);
        assert_eq!(
            m.to_world([500.0, 300.0], GUI, 1.0),
            [100.0, -50.0],
            "the player is centred"
        );
        m.scroll(3.0, p, GUI, 1.0);
        let after = m.to_world(p, GUI, 1.0);
        assert!(
            (after[0] - w[0]).abs() < 1e-6 && (after[1] - w[1]).abs() < 1e-6,
            "the point under the cursor stays"
        );
        for _ in 0..100 {
            m.scroll(5.0, p, GUI, 1.0);
        }
        assert_eq!(m.zoom, MAX_ZOOM);
    }

    #[test]
    fn drag_moves_the_map_with_the_mouse() {
        let mut m = WorldMap::new([0.0; 3]);
        let before = m.to_world([500.0, 300.0], GUI, 1.0);
        m.drag([19.0, 0.0], 1.0);
        let after = m.to_world([519.0, 300.0], GUI, 1.0);
        assert!((after[0] - before[0]).abs() < 1e-9);
        assert!(m.dragged);
    }

    #[test]
    fn visible_regions_cover_the_screen() {
        let mut m = WorldMap::new([0.0; 3]);
        m.zoom = 0.1;
        let (a, b) = m.visible_regions(GUI, 1.0);
        assert!(a.x < 0 && b.x >= 0);
        let r = m.region_rect(RegionPos::new(0, 0), GUI, 1.0);
        assert_eq!(r[0], 500.0);
        assert!((r[2] - 512.0 * 0.1 * 0.633) < 1e-3);
    }

    #[test]
    fn right_click_menu_offers_waypoint_actions() {
        let mut m = WorldMap::new([0.0; 3]);
        let mut wp = Waypoint::new("Home", [10, 70, 0], 3);
        wp.id = 42;
        let list = [&wp];
        let at = m.to_screen([10.5, 0.5], GUI, 1.0);
        m.right_click([at[0], at[1] - 6.0], GUI, 1.0, Some(70), &list);
        let items = &m.menu.as_ref().unwrap().items;
        assert!(items.contains(&MenuItem::EditWaypoint(42)));
        // Clicking an item: header row first, the next row is "Create Waypoint".
        let menu = m.menu.clone().unwrap();
        let (rect, row) = (menu.rect(GUI), menu.row());
        m.press([rect[0] + 5.0, rect[1] + 1.0 + row * 1.5]);
        let action = m.release([rect[0] + 5.0, rect[1] + 1.0 + row * 1.5], GUI, 1.0);
        assert!(matches!(
            action,
            MapAction::Menu(MenuItem::CreateWaypoint([10, 71, _]))
        ));
        assert!(m.menu.is_none());
    }

    #[test]
    fn menus_fit_their_longest_entry() {
        let mut m = WorldMap::new([0.0; 3]);
        m.element_menu(
            [10.0, 10.0],
            1.0,
            "Ancient City",
            vec![(1, "Create temporary waypoint".into())],
        );
        let menu = m.menu.clone().unwrap();
        assert_eq!(menu.items[0], MenuItem::Header("Ancient City".into()));
        let [_, _, w, _] = menu.rect(GUI);
        assert!(
            w >= (crate::ui::text_width("Create temporary waypoint") + 6.0) * menu.scale,
            "the entry fits ({w})"
        );
        assert!(w > 104.0 * menu.scale);
        m.element_menu([10.0, 10.0], 1.0, "Igloo", vec![(1, "Copy".into())]);
        assert_eq!(
            m.menu.unwrap().rect(GUI)[2],
            104.0 * 1.6,
            "short entries keep the usual width"
        );
    }

    #[test]
    fn a_drag_is_not_a_click() {
        let mut m = WorldMap::new([0.0; 3]);
        m.menu = Some(Menu {
            at: [0.0, 0.0],
            items: vec![MenuItem::TeleportHere([0, 0, 0])],
            scale: 1.0,
        });
        m.press([5.0, 5.0]);
        m.drag([10.0, 0.0], 1.0);
        assert_eq!(m.release([15.0, 5.0], GUI, 1.0), MapAction::None);
        assert!(m.menu.is_some(), "the menu stays after a drag");
    }

    #[test]
    fn zoom_buttons() {
        let mut m = WorldMap::new([0.0; 3]);
        let at = |b: Button, sa: bool| {
            let all = toolbar(sa);
            let i = all.iter().position(|x| *x == b).unwrap();
            let [x, y, w, h] = toolbar_rect(i, all.len(), GUI);
            [x + w / 2.0, y + h / 2.0]
        };
        let p = at(Button::ZoomIn, false);
        m.press(p);
        m.release(p, GUI, 1.0);
        assert!(m.zoom > DEFAULT_ZOOM);
        // The centre button goes back to the player.
        m.center = [500.0, 500.0];
        m.step([10.0, 64.0, 20.0]);
        let p = at(Button::Center, false);
        m.press(p);
        assert_eq!(m.release(p, GUI, 1.0), MapAction::None);
        assert_eq!(m.center, [10.0, 20.0]);
        // Seed Atlas buttons only while it is on; the host carries them out.
        assert_eq!(
            button_at(at(Button::Settings, false), GUI, false),
            Some(Button::Settings)
        );
        assert!(!toolbar(false).contains(&Button::SeedLayer));
        m.seed_atlas = true;
        let p = at(Button::SeedLayer, true);
        m.press(p);
        assert_eq!(m.release(p, GUI, 1.0), MapAction::Button(Button::SeedLayer));
        // The column stays on screen and clear of the top line.
        let all = toolbar(true);
        for i in 0..all.len() {
            let r = toolbar_rect(i, all.len(), GUI);
            assert!(r[0] + r[2] <= GUI[0] && r[1] >= 20.0 && r[1] + r[3] <= GUI[1]);
        }
    }

    #[test]
    fn zoom_labels() {
        assert_eq!(zoom_label(3.0), "3.0x");
        assert_eq!(zoom_label(0.1), "0.1x");
        assert_eq!(zoom_label(0.0625), "0.062x");
        assert_eq!(zoom_label(1.0 / 64.0), "0.016x");
    }

    #[test]
    fn unlimited_zoom_lifts_the_limits() {
        let mut m = WorldMap::new([0.0; 3]);
        for _ in 0..100 {
            m.scroll(-5.0, [500.0, 300.0], GUI, 1.0);
        }
        assert_eq!(m.zoom, MIN_ZOOM);
        m.set_unlimited(true, true);
        for _ in 0..100 {
            m.scroll(-5.0, [500.0, 300.0], GUI, 1.0);
        }
        assert_eq!(m.zoom, MIN_ZOOM_UNLIMITED);
        for _ in 0..200 {
            m.scroll(5.0, [500.0, 300.0], GUI, 1.0);
        }
        assert_eq!(m.zoom, MAX_ZOOM_UNLIMITED);
        m.set_unlimited(false, false);
        assert_eq!(m.zoom, MAX_ZOOM, "back within the limits");
    }

    #[test]
    fn opening_animation_zooms_in_to_the_target() {
        let mut m = WorldMap::new([0.0; 3]);
        m.open([10.0, 64.0, 20.0], true);
        assert!(m.zoom < DEFAULT_ZOOM);
        assert_eq!(m.center, [10.0, 20.0]);
        let mut last = m.zoom;
        for _ in 0..10 {
            m.tick(0.02);
            assert!(m.zoom >= last);
            last = m.zoom;
        }
        m.tick(1.0);
        assert_eq!(m.zoom, DEFAULT_ZOOM);
        assert!(m.opening.is_none());
        // Scrolling during the animation starts from the target.
        m.open([0.0; 3], true);
        m.scroll(1.0, [500.0, 300.0], GUI, 1.0);
        assert!((m.zoom - DEFAULT_ZOOM * ZOOM_STEP).abs() < 1e-9);
        m.open([0.0; 3], false);
        assert!((m.zoom - DEFAULT_ZOOM * ZOOM_STEP).abs() < 1e-9);
    }

    #[test]
    fn footsteps_every_other_block() {
        let mut m = WorldMap::new([0.0; 3]);
        for x in 0..10 {
            m.step([x as f64, 64.0, 0.0]);
        }
        assert_eq!(m.footsteps.len(), 5);
    }
}

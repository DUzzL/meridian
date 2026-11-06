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
/// Physical pixels per block per zoom step, in minimap units (a unit is
/// about three physical pixels on a 1080p-class screen): 3.0x is about 1.9
/// units per block, as measured on the reference.
const UNITS_PER_BLOCK_PER_ZOOM: f64 = 0.633;
/// Zoom factor per scroll notch.
const ZOOM_STEP: f64 = 1.2;
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
        let width = 104.0 * self.scale;
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
    pub show_coordinates: bool,
    pub show_zoom: bool,
    pub show_biome: bool,
    pub show_footsteps: bool,
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
        }
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
        let before = self.to_world(cursor, gui, unit);
        self.zoom = (self.zoom * ZOOM_STEP.powf(notches)).clamp(MIN_ZOOM, MAX_ZOOM);
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
        match button_at(p, gui, unit) {
            Some(Button::ZoomIn) => self.scroll(1.0, [gui[0] / 2.0, gui[1] / 2.0], gui, unit),
            Some(Button::ZoomOut) => self.scroll(-1.0, [gui[0] / 2.0, gui[1] / 2.0], gui, unit),
            None => {}
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
        // Entities: small dots with a dark rim.
        for e in o.entities {
            let [x, y] = self.to_screen([e.pos[0], e.pos[2]], gui, u);
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
        // The player: a red arrow along their facing.
        let [px, py] = self.to_screen([o.player[0], o.player[2]], gui, u);
        ops.extend(arrow([px, py], o.yaw, 6.5 * u));
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
            let text = format!("{:.1}x", self.zoom);
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
        // Side buttons.
        for b in [Button::ZoomIn, Button::ZoomOut] {
            let [x, y, w, h] = button_rect(b, gui, u);
            let hover =
                o.mouse[0] >= x && o.mouse[0] < x + w && o.mouse[1] >= y && o.mouse[1] < y + h;
            let color = if hover { 0xFFFF_FFA0 } else { 0xFFFF_FFFF };
            let (cx, cy, t) = (x + w / 2.0, y + h / 2.0, 1.5 * u);
            ops.push(DrawOp::Fill {
                rect: [cx - w * 0.4, cy - t / 2.0, w * 0.8, t],
                argb: color,
            });
            if b == Button::ZoomIn {
                ops.push(DrawOp::Fill {
                    rect: [cx - t / 2.0, cy - h * 0.4, t, h * 0.8],
                    argb: color,
                });
            }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    ZoomIn,
    ZoomOut,
}

fn button_rect(b: Button, gui: [f32; 2], u: f32) -> [f32; 4] {
    let fraction = match b {
        Button::ZoomIn => 0.31,
        Button::ZoomOut => 0.39,
    };
    let s = 14.0 * u;
    [
        gui[0] - 25.0 * u - s / 2.0,
        gui[1] * fraction - s / 2.0,
        s,
        s,
    ]
}

/// The side button under `p`.
pub fn button_at(p: [f32; 2], gui: [f32; 2], u: f32) -> Option<Button> {
    [Button::ZoomIn, Button::ZoomOut].into_iter().find(|b| {
        let [x, y, w, h] = button_rect(*b, gui, u);
        p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
    })
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
        let [x, y, w, h] = button_rect(Button::ZoomIn, GUI, 1.0);
        let p = [x + w / 2.0, y + h / 2.0];
        m.press(p);
        m.release(p, GUI, 1.0);
        assert!(m.zoom > DEFAULT_ZOOM);
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

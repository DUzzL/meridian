//! The minimap's layout and drawing, as a list of plain draw operations.
//!
//! The host turns [`DrawOp`]s into its own HUD calls; nothing here knows
//! Horizon. The map is drawn on a screen-space grid of cells: each cell
//! samples the map at the world point under its centre (turned with the
//! player unless north is locked), and runs of equal cells in a row merge
//! into one rectangle. A round map skips the cells outside its circle.

use crate::config::{Corner, MinimapSettings, MinimapShape, MinimapSize};

/// GUI pixels between the map and the screen edge.
pub const MARGIN: f32 = 4.0;
/// Height of one info line under the map.
pub const LINE_HEIGHT: f32 = 10.0;
/// The most cells drawn per side; larger maps use bigger cells.
const MAX_CELLS_PER_SIDE: f32 = 160.0;

/// Blocks per GUI pixel for each zoom step; `further_zoom_out` adds two.
pub fn zoom_steps(further: bool) -> &'static [f32] {
    if further {
        &[0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0]
    } else {
        &[0.5, 1.0, 2.0, 4.0, 8.0]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// One HUD drawing step, in GUI pixels from the screen's top left.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    Fill {
        rect: [f32; 4],
        argb: u32,
    },
    Quad {
        points: [[f32; 2]; 4],
        argb: u32,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        argb: u32,
        align: Align,
    },
}

/// Where and how big the map is this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Top left corner of the map square.
    pub x: f32,
    pub y: f32,
    pub size: f32,
    /// Blocks per GUI pixel.
    pub blocks_per_pixel: f32,
    pub rotate: bool,
    pub round: bool,
    /// Info lines go below the map (true) or above it (bottom corners).
    pub info_below: bool,
}

impl Layout {
    pub fn center(&self) -> [f32; 2] {
        [self.x + self.size / 2.0, self.y + self.size / 2.0]
    }
}

/// The map's side for a size setting and screen.
pub fn side(size: MinimapSize, gui: [f32; 2]) -> f32 {
    let px = match size {
        MinimapSize::Auto => (gui[0].min(gui[1]) * 0.28).clamp(64.0, 160.0),
        MinimapSize::Small => 64.0,
        MinimapSize::Medium => 96.0,
        MinimapSize::Large => 128.0,
        MinimapSize::Huge => 192.0,
    };
    // Even, so the centre falls between pixels the same way every frame.
    (px / 2.0).floor() * 2.0
}

/// The zoom step index in use: the enlarged map zooms one step further out
/// unless it has its own step.
pub fn zoom_index(s: &MinimapSettings, enlarged: bool) -> usize {
    let steps = zoom_steps(s.further_zoom_out).len();
    let base = s.zoom as usize;
    let i = if enlarged {
        if s.enlarged_zoom > 0 {
            s.enlarged_zoom as usize - 1
        } else {
            base + 1
        }
    } else {
        base
    };
    i.min(steps - 1)
}

/// Where the map goes on a `gui`-sized screen, with `info_lines` lines of
/// text beside it.
pub fn layout(s: &MinimapSettings, gui: [f32; 2], enlarged: bool, info_lines: usize) -> Layout {
    let normal = side(s.size, gui);
    let size = if enlarged {
        let big = (gui[0].min(gui[1]) * 0.7).floor();
        ((big / 2.0).floor() * 2.0).max(normal)
    } else {
        normal
    };
    let info = info_lines as f32 * LINE_HEIGHT;
    let (x, y, info_below) = if enlarged && s.enlarged_centered {
        ((gui[0] - size) / 2.0, (gui[1] - size) / 2.0, true)
    } else {
        let left = MARGIN + s.offset[0] as f32;
        let right = gui[0] - size - MARGIN - s.offset[0] as f32;
        let top = MARGIN + s.offset[1] as f32;
        let bottom = gui[1] - size - MARGIN - info - s.offset[1] as f32;
        match s.corner {
            Corner::TopLeft => (left, top, true),
            Corner::TopRight => (right, top, true),
            Corner::BottomLeft => (left, bottom + info, false),
            Corner::BottomRight => (right, bottom + info, false),
        }
    };
    let steps = zoom_steps(s.further_zoom_out);
    Layout {
        x: x.round(),
        y: y.round(),
        size,
        blocks_per_pixel: steps[zoom_index(s, enlarged)],
        rotate: !s.north_locked,
        round: s.shape == MinimapShape::Round,
        info_below,
    }
}

/// The world's directions on screen: `right` and `forward` as world (x, z)
/// unit vectors for the screen's +x and -y.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Axes {
    pub right: [f64; 2],
    pub forward: [f64; 2],
}

impl Axes {
    /// Turned with a player facing `yaw` (Minecraft degrees: 0 south, 90
    /// west), or north up.
    pub fn new(rotate: bool, yaw: f32) -> Self {
        if !rotate {
            return Self {
                right: [1.0, 0.0],
                forward: [0.0, -1.0],
            };
        }
        let (s, c) = f64::from(yaw).to_radians().sin_cos();
        Self {
            forward: [-s, c],
            right: [-c, -s],
        }
    }

    /// World offset (blocks) of a screen offset (pixels, y down).
    pub fn to_world(&self, dx: f64, dy: f64, bpp: f64) -> [f64; 2] {
        [
            (self.right[0] * dx - self.forward[0] * dy) * bpp,
            (self.right[1] * dx - self.forward[1] * dy) * bpp,
        ]
    }

    /// Screen offset (pixels, y down) of a world offset (blocks).
    pub fn to_screen(&self, wx: f64, wz: f64, bpp: f64) -> [f64; 2] {
        let dx = (wx * self.right[0] + wz * self.right[1]) / bpp;
        let dy = -(wx * self.forward[0] + wz * self.forward[1]) / bpp;
        [dx, dy]
    }
}

/// Multiplies a colour's alpha by `opacity` (0..=100).
pub fn with_opacity(argb: u32, opacity: u8) -> u32 {
    let a = ((argb >> 24) * opacity.min(100) as u32 + 50) / 100;
    (a << 24) | (argb & 0x00FF_FFFF)
}

/// Draws the map's terrain: an undiscovered backdrop, then runs of cells.
/// `pixel(x, z)` answers the map's ARGB at a block column (0 = unexplored).
pub fn terrain(
    layout: &Layout,
    center: [f64; 2],
    yaw: f32,
    opacity: u8,
    undiscovered_opacity: u8,
    pixel: impl Fn(i32, i32) -> u32,
) -> Vec<DrawOp> {
    let mut ops = Vec::new();
    let cell = (layout.size / MAX_CELLS_PER_SIDE).ceil().max(1.0);
    let n = (layout.size / cell).ceil() as i32;
    let half = layout.size as f64 / 2.0;
    let r2 = half * half;
    let axes = Axes::new(layout.rotate, yaw);
    let bpp = layout.blocks_per_pixel as f64;
    let backdrop = with_opacity(0xFF10_1010, undiscovered_opacity);
    if !layout.round {
        ops.push(DrawOp::Fill {
            rect: [layout.x, layout.y, layout.size, layout.size],
            argb: backdrop,
        });
    }
    for row in 0..n {
        let y = layout.y + row as f32 * cell;
        let dy = (row as f64 + 0.5) * cell as f64 - half;
        // The cells of this row inside the shape.
        let (first, last) = if layout.round {
            let span2 = r2 - dy * dy;
            if span2 <= 0.0 {
                continue;
            }
            let span = span2.sqrt();
            let first = ((half - span) / cell as f64).ceil().max(0.0) as i32;
            let last = (((half + span) / cell as f64).floor() as i32 - 1).min(n - 1);
            if last < first {
                continue;
            }
            ops.push(DrawOp::Fill {
                rect: [
                    layout.x + first as f32 * cell,
                    y,
                    (last - first + 1) as f32 * cell,
                    cell,
                ],
                argb: backdrop,
            });
            (first, last)
        } else {
            (0, n - 1)
        };
        let mut run: Option<(i32, u32)> = None;
        let flush = |ops: &mut Vec<DrawOp>, run: Option<(i32, u32)>, end: i32| {
            if let Some((start, argb)) = run
                && argb >> 24 != 0
            {
                ops.push(DrawOp::Fill {
                    rect: [
                        layout.x + start as f32 * cell,
                        y,
                        (end - start) as f32 * cell,
                        cell,
                    ],
                    argb: with_opacity(argb, opacity),
                });
            }
        };
        for col in first..=last {
            let dx = (col as f64 + 0.5) * cell as f64 - half;
            let [ox, oz] = axes.to_world(dx, dy, bpp);
            let argb = pixel(
                (center[0] + ox).floor() as i32,
                (center[1] + oz).floor() as i32,
            );
            match run {
                Some((_, c)) if c == argb => {}
                _ => {
                    flush(&mut ops, run, col);
                    run = Some((col, argb));
                }
            }
        }
        flush(&mut ops, run, last + 1);
    }
    ops
}

/// Where a world point lands on the map, and whether it had to be pulled in
/// to the rim (it lies outside the map).
pub fn project(
    layout: &Layout,
    center: [f64; 2],
    yaw: f32,
    point: [f64; 2],
    inset: f32,
) -> ([f32; 2], bool) {
    let axes = Axes::new(layout.rotate, yaw);
    let [mut dx, mut dy] = axes.to_screen(
        point[0] - center[0],
        point[1] - center[1],
        layout.blocks_per_pixel as f64,
    );
    let limit = (layout.size / 2.0 - inset).max(0.0) as f64;
    let mut clamped = false;
    if layout.round {
        let d = (dx * dx + dy * dy).sqrt();
        if d > limit {
            dx *= limit / d;
            dy *= limit / d;
            clamped = true;
        }
    } else {
        let m = dx.abs().max(dy.abs());
        if m > limit {
            dx *= limit / m;
            dy *= limit / m;
            clamped = true;
        }
    }
    let [cx, cy] = layout.center();
    ([cx + dx as f32, cy + dy as f32], clamped)
}

/// The player's arrow at the map's centre: pointing up when the map turns,
/// along the player's facing when north is locked.
pub fn arrow(layout: &Layout, yaw: f32, scale: f32, argb: u32) -> DrawOp {
    let [cx, cy] = layout.center();
    // Screen direction of the player's facing.
    let (dir_x, dir_y) = if layout.rotate {
        (0.0, -1.0)
    } else {
        let (s, c) = yaw.to_radians().sin_cos();
        (-s, c)
    };
    let len = 4.0 * scale;
    let (px, py) = (-dir_y, dir_x);
    let tip = [cx + dir_x * len, cy + dir_y * len];
    let left = [
        cx - dir_x * len + px * len * 0.8,
        cy - dir_y * len + py * len * 0.8,
    ];
    let notch = [cx - dir_x * len * 0.4, cy - dir_y * len * 0.4];
    let right = [
        cx - dir_x * len - px * len * 0.8,
        cy - dir_y * len - py * len * 0.8,
    ];
    DrawOp::Quad {
        points: [tip, right, notch, left],
        argb,
    }
}

/// N, E, S and W on the rim.
pub fn compass(layout: &Layout, yaw: f32) -> Vec<DrawOp> {
    let axes = Axes::new(layout.rotate, yaw);
    let reach = (layout.size / 2.0 - 5.0) as f64;
    let [cx, cy] = layout.center();
    [
        ("N", [0.0, -1.0], 0xFFFF_5555u32),
        ("E", [1.0, 0.0], 0xFFFF_FFFF),
        ("S", [0.0, 1.0], 0xFFFF_FFFF),
        ("W", [-1.0, 0.0], 0xFFFF_FFFF),
    ]
    .into_iter()
    .map(|(label, dir, argb): (&str, [f64; 2], u32)| {
        let [dx, dy] = axes.to_screen(dir[0], dir[1], 1.0);
        let mut x = dx * reach;
        let mut y = dy * reach;
        if !layout.round {
            // Out to the square's edge along the same direction.
            let m = dx.abs().max(dy.abs()).max(1e-6);
            x = dx / m * reach;
            y = dy / m * reach;
        }
        DrawOp::Text {
            x: cx + x as f32,
            y: cy + y as f32 - 4.0,
            text: label.to_owned(),
            argb,
            align: Align::Center,
        }
    })
    .collect()
}

/// A thin frame around a square map.
pub fn frame(layout: &Layout, argb: u32) -> Vec<DrawOp> {
    if layout.round {
        return Vec::new();
    }
    let (x, y, s) = (layout.x, layout.y, layout.size);
    vec![
        DrawOp::Fill {
            rect: [x - 1.0, y - 1.0, s + 2.0, 1.0],
            argb,
        },
        DrawOp::Fill {
            rect: [x - 1.0, y + s, s + 2.0, 1.0],
            argb,
        },
        DrawOp::Fill {
            rect: [x - 1.0, y, 1.0, s],
            argb,
        },
        DrawOp::Fill {
            rect: [x + s, y, 1.0, s],
            argb,
        },
    ]
}

/// The info lines centred under (or above) the map.
pub fn info(layout: &Layout, lines: &[(String, u32)]) -> Vec<DrawOp> {
    let cx = layout.x + layout.size / 2.0;
    lines
        .iter()
        .enumerate()
        .map(|(i, (text, argb))| {
            let y = if layout.info_below {
                layout.y + layout.size + 3.0 + i as f32 * LINE_HEIGHT
            } else {
                layout.y - 3.0 - (lines.len() - i) as f32 * LINE_HEIGHT
            };
            DrawOp::Text {
                x: cx,
                y,
                text: text.clone(),
                argb: *argb,
                align: Align::Center,
            }
        })
        .collect()
}

/// The minimap's own state between frames: whether it is enlarged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Controls {
    held: bool,
    toggled: bool,
}

impl Controls {
    pub fn enlarged(&self) -> bool {
        self.held || self.toggled
    }

    /// Reacts to one of the minimap's keys; true when `settings` changed
    /// (and should be saved).
    pub fn key(&mut self, id: &str, pressed: bool, settings: &mut MinimapSettings) -> bool {
        match (id, pressed) {
            ("minimap.enlarge", _) if settings.enlarge_toggles => {
                if pressed {
                    self.toggled = !self.toggled;
                }
                false
            }
            ("minimap.enlarge", _) => {
                self.held = pressed;
                false
            }
            ("minimap.zoom_in", true) | ("minimap.zoom_out", true) => {
                let last = zoom_steps(settings.further_zoom_out).len() - 1;
                let enlarged = self.enlarged();
                let current = zoom_index(settings, enlarged);
                let next = if id == "minimap.zoom_in" {
                    current.saturating_sub(1)
                } else {
                    (current + 1).min(last)
                };
                // The enlarged map stores its step + 1 (0 = one out from normal).
                if enlarged {
                    settings.enlarged_zoom = next as u8 + 1;
                } else {
                    settings.zoom = next as u8;
                }
                next != current
            }
            ("minimap.toggle", true) => {
                settings.enabled = !settings.enabled;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> MinimapSettings {
        MinimapSettings::default()
    }

    fn approx(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn layout_in_corners() {
        let gui = [480.0, 270.0];
        let mut s = settings();
        s.size = MinimapSize::Large;
        let l = layout(&s, gui, false, 2);
        assert_eq!((l.x, l.y, l.size), (480.0 - 128.0 - MARGIN, MARGIN, 128.0));
        assert!(l.info_below && l.rotate && !l.round);
        s.corner = Corner::BottomLeft;
        let l = layout(&s, gui, false, 2);
        assert_eq!((l.x, l.y), (MARGIN, 270.0 - 128.0 - MARGIN));
        assert!(!l.info_below, "info goes above a bottom map");
        assert_eq!(info(&l, &[("a".into(), 0), ("b".into(), 0)]).len(), 2);
    }

    #[test]
    fn auto_size_follows_screen() {
        assert_eq!(side(MinimapSize::Auto, [480.0, 270.0]), 74.0);
        assert_eq!(side(MinimapSize::Auto, [2000.0, 2000.0]), 160.0);
        assert_eq!(side(MinimapSize::Auto, [100.0, 100.0]), 64.0);
    }

    #[test]
    fn enlarging_zooms_out_and_grows() {
        let s = settings();
        let gui = [480.0, 270.0];
        let normal = layout(&s, gui, false, 0);
        let big = layout(&s, gui, true, 0);
        assert!(big.size > normal.size);
        assert_eq!(normal.blocks_per_pixel, 1.0);
        assert_eq!(big.blocks_per_pixel, 2.0);
        let mut s = s;
        s.enlarged_centered = true;
        let c = layout(&s, gui, true, 0);
        assert_eq!(c.center(), [240.0, 135.0]);
        s.zoom = 200;
        assert_eq!(zoom_index(&s, false), 4, "clamped to the last step");
    }

    #[test]
    fn axes_turn_with_the_player() {
        // Facing south (yaw 0): forward is +z, right is -x (west).
        let a = Axes::new(true, 0.0);
        assert!(approx(a.to_world(0.0, -10.0, 1.0), [0.0, 10.0]));
        assert!(approx(a.to_world(10.0, 0.0, 1.0), [-10.0, 0.0]));
        // Facing west (yaw 90): forward is -x.
        let a = Axes::new(true, 90.0);
        assert!(approx(a.to_world(0.0, -1.0, 2.0), [-2.0, 0.0]));
        // North up: up is -z, right is +x.
        let n = Axes::new(false, 123.0);
        assert!(approx(n.to_world(3.0, -4.0, 1.0), [3.0, -4.0]));
        for yaw in [0.0f32, 37.0, 90.0, 211.0] {
            let a = Axes::new(true, yaw);
            let w = a.to_world(5.0, -7.0, 0.5);
            assert!(approx(a.to_screen(w[0], w[1], 0.5), [5.0, -7.0]));
        }
    }

    #[test]
    fn flat_terrain_merges_into_one_run_per_row() {
        let l = Layout {
            x: 0.0,
            y: 0.0,
            size: 64.0,
            blocks_per_pixel: 1.0,
            rotate: false,
            round: false,
            info_below: true,
        };
        let ops = terrain(&l, [0.0, 0.0], 0.0, 100, 50, |_, _| 0xFF33_6633);
        assert_eq!(ops.len(), 1 + 64, "backdrop + one run per row");
        let unexplored = terrain(&l, [0.0, 0.0], 0.0, 100, 50, |_, _| 0);
        assert_eq!(unexplored.len(), 1, "only the backdrop");
    }

    #[test]
    fn round_maps_skip_corners() {
        let l = Layout {
            x: 0.0,
            y: 0.0,
            size: 64.0,
            blocks_per_pixel: 1.0,
            rotate: true,
            round: true,
            info_below: true,
        };
        let ops = terrain(&l, [0.0, 0.0], 45.0, 100, 50, |x, z| {
            0xFF00_0000 | ((x & 1) as u32) << 8 | (z & 1) as u32
        });
        let mut covered = 0.0;
        for op in &ops {
            if let DrawOp::Fill {
                rect: [x, y, w, h],
                argb,
            } = op
            {
                if argb & 0x00FF_FFFF != 0x0010_1010 {
                    continue;
                }
                covered += w * h;
                for (px, py) in [(*x, *y), (x + w, *y), (*x, y + h), (x + w, y + h)] {
                    let d = ((px - 32.0).powi(2) + (py - 32.0).powi(2)).sqrt();
                    assert!(
                        d <= 32.0 * 1.5,
                        "backdrop far outside the circle at {px},{py}"
                    );
                }
            }
        }
        let circle = std::f32::consts::PI * 32.0 * 32.0;
        assert!(
            (covered - circle).abs() / circle < 0.08,
            "covered {covered} of {circle}"
        );
    }

    #[test]
    fn markers_clamp_to_the_rim() {
        let l = Layout {
            x: 0.0,
            y: 0.0,
            size: 100.0,
            blocks_per_pixel: 1.0,
            rotate: false,
            round: false,
            info_below: true,
        };
        let (p, clamped) = project(&l, [0.0, 0.0], 0.0, [10.0, -20.0], 0.0);
        assert_eq!((p, clamped), ([60.0, 30.0], false));
        let (p, clamped) = project(&l, [0.0, 0.0], 0.0, [1000.0, 0.0], 4.0);
        assert!(clamped);
        assert_eq!(p, [96.0, 50.0]);
        let round = Layout { round: true, ..l };
        let (p, _) = project(&round, [0.0, 0.0], 0.0, [1000.0, 1000.0], 0.0);
        let d = ((p[0] - 50.0).powi(2) + (p[1] - 50.0).powi(2)).sqrt();
        assert!((d - 50.0).abs() < 1e-3);
    }

    #[test]
    fn north_label_follows_rotation() {
        let l = Layout {
            x: 0.0,
            y: 0.0,
            size: 100.0,
            blocks_per_pixel: 1.0,
            rotate: true,
            round: true,
            info_below: true,
        };
        // Facing north (yaw 180): N is at the top.
        let n = &compass(&l, 180.0)[0];
        let DrawOp::Text { x, y, .. } = n else {
            panic!()
        };
        assert!((x - 50.0).abs() < 1e-3 && *y < 20.0);
        // Facing south: N at the bottom.
        let DrawOp::Text { y, .. } = &compass(&l, 0.0)[0] else {
            panic!()
        };
        assert!(*y > 80.0);
    }

    #[test]
    fn enlarge_holds_or_toggles() {
        let mut s = settings();
        let mut c = Controls::default();
        c.key("minimap.enlarge", true, &mut s);
        assert!(c.enlarged());
        c.key("minimap.enlarge", false, &mut s);
        assert!(!c.enlarged(), "held by default");
        s.enlarge_toggles = true;
        c.key("minimap.enlarge", true, &mut s);
        c.key("minimap.enlarge", false, &mut s);
        assert!(c.enlarged(), "toggled");
        c.key("minimap.enlarge", true, &mut s);
        assert!(!c.enlarged());
    }

    #[test]
    fn zoom_keys_step_and_stop() {
        let mut s = settings();
        let mut c = Controls::default();
        assert!(c.key("minimap.zoom_in", true, &mut s));
        assert_eq!(s.zoom, 0);
        assert!(!c.key("minimap.zoom_in", true, &mut s), "already closest");
        for _ in 0..10 {
            c.key("minimap.zoom_out", true, &mut s);
        }
        assert_eq!(s.zoom as usize, zoom_steps(false).len() - 1);
        assert!(
            !c.key("minimap.zoom_out", false, &mut s),
            "releases do nothing"
        );
        // Enlarged: its own step, starting one out from normal.
        s.zoom = 1;
        c.key("minimap.enlarge", true, &mut s);
        assert_eq!(zoom_index(&s, true), 2);
        c.key("minimap.zoom_in", true, &mut s);
        assert_eq!(zoom_index(&s, true), 1);
        assert_eq!(s.zoom, 1, "the normal step is untouched");
        assert!(c.key("minimap.toggle", true, &mut s));
        assert!(!s.enabled);
    }

    #[test]
    fn opacity_scales_alpha() {
        assert_eq!(with_opacity(0xFF12_3456, 100), 0xFF12_3456);
        assert_eq!(with_opacity(0xFF12_3456, 0), 0x0012_3456);
        assert_eq!(with_opacity(0xFF12_3456, 50) >> 24, 128);
    }
}

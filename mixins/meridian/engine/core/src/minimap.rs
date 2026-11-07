//! The minimap's layout and drawing, as a list of plain draw operations.
//!
//! The host turns [`DrawOp`]s into its own HUD calls; nothing here knows
//! Horizon. The map is drawn on a screen-space grid of cells: each cell
//! samples the map at the world point under its centre (turned with the
//! player unless north is locked), and runs of equal cells in a row merge
//! into one rectangle. A round map skips the cells outside its circle.

use crate::config::{Corner, MinimapSettings, MinimapShape, MinimapSize};

/// Height of one info line under the map, GUI pixels.
pub const LINE_HEIGHT: f32 = 10.0;
/// The most cells drawn per side; larger maps use bigger cells.
const MAX_CELLS_PER_SIDE: f32 = 160.0;
/// Blocks per minimap unit at the default zoom (a unit is about three
/// physical pixels on a 1080p-class screen: 1.5 pixels per block).
pub const BLOCKS_PER_UNIT: f32 = 2.0;
/// Screen height, in minimap units, that one unit is measured against: the
/// minimap's frame, marks and spacing scale with the window, not with the
/// GUI scale.
const UNITS_PER_SCREEN: f32 = 594.0;

/// Multipliers of the default zoom (blocks per pixel) for each zoom step;
/// `further_zoom_out` adds two. Step 1 is the default.
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
        /// GUI pixels per font pixel.
        scale: f32,
    },
    /// A dark plate sized to a text drawn at (`x`, `y`) with `align` and
    /// `scale` (the host knows the font's widths), `pad` GUI pixels around.
    Plate {
        x: f32,
        y: f32,
        text: String,
        align: Align,
        scale: f32,
        pad: f32,
        argb: u32,
    },
    /// One of the game's own GUI sprites (`widget/button`), stretched
    /// nine-slice into `rect`.
    NineSlice {
        sprite: &'static str,
        rect: [f32; 4],
        argb: u32,
    },
    /// A Seed Atlas structure icon (by marker id, `completed` for the
    /// badge), fitted into `rect`; greyed out when `dim`.
    Icon {
        id: String,
        rect: [f32; 4],
        dim: bool,
    },
    /// Text wrapped to `width` (the host measures it).
    Paragraph {
        x: f32,
        y: f32,
        width: f32,
        text: String,
        argb: u32,
    },
    /// Part of one of the game's textures (`entity/zombie/zombie`, the path
    /// under `textures/`): `uv` = u, v, width, height in texels of a
    /// 64-texel-wide texture, stretched into `rect`, tinted by `argb`.
    Blit {
        texture: String,
        rect: [f32; 4],
        uv: [u16; 4],
        argb: u32,
    },
}

/// Where and how big the map is this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    /// Top left corner of the map square (inside the frame).
    pub x: f32,
    pub y: f32,
    pub size: f32,
    /// Blocks per GUI pixel.
    pub blocks_per_pixel: f32,
    pub rotate: bool,
    pub round: bool,
    /// Info lines go below the map (true) or above it (bottom corners).
    pub info_below: bool,
    /// GUI pixels per minimap unit (frame lines, marks, spacing).
    pub unit: f32,
}

impl Layout {
    pub fn center(&self) -> [f32; 2] {
        [self.x + self.size / 2.0, self.y + self.size / 2.0]
    }

    /// Width of the frame around the map.
    pub fn frame_width(&self) -> f32 {
        if self.round { 0.0 } else { 4.0 * self.unit }
    }
}

/// GUI pixels per minimap unit on a screen `gui_height` GUI pixels tall at
/// `gui_scale` physical pixels per GUI pixel. A unit is a whole number of
/// physical pixels, so lines stay sharp.
pub fn unit(gui_height: f32, gui_scale: f32) -> f32 {
    let scale = gui_scale.max(0.25);
    let physical = (gui_height * scale / UNITS_PER_SCREEN).round().max(1.0);
    physical / scale
}

/// The map's side (inside the frame) for a size setting and screen.
pub fn side(size: MinimapSize, gui: [f32; 2]) -> f32 {
    let share = match size {
        MinimapSize::Auto => 0.232,
        MinimapSize::Small => 0.16,
        MinimapSize::Medium => 0.2,
        MinimapSize::Large => 0.28,
        MinimapSize::Huge => 0.36,
    };
    let px = (gui[1] * share).clamp(48.0, gui[0].min(gui[1]) * 0.9);
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

/// Where the map goes on a `gui`-sized screen drawn at `gui_scale`
/// physical pixels per GUI pixel, with `info_lines` lines of text beside it.
pub fn layout(
    s: &MinimapSettings,
    gui: [f32; 2],
    gui_scale: f32,
    enlarged: bool,
    info_lines: usize,
) -> Layout {
    let u = unit(gui[1], gui_scale);
    let normal = side(s.size, gui);
    let size = if enlarged {
        let big = (gui[1] * 0.6).floor();
        ((big / 2.0).floor() * 2.0).max(normal)
    } else {
        normal
    };
    let frame = if s.shape == MinimapShape::Round {
        0.0
    } else {
        4.0 * u
    };
    let margin = 5.0 * u + frame;
    let info = if info_lines > 0 {
        3.0 * u + info_lines as f32 * LINE_HEIGHT
    } else {
        0.0
    };
    let (x, y, info_below) = if enlarged && s.enlarged_centered {
        ((gui[0] - size) / 2.0, (gui[1] - size) / 2.0, true)
    } else {
        let left = margin + s.offset[0] as f32;
        let right = gui[0] - size - margin - s.offset[0] as f32;
        let top = margin + s.offset[1] as f32;
        let bottom = gui[1] - size - margin - s.offset[1] as f32;
        match s.corner {
            Corner::TopLeft => (left, top, true),
            Corner::TopRight => (right, top, true),
            Corner::BottomLeft => (left, bottom, false),
            Corner::BottomRight => (right, bottom, false),
        }
    };
    let _ = info;
    let steps = zoom_steps(s.further_zoom_out);
    let default_bpp = BLOCKS_PER_UNIT / u;
    Layout {
        x: x.round(),
        y: y.round(),
        size,
        blocks_per_pixel: steps[zoom_index(s, enlarged)] * default_bpp,
        rotate: !s.north_locked,
        round: s.shape == MinimapShape::Round,
        info_below,
        unit: u,
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
    // One block per cell where the zoom allows; never more cells than the limit.
    let cell = (1.0 / layout.blocks_per_pixel).max(layout.size / MAX_CELLS_PER_SIDE);
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

/// The player's mark at the map's centre: a small cross while the map turns
/// with the player (forward is always up), an arrow along the player's
/// facing when north is locked.
pub fn player_mark(layout: &Layout, yaw: f32, scale: f32) -> Vec<DrawOp> {
    let [cx, cy] = layout.center();
    let u = layout.unit;
    if layout.rotate {
        let (white, tip) = (0xFFFF_FFFF, 0xFFB4_8CDC);
        let px = |dx: f32, dy: f32, argb: u32| DrawOp::Fill {
            rect: [cx + (dx - 0.5) * u, cy + (dy - 0.5) * u, u, u],
            argb,
        };
        return vec![
            px(0.0, -2.0, tip),
            px(0.0, -1.0, white),
            px(-2.0, 0.0, tip),
            px(-1.0, 0.0, white),
            px(0.0, 0.0, tip),
            px(1.0, 0.0, white),
            px(2.0, 0.0, tip),
            px(0.0, 1.0, white),
            px(0.0, 2.0, tip),
        ];
    }
    let (s, c) = yaw.to_radians().sin_cos();
    let (dir_x, dir_y) = (-s, c);
    let len = 2.5 * u * scale;
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
    vec![DrawOp::Quad {
        points: [tip, right, notch, left],
        argb: 0xFFFF_FFFF,
    }]
}

/// N, E, S and W just inside the rim, each on a small blue plate.
pub fn compass(layout: &Layout, yaw: f32) -> Vec<DrawOp> {
    let axes = Axes::new(layout.rotate, yaw);
    let u = layout.unit;
    let reach = (layout.size / 2.0 - 4.0 * u) as f64;
    let [cx, cy] = layout.center();
    let mut ops = Vec::new();
    for (label, dir) in [
        ("N", [0.0, -1.0]),
        ("E", [1.0, 0.0]),
        ("S", [0.0, 1.0]),
        ("W", [-1.0, 0.0]),
    ] {
        let [dx, dy] = axes.to_screen(dir[0], dir[1], 1.0);
        let (mut x, mut y) = (dx * reach, dy * reach);
        if !layout.round {
            // Out to the square's edge along the same direction.
            let m = dx.abs().max(dy.abs()).max(1e-6);
            x = dx / m * reach;
            y = dy / m * reach;
        }
        let (x, y) = (cx + x as f32, cy + y as f32);
        let half = 3.0 * u;
        ops.push(DrawOp::Fill {
            rect: [x - half, y - half, 2.0 * half, 2.0 * half],
            argb: 0xE06C_6CE8,
        });
        let scale = 0.75 * u;
        ops.push(DrawOp::Text {
            x: x + 0.5 * scale,
            y: y - 3.5 * scale,
            text: label.to_owned(),
            argb: 0xFFFF_FFFF,
            align: Align::Center,
            scale,
        });
    }
    ops
}

/// The bevelled frame around a square map: black outside, a light grey
/// band, a dark line inside.
pub fn frame(layout: &Layout) -> Vec<DrawOp> {
    if layout.round {
        return Vec::new();
    }
    let u = layout.unit;
    let ring = |inset: f32, width: f32, argb: u32| {
        // `inset` units outside the map, `width` units wide.
        let o = inset * u;
        let w = width * u;
        let (x, y, s) = (layout.x - o, layout.y - o, layout.size + 2.0 * o);
        [
            DrawOp::Fill {
                rect: [x, y, s, w],
                argb,
            },
            DrawOp::Fill {
                rect: [x, y + s - w, s, w],
                argb,
            },
            DrawOp::Fill {
                rect: [x, y + w, w, s - 2.0 * w],
                argb,
            },
            DrawOp::Fill {
                rect: [x + s - w, y + w, w, s - 2.0 * w],
                argb,
            },
        ]
    };
    let mut ops = Vec::new();
    ops.extend(ring(4.0, 1.0, 0xFF00_0000));
    ops.extend(ring(3.0, 2.0, 0xFFA8_A8A8));
    // A lighter edge on the band's top and left, as if lit from there.
    let (o, w) = (3.0 * u, u);
    let (x, y, s) = (layout.x - o, layout.y - o, layout.size + 2.0 * o);
    ops.push(DrawOp::Fill {
        rect: [x, y, s - w, w],
        argb: 0xFFD0_D0D0,
    });
    ops.push(DrawOp::Fill {
        rect: [x, y, w, s - w],
        argb: 0xFFD0_D0D0,
    });
    ops.extend(ring(1.0, 1.0, 0xFF1E_1E1E));
    ops
}

/// The info lines centred under (or above) the map and its frame.
pub fn info(layout: &Layout, lines: &[(String, u32)]) -> Vec<DrawOp> {
    let cx = layout.x + layout.size / 2.0;
    let gap = layout.frame_width() + 3.0 * layout.unit;
    let line = LINE_HEIGHT * layout.unit;
    lines
        .iter()
        .enumerate()
        .map(|(i, (text, argb))| {
            let y = if layout.info_below {
                layout.y + layout.size + gap + i as f32 * line
            } else {
                layout.y - gap - (lines.len() - i) as f32 * line
            };
            DrawOp::Text {
                x: cx,
                y,
                text: text.clone(),
                argb: *argb,
                align: Align::Center,
                scale: layout.unit,
            }
        })
        .collect()
}

/// A waypoint's mark: a square in its colour with its initial in white.
/// `size` is the square's side in GUI pixels, `text_scale` the initial's.
pub fn waypoint_mark(
    at: [f32; 2],
    size: f32,
    argb: u32,
    initial: &str,
    text_scale: f32,
) -> Vec<DrawOp> {
    let [x, y] = at;
    let half = size / 2.0;
    vec![
        DrawOp::Fill {
            rect: [x - half, y - half, size, size],
            argb,
        },
        DrawOp::Text {
            x: x + 0.5 * text_scale,
            y: y - 3.5 * text_scale,
            text: initial.to_owned(),
            argb: 0xFFFF_FFFF,
            align: Align::Center,
            scale: text_scale,
        },
    ]
}

/// Where a world point appears on screen (GUI pixels), from the camera's
/// position and its camera-relative view and projection matrices (column
/// major, as Horizon gives them); `None` behind the camera.
pub fn world_to_screen(
    camera: [f64; 3],
    view: &[f32; 16],
    projection: &[f32; 16],
    point: [f64; 3],
    gui: [f32; 2],
) -> Option<[f32; 2]> {
    let rel = [
        (point[0] - camera[0]) as f32,
        (point[1] - camera[1]) as f32,
        (point[2] - camera[2]) as f32,
        1.0,
    ];
    let mul = |m: &[f32; 16], v: [f32; 4]| {
        let mut out = [0.0f32; 4];
        for (row, o) in out.iter_mut().enumerate() {
            *o = (0..4).map(|col| m[col * 4 + row] * v[col]).sum();
        }
        out
    };
    let clip = mul(projection, mul(view, rel));
    if clip[3] <= 1e-4 {
        return None;
    }
    let (nx, ny) = (clip[0] / clip[3], clip[1] / clip[3]);
    Some([(nx + 1.0) / 2.0 * gui[0], (1.0 - ny) / 2.0 * gui[1]])
}

/// "30.1m" / "30,1m" (one decimal, the locale's separator), "12.3km" from
/// `km_from` blocks.
pub fn distance_text(blocks: f64, km_from: u32, comma: bool) -> String {
    let text = if km_from > 0 && blocks >= km_from as f64 {
        format!("{:.1}km", blocks / 1000.0)
    } else {
        format!("{blocks:.1}m")
    };
    if comma { text.replace('.', ",") } else { text }
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
        // A 1280 x 720 GUI at scale 2: one unit is 720 / 594 GUI pixels.
        let gui = [1280.0, 720.0];
        let s = settings();
        assert_eq!(
            s.corner,
            Corner::TopLeft,
            "top left by default, like the reference"
        );
        let l = layout(&s, gui, 2.0, false, 1);
        let u = 1.0f32;
        assert_eq!(l.size, 166.0);
        assert_eq!((l.x, l.y), ((9.0 * u).round(), (9.0 * u).round()));
        assert!(l.info_below && l.rotate && !l.round);
        assert!(
            (l.blocks_per_pixel - 2.0).abs() < 1e-6,
            "two blocks per unit"
        );
        let mut s = s;
        s.corner = Corner::BottomRight;
        let l = layout(&s, gui, 2.0, false, 1);
        let right_edge = l.x + l.size + l.frame_width() + 5.0 * l.unit;
        assert!((1280.0 - right_edge).abs() <= 1.0, "{right_edge}");
        assert!(!l.info_below, "info goes above a bottom map");
        let lines = info(&l, &[("a".into(), 0), ("b".into(), 0)]);
        let DrawOp::Text { y, .. } = &lines[1] else {
            panic!()
        };
        assert!(*y < l.y);
    }

    #[test]
    fn size_follows_screen_height() {
        assert_eq!(side(MinimapSize::Auto, [1280.0, 720.0]), 166.0);
        assert_eq!(side(MinimapSize::Auto, [432.0, 254.0]), 58.0);
        assert_eq!(side(MinimapSize::Auto, [100.0, 100.0]), 48.0);
        assert!((unit(594.0, 1.0) - 1.0).abs() < 1e-6);
        assert!(
            (unit(344.0, 5.0) - 0.6).abs() < 1e-6,
            "3 physical pixels at 1720 px"
        );
    }

    #[test]
    fn enlarging_zooms_out_and_grows() {
        let s = settings();
        let gui = [1280.0, 720.0];
        let normal = layout(&s, gui, 2.0, false, 0);
        let big = layout(&s, gui, 2.0, true, 0);
        assert!(big.size > normal.size);
        assert_eq!(big.blocks_per_pixel, 2.0 * normal.blocks_per_pixel);
        let mut s = s;
        s.enlarged_centered = true;
        let c = layout(&s, gui, 2.0, true, 0);
        assert_eq!(c.center(), [640.0, 360.0]);
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
            unit: 1.0,
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
            unit: 1.0,
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
            unit: 1.0,
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
            unit: 1.0,
        };
        // Facing north (yaw 180): N is at the top.
        let n = &compass(&l, 180.0)[1];
        let DrawOp::Text { x, y, .. } = n else {
            panic!()
        };
        assert!((x - 50.0).abs() <= 1.0 && *y < 20.0);
        // Facing south: N at the bottom.
        let DrawOp::Text { y, .. } = &compass(&l, 0.0)[1] else {
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
    fn projects_points_in_front_and_drops_points_behind() {
        // Identity view; a simple perspective looking down -z.
        let view = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut proj = [0.0f32; 16];
        proj[0] = 1.0;
        proj[5] = 1.0;
        proj[11] = -1.0; // w = -z
        let gui = [200.0, 100.0];
        let ahead = world_to_screen([0.0; 3], &view, &proj, [0.0, 0.0, -10.0], gui).unwrap();
        assert_eq!(ahead, [100.0, 50.0]);
        let right_up = world_to_screen([0.0; 3], &view, &proj, [5.0, 5.0, -10.0], gui).unwrap();
        assert_eq!(right_up, [150.0, 25.0]);
        assert!(world_to_screen([0.0; 3], &view, &proj, [0.0, 0.0, 10.0], gui).is_none());
    }

    #[test]
    fn distances_like_the_reference() {
        assert_eq!(distance_text(30.06, 10_000, true), "30,1m");
        assert_eq!(distance_text(30.06, 10_000, false), "30.1m");
        assert_eq!(distance_text(12_345.0, 10_000, false), "12.3km");
    }

    #[test]
    fn frame_and_marks() {
        let l = Layout {
            x: 10.0,
            y: 10.0,
            size: 100.0,
            blocks_per_pixel: 1.0,
            rotate: true,
            round: false,
            info_below: true,
            unit: 1.0,
        };
        let f = frame(&l);
        let outer = f.iter().find_map(|op| match op {
            DrawOp::Fill {
                rect,
                argb: 0xFF00_0000,
            } => Some(*rect),
            _ => None,
        });
        assert_eq!(
            outer,
            Some([6.0, 6.0, 108.0, 1.0]),
            "black line 4 units out"
        );
        assert_eq!(player_mark(&l, 0.0, 1.0).len(), 9, "a cross while turning");
        let locked = Layout { rotate: false, ..l };
        assert!(
            matches!(player_mark(&locked, 0.0, 1.0)[0], DrawOp::Quad { .. }),
            "an arrow when north is locked"
        );
        assert_eq!(
            waypoint_mark([50.2, 50.0], 9.0, 0xFF00_FF00, "H", 1.0).len(),
            2
        );
    }

    #[test]
    fn opacity_scales_alpha() {
        assert_eq!(with_opacity(0xFF12_3456, 100), 0xFF12_3456);
        assert_eq!(with_opacity(0xFF12_3456, 0), 0x0012_3456);
        assert_eq!(with_opacity(0xFF12_3456, 50) >> 24, 128);
    }
}

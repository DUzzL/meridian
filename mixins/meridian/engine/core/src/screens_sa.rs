//! The Seed Atlas settings as the Fabric version 2.0 lays them out
//! (`SeedAtlasSettingsScreen`, `MarkerSettingsScreen`, `MarkerToggleList`,
//! `PerformanceSettingsScreen`): the same buttons, texts and positions, in
//! GUI pixels from the top of the screen.

use crate::minimap::Align;
use crate::screens::{Action, Env, Screen};
use crate::screens_seedatlas::{
    BiomeHighlightScreen, ConfirmResetScreen, NearestScreen, WorldGenerationScreen,
};
use crate::seedatlas::MARKERS;
use crate::seedatlas_state::{game_version, world_type_label};
use crate::ui::{self, Widget};

const GREY: u32 = 0xFFA0_A0A0;
const WHITE: u32 = 0xFFFF_FFFF;

use crate::seedatlas_state::{
    BIOME_RESOLUTION_STEPS, MARKER_SIZE_STEPS, PREFETCH_RADIUS_STEPS, WORKER_THREAD_STEPS,
};

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn next<T: Copy + PartialEq>(steps: &[T], now: T, default: T) -> T {
    steps
        .iter()
        .position(|s| *s == now)
        .map_or(default, |i| steps[(i + 1) % steps.len()])
}

/// `contentWidth` and `left` of the Fabric version's compact screens.
fn content(env: &Env<'_>) -> (f32, f32) {
    let width = 340f32.min(env.gui[0] - 20.0);
    (width, ((env.gui[0] - width) / 2.0).floor())
}

fn done(env: &Env<'_>, width: f32) -> Widget {
    Widget::button(
        "done",
        [
            ((env.gui[0] - width) / 2.0).floor(),
            (env.gui[1] - 28.0).max(0.0),
            width,
            20.0,
        ],
        "Done",
        true,
    )
}

/// The slider value under `x` (`AbstractSliderButton.setValueFromMouse`).
fn slider_value(rect: [f32; 4], x: f32) -> f32 {
    ((x - (rect[0] + 4.0)) / (rect[2] - 8.0)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------- main --

/// "Seed Atlas Settings".
#[derive(Debug, Default, PartialEq)]
pub struct SeedAtlasSettingsScreen {
    /// The Y fields (Overworld, Nether, End) as typed; `None` until edited.
    pub heights: [Option<String>; 3],
    pub focused: Option<usize>,
    /// The opacity slider is being dragged.
    pub dragging: bool,
    /// Opened from the minimap settings (Done goes back there).
    pub from_settings: bool,
}

impl SeedAtlasSettingsScreen {
    pub fn new() -> Self {
        Self::default()
    }

    fn height(&self, env: &Env<'_>, i: usize) -> String {
        let sa = &env.settings.seed_atlas;
        self.heights[i]
            .clone()
            .unwrap_or_else(|| [sa.overworld_y, sa.nether_y, sa.end_y][i].to_string())
    }

    fn opacity_rect(env: &Env<'_>) -> [f32; 4] {
        let (width, left) = content(env);
        [left, 59.0, ((width - 6.0) / 2.0).floor(), 20.0]
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let sa = &env.settings.seed_atlas;
        let (width, left) = content(env);
        let gap = 6.0;
        let half = ((width - gap) / 2.0).floor();
        let right = left + half + gap;
        let mut ws = vec![Widget::text(
            env.gui[0] / 2.0,
            14.0,
            "Seed Atlas Settings",
            WHITE,
            Align::Center,
        )];
        let (version, _) = game_version(sa, &sa.context_key);
        ws.push(Widget::button(
            "sa:layer",
            [left, 34.0, half, 20.0],
            format!("Seed map layer: {}", on_off(sa.biome_layer)),
            true,
        ));
        ws.push(Widget::button(
            "sa:world",
            [right, 34.0, half, 20.0],
            format!(
                "World: {}, {}",
                version.label(),
                world_type_label(sa.large_biomes)
            ),
            true,
        ));
        let opacity = sa.biome_opacity as f32 / 255.0;
        ws.push(Widget::slider(
            "sa:opacity",
            Self::opacity_rect(env),
            format!("Layer opacity: {}%", (opacity * 100.0).round() as i32),
            opacity,
        ));
        ws.push(Widget::button(
            "sa:overlay",
            [right, 59.0, half, 20.0],
            format!("Overlay explored: {}", on_off(sa.overlay_explored)),
            true,
        ));
        let field_w = ((width - 12.0) / 3.0).floor();
        for (i, label) in ["Overworld Y", "Nether Y", "End Y"].into_iter().enumerate() {
            let x = left + (field_w + 6.0) * i as f32;
            ws.push(Widget::text(
                x + (field_w / 2.0).floor(),
                87.0,
                label,
                GREY,
                Align::Center,
            ));
            let value = [sa.overworld_y, sa.nether_y, sa.end_y][i].to_string();
            ws.push(Widget::field(
                &format!("sa:y{i}"),
                [x, 98.0, field_w, 20.0],
                &self.height(env, i),
                &value,
                self.focused == Some(i),
            ));
        }
        ws.push(Widget::button(
            "sa:structures",
            [left, 125.0, half, 20.0],
            format!("Structures: {}", on_off(sa.structures)),
            true,
        ));
        ws.push(Widget::button(
            "sa:markers",
            [right, 125.0, half, 20.0],
            "Structure types...",
            true,
        ));
        ws.push(Widget::button(
            "sa:performance",
            [left, 150.0, half, 20.0],
            "Performance...",
            true,
        ));
        let terrain = version.has_terrain();
        ws.push(Widget::button(
            "sa:terrain",
            [right, 150.0, half, 20.0],
            if terrain {
                format!("Terrain look: {}", on_off(sa.terrain_look))
            } else {
                "Terrain look: from 1.18".into()
            },
            terrain,
        ));
        let third = ((width - gap * 2.0) / 3.0).floor();
        ws.push(Widget::button(
            "sa:highlight",
            [left, 175.0, third, 20.0],
            "Biome highlights...",
            true,
        ));
        ws.push(Widget::button(
            "sa:biome",
            [left + third + gap, 175.0, third, 20.0],
            "Nearest biome...",
            true,
        ));
        ws.push(Widget::button(
            "sa:structure",
            [
                left + (third + gap) * 2.0,
                175.0,
                width - (third + gap) * 2.0,
                20.0,
            ],
            "Nearest structure...",
            true,
        ));
        ws.push(Widget::text(
            env.gui[0] / 2.0,
            env.gui[1] - 42.0,
            env.seed_atlas.seed_status.clone(),
            GREY,
            Align::Center,
        ));
        ws.push(done(env, 200.0));
        ws
    }

    /// The tooltips of the Fabric version's buttons.
    pub(crate) fn tooltip(&self, id: &str, env: &Env<'_>) -> Option<String> {
        let terrain = game_version(
            &env.settings.seed_atlas,
            &env.settings.seed_atlas.context_key,
        )
        .0
        .has_terrain();
        Some(match id {
            "sa:world" => "World type and the Minecraft version this world or server generates with.".into(),
            "sa:opacity" => "Below 100%, the seed layer is blended with the explored terrain underneath.".into(),
            "sa:overlay" => "Also draws the seed map over explored terrain. Layer opacity blends both maps; biome focus is always drawn on top.".into(),
            "sa:terrain" if terrain => "Shades the seed map with predicted terrain heights and biome colours. Trees, structures and exact surface blocks are not predicted.".into(),
            "sa:terrain" => "Minecraft versions before 1.18 have no noise-based terrain to predict, so the map shows plain biome colours.".into(),
            _ => return None,
        })
    }

    /// Writes the typed heights into the settings (`commitValues`).
    fn commit(&mut self, env: &mut Env<'_>) {
        let sa = &mut env.settings.seed_atlas;
        for (i, typed) in self.heights.iter_mut().enumerate() {
            if let Some(text) = typed.take()
                && let Ok(y) = text.trim().parse::<i32>()
            {
                let slot = match i {
                    0 => &mut sa.overworld_y,
                    1 => &mut sa.nether_y,
                    _ => &mut sa.end_y,
                };
                if *slot != y {
                    *slot = y;
                    env.settings_changed = true;
                }
            }
        }
        self.focused = None;
    }

    pub(crate) fn click(&mut self, id: Option<&str>, at: [f32; 2], env: &mut Env<'_>) -> Action {
        if let Some(i) = id
            .and_then(|id| id.strip_prefix("sa:y"))
            .and_then(|i| i.parse::<usize>().ok())
        {
            self.focused = Some(i);
            if self.heights[i].is_none() {
                self.heights[i] = Some(self.height(env, i));
            }
            return Action::None;
        }
        let opacity = (slider_value(Self::opacity_rect(env), at[0]) * 255.0).round() as u8;
        let sa = &mut env.settings.seed_atlas;
        let mut changed = true;
        let open = |s: Screen| Action::Open(s);
        let action = match id {
            Some("sa:layer") => {
                sa.biome_layer = !sa.biome_layer;
                Action::None
            }
            Some("sa:opacity") => {
                self.dragging = true;
                sa.biome_opacity = opacity;
                Action::None
            }
            Some("sa:overlay") => {
                sa.overlay_explored = !sa.overlay_explored;
                Action::None
            }
            Some("sa:structures") => {
                sa.structures = !sa.structures;
                Action::None
            }
            Some("sa:terrain") => {
                sa.terrain_look = !sa.terrain_look;
                Action::None
            }
            other => {
                changed = false;
                self.commit(env);
                match other {
                    Some("sa:world") => {
                        open(Screen::WorldGeneration(WorldGenerationScreen::new(env)))
                    }
                    Some("sa:markers") => {
                        open(Screen::SeedAtlasMarkers(MarkerSettingsScreen::default()))
                    }
                    Some("sa:performance") => {
                        open(Screen::SeedAtlasPerformance(PerformanceSettingsScreen))
                    }
                    Some("sa:highlight") => {
                        open(Screen::BiomeHighlight(BiomeHighlightScreen::new()))
                    }
                    Some("sa:biome") => open(Screen::Nearest(NearestScreen::biome(env))),
                    Some("sa:structure") => open(Screen::Nearest(NearestScreen::structure(env))),
                    Some("done") if self.from_settings => {
                        open(Screen::Settings(crate::screens::SettingsScreen::default()))
                    }
                    Some("done") => Action::Close,
                    _ => Action::None,
                }
            }
        };
        if changed {
            env.settings_changed = true;
        }
        action
    }

    pub(crate) fn drag(&mut self, at: [f32; 2], env: &mut Env<'_>) {
        if self.dragging {
            let v = (slider_value(Self::opacity_rect(env), at[0]) * 255.0).round() as u8;
            if env.settings.seed_atlas.biome_opacity != v {
                env.settings.seed_atlas.biome_opacity = v;
                env.settings_changed = true;
            }
        }
    }

    pub(crate) fn typed(&mut self, c: char) {
        if let Some(i) = self.focused
            && let Some(text) = self.heights[i].as_mut()
        {
            ui::type_char(text, c, 6, |c| c.is_ascii_digit() || c == '-');
        }
    }

    pub(crate) fn key(&mut self, key: &str, env: &mut Env<'_>) -> Action {
        match key {
            "key.keyboard.backspace" => {
                if let Some(i) = self.focused
                    && let Some(text) = self.heights[i].as_mut()
                {
                    ui::backspace(text);
                }
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => self.commit(env),
            "key.keyboard.tab" => self.focused = Some(self.focused.map_or(0, |i| (i + 1) % 3)),
            "key.keyboard.escape" => {
                self.commit(env);
                return Action::Close;
            }
            _ => {}
        }
        Action::None
    }
}

// ------------------------------------------------------------- markers --

const HEADER_HEIGHT: f32 = 68.0;
const FOOTER_HEIGHT: f32 = 38.0;
const ROW_HEIGHT: f32 = 28.0;
const ROW_GAP: f32 = 2.0;
const CONTENT_PAD: f32 = 6.0;
const ICON_SIZE: f32 = 20.0;
const TOGGLE_WIDTH: f32 = 56.0;
const RESET_WIDTH: f32 = 40.0;
const SCROLLBAR_WIDTH: f32 = 6.0;
const SCROLLBAR_PAD: f32 = 3.0;
const BADGE_SIZE: f32 = 12.0;

/// "Seed Atlas Markers": completed opacity, marker size, the reset of every
/// completed structure, and a list of every structure kind.
#[derive(Debug, Default, PartialEq)]
pub struct MarkerSettingsScreen {
    pub scroll: f32,
    dragging_slider: bool,
    dragging_scrollbar: bool,
}

impl MarkerSettingsScreen {
    fn list_rect(env: &Env<'_>) -> [f32; 4] {
        let [w, h] = env.gui;
        let margin = 16f32.min(6f32.max((w / 18.0).floor()));
        let width = 420f32.min(180f32.max(w - margin * 2.0)).min(w - 8.0);
        let left = ((w - width) / 2.0).floor();
        let top = HEADER_HEIGHT;
        let bottom = (top + 48.0).max(h - FOOTER_HEIGHT).min(h - 26.0);
        [left, top, width, 48f32.max(bottom - top)]
    }

    fn header(env: &Env<'_>) -> ([f32; 4], [f32; 4], [f32; 4]) {
        let [left, _, width, _] = Self::list_rect(env);
        let third = ((width - 12.0) / 3.0).floor();
        (
            [left, 40.0, third, 20.0],
            [left + third + 6.0, 40.0, third, 20.0],
            [
                left + (third + 6.0) * 2.0,
                40.0,
                width - (third + 6.0) * 2.0,
                20.0,
            ],
        )
    }

    fn max_scroll(env: &Env<'_>) -> f32 {
        let [_, _, _, h] = Self::list_rect(env);
        let n = MARKERS.len() as f32;
        let content = n * ROW_HEIGHT + (n - 1.0).max(0.0) * ROW_GAP;
        (content - (h - CONTENT_PAD * 2.0).max(0.0)).max(0.0)
    }

    fn content_right(env: &Env<'_>) -> f32 {
        let [x, _, w, _] = Self::list_rect(env);
        x + w
            - CONTENT_PAD
            - if Self::max_scroll(env) > 0.0 {
                SCROLLBAR_WIDTH + SCROLLBAR_PAD + 2.0
            } else {
                0.0
            }
    }

    /// The scrollbar's track and thumb (`renderScrollbar`), if it scrolls.
    fn scrollbar(&self, env: &Env<'_>) -> Option<([f32; 4], [f32; 4])> {
        let max = Self::max_scroll(env);
        if max <= 0.0 {
            return None;
        }
        let [x, y, w, h] = Self::list_rect(env);
        let left = x + w - SCROLLBAR_PAD - SCROLLBAR_WIDTH;
        let (top, bottom) = (y + CONTENT_PAD, y + h - CONTENT_PAD);
        let track = (bottom - top).max(1.0);
        let thumb = 16f32.max((track * track / (track + max)).round());
        let travel = (track - thumb).max(1.0);
        let thumb_top = top + (self.scroll.clamp(0.0, max) / max * travel).round();
        Some((
            [left, top, SCROLLBAR_WIDTH, track],
            [left, thumb_top, SCROLLBAR_WIDTH, thumb],
        ))
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let sa = &env.settings.seed_atlas;
        let mut ws = vec![
            Widget::text(
                env.gui[0] / 2.0,
                8.0,
                "Seed Atlas Markers",
                WHITE,
                Align::Center,
            ),
            Widget::text(
                env.gui[0] / 2.0,
                23.0,
                if sa.structures {
                    "Structure rendering is enabled"
                } else {
                    "Structure rendering is disabled globally"
                },
                0xFFB8_B8B8,
                Align::Center,
            ),
        ];
        let (slider, size, reset) = Self::header(env);
        let pct = sa.completed_opacity_pct();
        ws.push(Widget::slider(
            "mk:completed",
            slider,
            format!(
                "Completed: {}",
                if pct == 0 {
                    "Hidden".to_owned()
                } else {
                    format!("{pct} %")
                }
            ),
            pct as f32 / 100.0,
        ));
        ws.push(Widget::button(
            "mk:size",
            size,
            format!("Marker size: {} %", sa.marker_size),
            true,
        ));
        let completed = env.seed_atlas.completed_count.unwrap_or(0);
        ws.push(Widget::button(
            "mk:reset",
            reset,
            format!("Reset completed ({completed})"),
            completed > 0,
        ));

        let [lx, ly, lw, lh] = Self::list_rect(env);
        ws.push(Widget {
            id: "mk:list".into(),
            rect: [lx, ly, lw, lh],
            text: String::new(),
            kind: ui::Kind::Panel { fill: 0xD012_1212 },
        });
        let (left, right) = (lx + CONTENT_PAD, Self::content_right(env));
        let (top, bottom) = (ly + CONTENT_PAD, ly + lh - CONTENT_PAD);
        let scroll = self.scroll.clamp(0.0, Self::max_scroll(env)).round();
        let mut y = top - scroll;
        for (index, m) in MARKERS.iter().enumerate() {
            let row_bottom = y + ROW_HEIGHT;
            if row_bottom >= top && y <= bottom {
                // Backgrounds are clipped to the list; a row's contents show
                // while it is wholly inside.
                let clip = |r: [f32; 4]| {
                    let (t, b) = (r[1].max(top), (r[1] + r[3]).min(bottom));
                    [r[0], t, r[2], (b - t).max(0.0)]
                };
                let hovered = env.mouse[0] >= left
                    && env.mouse[0] < right
                    && env.mouse[1] >= y.max(top)
                    && env.mouse[1] < row_bottom.min(bottom);
                let background = if hovered {
                    0x7047_4747
                } else if index % 2 == 0 {
                    0x301C_1C1C
                } else {
                    0x4024_2424
                };
                ws.push(Widget::fill(
                    clip([left, y, right - left, ROW_HEIGHT]),
                    background,
                ));
                let on = crate::seedatlas::marker_enabled(sa, m.id);
                ws.push(Widget::fill(
                    clip([left, y + 2.0, 2.0, ROW_HEIGHT - 4.0]),
                    if on { 0xFF55_AA55 } else { 0xFF55_5555 },
                ));
                if y >= top && row_bottom <= bottom {
                    ws.push(Widget::icon(
                        m.id,
                        [left + 6.0 + 1.0, y + 5.0, 18.0, 18.0],
                        false,
                    ));
                    let toggle_x = right - TOGGLE_WIDTH - 4.0;
                    let toggle_y = y + ((ROW_HEIGHT - 20.0) / 2.0).floor();
                    ws.push(Widget::button(
                        &format!("mk:on:{}", m.id),
                        [toggle_x, toggle_y, TOGGLE_WIDTH, 20.0],
                        on_off(on),
                        true,
                    ));
                    let count = env
                        .seed_atlas
                        .completed_by_kind
                        .get(m.id)
                        .copied()
                        .unwrap_or(0);
                    let reset_x = toggle_x - RESET_WIDTH - 4.0;
                    ws.push(Widget::button(
                        &format!("mk:rs:{}", m.id),
                        [reset_x, toggle_y, RESET_WIDTH, 20.0],
                        "",
                        count > 0,
                    ));
                    // The badge and the count on the reset button.
                    let text = count.to_string();
                    let content_w = BADGE_SIZE + 3.0 + 6.0 * text.len() as f32 - 1.0;
                    let badge_x = (reset_x + (RESET_WIDTH - content_w) / 2.0).floor();
                    let badge_y = (toggle_y + (20.0 - BADGE_SIZE) / 2.0).floor();
                    ws.push(Widget::icon(
                        "completed",
                        [badge_x, badge_y, BADGE_SIZE, BADGE_SIZE],
                        count == 0,
                    ));
                    ws.push(Widget::text(
                        badge_x + BADGE_SIZE + 3.0,
                        toggle_y + 6.0,
                        text,
                        if count > 0 { WHITE } else { GREY },
                        Align::Left,
                    ));
                    let name_x = left + 6.0 + ICON_SIZE + 8.0;
                    let available = (reset_x - 8.0 - name_x).max(0.0);
                    ws.push(Widget::text(
                        name_x,
                        y + 10.0,
                        clipped(m.name, available),
                        WHITE,
                        Align::Left,
                    ));
                }
            }
            y += ROW_HEIGHT + ROW_GAP;
        }
        if let Some((track, thumb)) = self.scrollbar(env) {
            ws.push(Widget::fill(track, 0xFF2A_2A2A));
            ws.push(Widget::fill(thumb, 0xFF8A_8A8A));
            ws.push(Widget::fill(
                [thumb[0], thumb[1], 1.0, thumb[3]],
                0xFFB0_B0B0,
            ));
        }
        let done_w = 200f32.min(120f32.max(env.gui[0] - 24.0));
        ws.push(done(env, done_w));
        ws
    }

    pub(crate) fn tooltip(&self, id: &str, _env: &Env<'_>) -> Option<String> {
        if id == "mk:completed" {
            return Some("Fades structures you marked as completed. 100 % draws them like any other marker, 0 % hides them.".into());
        }
        if id == "mk:reset" {
            return Some(
                "Marks every completed structure of this world or server as not completed.".into(),
            );
        }
        let kind = id.strip_prefix("mk:rs:")?;
        let name = MARKERS.iter().find(|m| m.id == kind)?.name;
        Some(format!("Reset completed {name}"))
    }

    fn set_completed(env: &mut Env<'_>, x: f32) {
        let (slider, _, _) = Self::header(env);
        let pct = (slider_value(slider, x) * 100.0).round() as u8;
        let sa = &mut env.settings.seed_atlas;
        if sa.completed_opacity_pct() != pct || sa.hide_completed {
            sa.hide_completed = false;
            sa.completed_opacity = pct;
            env.settings_changed = true;
        }
    }

    pub(crate) fn click(&mut self, id: Option<&str>, at: [f32; 2], env: &mut Env<'_>) -> Action {
        if let Some((track, _)) = self.scrollbar(env)
            && at[0] >= track[0] - 2.0
            && at[0] < track[0] + track[2] + SCROLLBAR_PAD - 1.0
            && at[1] >= track[1]
            && at[1] < track[1] + track[3]
        {
            self.dragging_scrollbar = true;
            self.scroll_to(at[1], env);
            return Action::None;
        }
        match id {
            Some("mk:completed") => {
                self.dragging_slider = true;
                Self::set_completed(env, at[0]);
            }
            Some("mk:size") => {
                let sa = &mut env.settings.seed_atlas;
                sa.marker_size = next(&MARKER_SIZE_STEPS, sa.marker_size, 150);
                env.settings_changed = true;
            }
            Some("mk:reset") => {
                return Action::Open(Screen::ConfirmReset(ConfirmResetScreen::new(env, None)));
            }
            Some("done") => {
                return Action::Open(Screen::SeedAtlasSettings(SeedAtlasSettingsScreen::new()));
            }
            Some(other) => {
                if let Some(kind) = other.strip_prefix("mk:on:") {
                    let on = crate::seedatlas::marker_enabled(&env.settings.seed_atlas, kind);
                    env.settings.seed_atlas.markers.insert(kind.to_owned(), !on);
                    env.settings_changed = true;
                } else if let Some(kind) = other.strip_prefix("mk:rs:") {
                    return Action::Open(Screen::ConfirmReset(ConfirmResetScreen::new(
                        env,
                        Some(kind),
                    )));
                }
            }
            None => {}
        }
        Action::None
    }

    fn scroll_to(&mut self, mouse_y: f32, env: &Env<'_>) {
        let Some((track, thumb)) = self.scrollbar(env) else {
            self.scroll = 0.0;
            return;
        };
        let max = Self::max_scroll(env);
        let travel = (track[3] - thumb[3]).max(1.0);
        let relative = (mouse_y - track[1] - thumb[3] / 2.0) / travel;
        self.scroll = (relative * max).clamp(0.0, max);
    }

    pub(crate) fn drag(&mut self, at: [f32; 2], env: &mut Env<'_>) {
        if self.dragging_scrollbar {
            self.scroll_to(at[1], env);
        } else if self.dragging_slider {
            Self::set_completed(env, at[0]);
        }
    }

    pub(crate) fn release(&mut self) {
        self.dragging_scrollbar = false;
        self.dragging_slider = false;
    }

    /// The mouse wheel, `lines` up positive (`mouseScrolled`: a row a notch).
    pub(crate) fn scroll(&mut self, lines: f32, max: f32) {
        self.scroll = (self.scroll - lines * ROW_HEIGHT).clamp(0.0, max);
    }

    pub(crate) fn max_scroll_for(env: &Env<'_>) -> f32 {
        Self::max_scroll(env)
    }
}

/// The name cut to `width` GUI pixels with "..." (the game's font is about
/// six pixels a character).
fn clipped(name: &str, width: f32) -> String {
    let fits = (width / 6.0).floor() as usize;
    if name.chars().count() <= fits {
        return name.to_owned();
    }
    let keep = fits.saturating_sub(3);
    format!("{}...", name.chars().take(keep).collect::<String>())
}

// --------------------------------------------------------- performance --

/// "Seed Atlas Performance".
#[derive(Debug, Default, PartialEq)]
pub struct PerformanceSettingsScreen;

fn resolution_label(r: u8) -> &'static str {
    match r {
        1 => "Full Detail (1x)",
        2 => "High (2x)",
        4 => "Reduced (4x)",
        8 => "Low (8x)",
        _ => "Very Low (16x)",
    }
}

fn prefetch_label(r: u8) -> &'static str {
    match r {
        0 => "Off",
        1 => "Near",
        2 => "Medium",
        _ => "Far",
    }
}

impl PerformanceSettingsScreen {
    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let sa = &env.settings.seed_atlas;
        let (width, left) = content(env);
        let workers = if sa.worker_threads == 0 {
            "Auto".to_owned()
        } else {
            sa.worker_threads.to_string()
        };
        vec![
            Widget::text(
                env.gui[0] / 2.0,
                14.0,
                "Seed Atlas Performance",
                WHITE,
                Align::Center,
            ),
            Widget::button(
                "pf:resolution",
                [left, 36.0, width, 20.0],
                format!(
                    "Biome resolution: {}",
                    resolution_label(sa.biome_resolution)
                ),
                true,
            ),
            Widget::button(
                "pf:workers",
                [left, 61.0, width, 20.0],
                format!("Worker threads: {workers}"),
                true,
            ),
            Widget::button(
                "pf:prefetch",
                [left, 86.0, width, 20.0],
                format!(
                    "Background prefetch: {}",
                    prefetch_label(sa.prefetch_radius)
                ),
                true,
            ),
            Widget::paragraph(
                [left, 118.0, width, 40.0],
                "Map detail adapts automatically while zooming. A larger minimum step renders fewer nearby biome samples. Background prefetch only runs while the world map is open.",
                GREY,
            ),
            done(env, 200.0),
        ]
    }

    pub(crate) fn click(&mut self, id: Option<&str>, env: &mut Env<'_>) -> Action {
        let sa = &mut env.settings.seed_atlas;
        match id {
            Some("pf:resolution") => {
                sa.biome_resolution = next(&BIOME_RESOLUTION_STEPS, sa.biome_resolution, 1)
            }
            Some("pf:workers") => {
                sa.worker_threads = next(&WORKER_THREAD_STEPS, sa.worker_threads, 0)
            }
            Some("pf:prefetch") => {
                sa.prefetch_radius = next(&PREFETCH_RADIUS_STEPS, sa.prefetch_radius, 1)
            }
            Some("done") => {
                return Action::Open(Screen::SeedAtlasSettings(SeedAtlasSettingsScreen::new()));
            }
            _ => return Action::None,
        }
        env.settings_changed = true;
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::screens_seedatlas::SeedAtlasEnv;
    use crate::waypoint::DimensionWaypoints;

    fn env<'a>(w: &'a mut DimensionWaypoints, s: &'a mut Settings) -> Env<'a> {
        Env {
            gui: [432.0, 245.0],
            mouse: [-1.0, -1.0],
            world_label: "Test",
            dimension_label: "overworld (auto)",
            player_block: [0, 64, 0],
            waypoints: w,
            settings: s,
            waypoints_changed: false,
            settings_changed: false,
            seed_atlas: SeedAtlasEnv::default(),
            profiles: vec!["default".into()],
            profile: "default".into(),
            minimap: None,
        }
    }

    fn find<'a>(ws: &'a [Widget], id: &str) -> &'a Widget {
        ws.iter()
            .find(|w| w.id == id)
            .unwrap_or_else(|| panic!("no widget {id}"))
    }

    #[test]
    fn main_screen_lays_out_like_the_fabric_version() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let e = env(&mut w, &mut s);
        let ws = SeedAtlasSettingsScreen::new().widgets(&e);
        // 340 wide, centred: left 46, halves of 167.
        assert_eq!(find(&ws, "sa:layer").rect, [46.0, 34.0, 167.0, 20.0]);
        assert_eq!(find(&ws, "sa:world").rect, [219.0, 34.0, 167.0, 20.0]);
        assert_eq!(find(&ws, "sa:opacity").rect, [46.0, 59.0, 167.0, 20.0]);
        assert_eq!(find(&ws, "sa:y1").rect, [161.0, 98.0, 109.0, 20.0]);
        assert_eq!(find(&ws, "sa:structures").text, "Structures: Off");
        assert_eq!(find(&ws, "sa:structure").rect, [276.0, 175.0, 110.0, 20.0]);
        assert_eq!(find(&ws, "done").rect, [116.0, 217.0, 200.0, 20.0]);
        assert_eq!(find(&ws, "sa:layer").text, "Seed map layer: On");
        assert_eq!(find(&ws, "sa:opacity").text, "Layer opacity: 100%");
    }

    #[test]
    fn heights_are_committed_when_leaving() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut sc = SeedAtlasSettingsScreen::new();
        sc.click(Some("sa:y0"), [0.0, 0.0], &mut e);
        for _ in 0..3 {
            sc.key("key.keyboard.backspace", &mut e);
        }
        for c in "80x".chars() {
            sc.typed(c);
        }
        assert_eq!(e.settings.seed_atlas.overworld_y, 255, "not before leaving");
        let a = sc.click(Some("sa:performance"), [0.0, 0.0], &mut e);
        assert!(matches!(a, Action::Open(Screen::SeedAtlasPerformance(_))));
        assert_eq!(e.settings.seed_atlas.overworld_y, 80);
    }

    #[test]
    fn opacity_slider_follows_the_mouse() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut sc = SeedAtlasSettingsScreen::new();
        let r = SeedAtlasSettingsScreen::opacity_rect(&e);
        sc.click(
            Some("sa:opacity"),
            [r[0] + 4.0 + (r[2] - 8.0) / 2.0, 60.0],
            &mut e,
        );
        assert_eq!(e.settings.seed_atlas.biome_opacity, 128);
        sc.drag([r[0], 60.0], &mut e);
        assert_eq!(e.settings.seed_atlas.biome_opacity, 0);
    }

    #[test]
    fn marker_list_toggles_and_scrolls() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut sc = MarkerSettingsScreen::default();
        let ws = sc.widgets(&e);
        let camp = find(&ws, "mk:on:camp");
        assert_eq!(camp.text, "On");
        assert_eq!(camp.rect[2], TOGGLE_WIDTH);
        sc.click(Some("mk:on:camp"), [0.0, 0.0], &mut e);
        assert!(!crate::seedatlas::marker_enabled(
            &e.settings.seed_atlas,
            "camp"
        ));
        // 28 kinds do not fit: the list scrolls a row a notch.
        let max = MarkerSettingsScreen::max_scroll_for(&e);
        assert!(max > 0.0);
        sc.scroll(-2.0, max);
        assert_eq!(sc.scroll, 56.0);
        assert!(
            !sc.widgets(&e).iter().any(|w| w.id == "mk:on:camp"),
            "scrolled out"
        );
        // Marker size cycles through the Fabric steps.
        sc.click(Some("mk:size"), [0.0, 0.0], &mut e);
        assert_eq!(e.settings.seed_atlas.marker_size, 200);
        assert!(matches!(
            sc.click(Some("done"), [0.0, 0.0], &mut e),
            Action::Open(Screen::SeedAtlasSettings(_))
        ));
    }

    #[test]
    fn performance_cycles_like_the_fabric_version() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut sc = PerformanceSettingsScreen;
        assert_eq!(
            find(&sc.widgets(&e), "pf:resolution").text,
            "Biome resolution: Full Detail (1x)"
        );
        sc.click(Some("pf:resolution"), &mut e);
        assert_eq!(
            find(&sc.widgets(&e), "pf:resolution").text,
            "Biome resolution: High (2x)"
        );
        assert_eq!(
            find(&sc.widgets(&e), "pf:workers").text,
            "Worker threads: Auto"
        );
    }
}

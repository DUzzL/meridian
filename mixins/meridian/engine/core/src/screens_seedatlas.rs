//! The Seed Atlas screens: go to the nearest biome or structure, biome
//! highlights, world generation (version and type) and the reset question
//! (`NearestScreen`, `BiomeHighlightScreen` with `BiomeToggleList`,
//! `WorldGenerationScreen` and `ConfirmResetScreen` of the Fabric version).
//!
//! They are laid out like the other screens in `screens.rs`, in GUI pixels
//! around the screen's centre, and answer an [`Action`] for what only the
//! host can do. Their parent is the Seed Atlas settings page
//! ([`SettingsScreen::seed_atlas`]).

use std::cell::Cell;

use meridian_seedatlas::{Dimension, biomes};

use crate::minimap::Align;
use crate::nearest::{NearestOptions, applies_to};
use crate::screens::{Action, Env, Screen};
use crate::seedatlas::MARKERS;
use crate::seedatlas_state::{GameVersion, game_version, set_game_version, world_type_label};
use crate::ui::{self, Widget};

/// What the Seed Atlas screens need to know about the game besides
/// [`Env`]'s settings (the version of the open world is
/// `settings.seed_atlas.context_key`).
#[derive(Clone, Debug)]
pub struct SeedAtlasEnv {
    /// The player's dimension (the structure list shows only the kinds
    /// that exist there).
    pub dimension: Dimension,
    /// Whether the open world or server has a seed. The biome lists come
    /// from the generation engine and are empty without one.
    pub has_seed: bool,
    /// How many structures of this world or server are completed; `None`
    /// when unknown (no world open).
    pub completed_count: Option<usize>,
    /// The choices of the nearest screens, remembered for the session by
    /// the host and handed to each new nearest screen.
    pub nearest: NearestOptions,
    /// Completed structures by marker id.
    pub completed_by_kind: std::collections::BTreeMap<String, usize>,
    /// "Active seed: …" (with the Nether's and End's own seeds), or that
    /// there is none.
    pub seed_status: String,
}

impl Default for SeedAtlasEnv {
    fn default() -> Self {
        Self {
            dimension: Dimension::Overworld,
            has_seed: false,
            completed_count: None,
            nearest: NearestOptions::default(),
            completed_by_kind: Default::default(),
            seed_status: "No seed configured for this world/server".into(),
        }
    }
}

/// What a nearest search looks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NearestKind {
    /// A biome by its engine id, with its shown name (the waypoint's name).
    Biome { id: i32, name: String },
    /// A marker kind by its `MarkerKind::id`, with its shown name.
    Structure { marker: &'static str, name: String },
}

/// How a nearest search ended, for [`NearestScreen::finish`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NearestOutcome {
    /// Found; the host opened the map there.
    Found,
    /// Nothing within reach.
    NotFound,
    /// Found, but the world map could not be opened.
    NoMap,
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn parent() -> Action {
    Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::SeedAtlas)))
}

/// The biomes of the world's version (nothing without a seed), by name.
fn biome_entries(env: &Env<'_>) -> Vec<Entry> {
    if !env.seed_atlas.has_seed {
        return Vec::new();
    }
    let mc = game_version(
        &env.settings.seed_atlas,
        &env.settings.seed_atlas.context_key,
    )
    .0
    .mc();
    let mut all: Vec<Entry> = (0..256)
        .filter(|&id| biomes::biome_exists(mc, id))
        .filter_map(|id| {
            let raw = biomes::name_in(mc, id)?;
            Some(Entry {
                id: id.to_string(),
                name: display_name(raw),
                alias: raw.to_owned(),
                swatch: Some(biomes::color_in(mc, id)),
            })
        })
        .collect();
    all.sort_by_key(|e| e.name.to_lowercase());
    all
}

// ------------------------------------------------------------------ list --

/// One row of a list: an id for its button, its name, and a colour swatch.
#[derive(Clone, Debug)]
struct Entry {
    id: String,
    name: String,
    /// The engine's name, also matched by the search.
    alias: String,
    swatch: Option<u32>,
}

/// "snowy_plains" as "Snowy Plains".
fn display_name(raw: &str) -> String {
    raw.split('_')
        .map(|word| {
            let mut c = word.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const ROW_H: f32 = 22.0;
const ROW_PITCH: f32 = 24.0;
const ROW_BUTTON_W: f32 = 56.0;
const LIST_TOP: f32 = 92.0;

/// A searchable, scrolling list with one button per row.
#[derive(Debug, Default, PartialEq)]
struct ListView {
    filter: String,
    filter_focused: bool,
    scroll: usize,
    /// The furthest scroll of the last layout (the host scrolls blind).
    max_scroll: Cell<usize>,
}

impl ListView {
    fn width(env: &Env<'_>) -> f32 {
        (env.gui[0] - 8.0).clamp(1.0, 420.0)
    }

    fn left(env: &Env<'_>) -> f32 {
        env.center()[0] - (Self::width(env) / 2.0).floor()
    }

    fn bottom(env: &Env<'_>, top: f32) -> f32 {
        (env.gui[1] - 38.0).max(top + 48.0).min(env.gui[1] - 26.0)
    }

    fn visible(env: &Env<'_>, top: f32) -> usize {
        (((Self::bottom(env, top) - top - 2.0) / ROW_PITCH).floor() as usize).max(1)
    }

    fn filtered<'a>(&self, entries: &'a [Entry]) -> Vec<&'a Entry> {
        let q = self.filter.trim().to_lowercase();
        entries
            .iter()
            .filter(|e| {
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.alias.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// The rows from `top` down; `cell` answers a row's button label, its
    /// enabled state and whether the row is highlighted.
    fn widgets(
        &self,
        env: &Env<'_>,
        top: f32,
        entries: &[Entry],
        cell: impl Fn(&Entry) -> (String, bool, bool),
    ) -> Vec<Widget> {
        let rows = self.filtered(entries);
        let visible = Self::visible(env, top);
        let max = rows.len().saturating_sub(visible);
        self.max_scroll.set(max);
        let (left, width) = (Self::left(env), Self::width(env));
        let mut ws = vec![
            Widget::separator(top - 2.0, env.gui[0]),
            Widget::separator(Self::bottom(env, top), env.gui[0]),
        ];
        for (i, e) in rows
            .into_iter()
            .skip(self.scroll.min(max))
            .take(visible)
            .enumerate()
        {
            let y = top + i as f32 * ROW_PITCH;
            let (label, enabled, selected) = cell(e);
            ws.push(Widget::row(
                &format!("row:{}", e.id),
                [left, y, width, ROW_H],
                selected,
            ));
            let mut text_x = left + 8.0;
            if let Some(color) = e.swatch {
                ws.push(Widget {
                    id: String::new(),
                    rect: [left + 5.0, y + 5.0, 12.0, 12.0],
                    text: String::new(),
                    kind: ui::Kind::Mark { color },
                });
                text_x = left + 24.0;
            }
            ws.push(Widget {
                id: String::new(),
                rect: [text_x, y + 7.0, 0.0, 9.0],
                text: e.name.clone(),
                kind: ui::Kind::Label {
                    color: 0xFFFF_FFFF,
                    align: Align::Left,
                },
            });
            ws.push(Widget::button(
                &format!("btn:{}", e.id),
                [
                    left + width - ROW_BUTTON_W - 4.0,
                    y + 1.0,
                    ROW_BUTTON_W,
                    20.0,
                ],
                label,
                enabled,
            ));
        }
        ws
    }

    fn search(&self, env: &Env<'_>, y: f32, placeholder: &str) -> Widget {
        Widget::field(
            "search",
            [Self::left(env), y, Self::width(env), ui::BUTTON_H],
            &self.filter,
            placeholder,
            self.filter_focused,
        )
    }

    /// The done button at the bottom, as the Fabric screens place it.
    fn done(env: &Env<'_>) -> Widget {
        let w = (env.gui[0] - 24.0).clamp(120.0, 200.0);
        Widget::button(
            "done",
            [
                (env.center()[0] - (w / 2.0).floor()).floor(),
                (env.gui[1] - 28.0).max(0.0),
                w,
                ui::BUTTON_H,
            ],
            "Done",
            true,
        )
    }

    fn scroll_by(&mut self, lines: f32) {
        self.scroll = ((self.scroll as f32 - lines).max(0.0) as usize).min(self.max_scroll.get());
    }

    fn typed(&mut self, c: char) {
        if self.filter_focused && ui::type_char(&mut self.filter, c, 24, |_| true) {
            self.scroll = 0;
        }
    }

    fn key(&mut self, key: &str) {
        match key {
            "key.keyboard.backspace" if self.filter_focused => {
                ui::backspace(&mut self.filter);
                self.scroll = 0;
            }
            "key.keyboard.up" if !self.filter_focused => self.scroll_by(1.0),
            "key.keyboard.down" if !self.filter_focused => self.scroll_by(-1.0),
            "key.keyboard.page_up" if !self.filter_focused => self.scroll_by(5.0),
            "key.keyboard.page_down" if !self.filter_focused => self.scroll_by(-5.0),
            _ => {}
        }
    }
}

// --------------------------------------------------------------- nearest --

/// "Go to Nearest Biome" / "Go to Nearest Structure".
#[derive(Debug, PartialEq)]
pub struct NearestScreen {
    biome_mode: bool,
    /// The waypoint mode and the skip switch; the host keeps them for the
    /// session (they also ride along in [`Action::FindNearest`]).
    pub options: NearestOptions,
    /// A search is running: the buttons are inactive.
    pub busy: bool,
    /// The line under the title.
    pub status: String,
    searching: String,
    list: ListView,
}

impl NearestScreen {
    /// The biome mode.
    pub fn biome(env: &Env<'_>) -> Self {
        Self::new(true, env)
    }

    /// The structure mode.
    pub fn structure(env: &Env<'_>) -> Self {
        Self::new(false, env)
    }

    fn new(biome_mode: bool, env: &Env<'_>) -> Self {
        Self {
            biome_mode,
            options: env.seed_atlas.nearest,
            busy: false,
            status: String::new(),
            searching: String::new(),
            list: ListView::default(),
        }
    }

    /// Whether this is the biome list (else the structure list).
    pub fn is_biome(&self) -> bool {
        self.biome_mode
    }

    /// The search ended (the host calls this when its thread is done).
    pub fn finish(&mut self, outcome: NearestOutcome) {
        self.busy = false;
        self.status = match outcome {
            NearestOutcome::Found => String::new(),
            NearestOutcome::NotFound => format!(
                "{} was not found near you (structures up to 16k blocks away, biomes up to 32k).",
                self.searching
            ),
            NearestOutcome::NoMap => "The world map is not available.".into(),
        };
    }

    fn entries(&self, env: &Env<'_>) -> Vec<Entry> {
        if self.biome_mode {
            return biome_entries(env);
        }
        if !env.seed_atlas.has_seed {
            return Vec::new();
        }
        MARKERS
            .iter()
            .filter(|m| applies_to(m, env.seed_atlas.dimension))
            .map(|m| Entry {
                id: m.id.to_owned(),
                name: m.name.to_owned(),
                alias: m.id.to_owned(),
                swatch: None,
            })
            .collect()
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, _] = env.center();
        let (left, width) = (ListView::left(env), ListView::width(env));
        let half = ((width - 6.0) / 2.0).floor();
        let title = if self.biome_mode {
            "Go to Nearest Biome"
        } else {
            "Go to Nearest Structure"
        };
        let mut ws = vec![Widget::label(cx, 8.0, title, Align::Center)];
        if !self.status.is_empty() {
            ws.push(Widget {
                id: String::new(),
                rect: [cx, 24.0, 0.0, 9.0],
                text: self.status.clone(),
                kind: ui::Kind::Label {
                    color: 0xFFB8_B8B8,
                    align: Align::Center,
                },
            });
        }
        ws.push(Widget::button(
            "waypoint",
            [left, 38.0, if self.biome_mode { width } else { half }, 20.0],
            self.options.waypoint_label(),
            true,
        ));
        if !self.biome_mode {
            ws.push(Widget::button(
                "skip",
                [left + half + 6.0, 38.0, width - half - 6.0, 20.0],
                self.options.skip_label(),
                true,
            ));
        }
        if env.seed_atlas.has_seed {
            let placeholder = if self.biome_mode {
                "Search biomes..."
            } else {
                "Search structures..."
            };
            ws.push(self.list.search(env, 64.0, placeholder));
            let entries = self.entries(env);
            let busy = self.busy;
            ws.extend(
                self.list
                    .widgets(env, LIST_TOP, &entries, |_| ("Find".into(), !busy, false)),
            );
        } else {
            ws.push(Widget {
                id: String::new(),
                rect: [cx, 100.0, 0.0, 9.0],
                text: "No seed configured for this world/server".into(),
                kind: ui::Kind::Label {
                    color: 0xFFB8_B8B8,
                    align: Align::Center,
                },
            });
        }
        ws.push(ListView::done(env));
        ws
    }

    pub(crate) fn click(&mut self, id: Option<&str>, env: &mut Env<'_>) -> Action {
        self.list.filter_focused = false;
        match id {
            Some("search") => self.list.filter_focused = true,
            Some("waypoint") => self.options.waypoint = self.options.waypoint.next(),
            Some("skip") => self.options.skip_completed = !self.options.skip_completed,
            Some("done") => return parent(),
            Some(b) if b.starts_with("btn:") && !self.busy => {
                let Some(entry) = self.entries(env).into_iter().find(|e| e.id == b[4..]) else {
                    return Action::None;
                };
                let kind = if self.biome_mode {
                    let Ok(id) = entry.id.parse() else {
                        return Action::None;
                    };
                    NearestKind::Biome {
                        id,
                        name: entry.name.clone(),
                    }
                } else {
                    let Some(m) = MARKERS.iter().find(|m| m.id == entry.id) else {
                        return Action::None;
                    };
                    NearestKind::Structure {
                        marker: m.id,
                        name: entry.name.clone(),
                    }
                };
                self.busy = true;
                self.status = format!("Searching for {}...", entry.name);
                self.searching = entry.name;
                return Action::FindNearest {
                    kind,
                    waypoint: self.options.waypoint,
                    skip_completed: !self.biome_mode && self.options.skip_completed,
                };
            }
            _ => {}
        }
        Action::None
    }

    pub(crate) fn typed(&mut self, c: char) {
        self.list.typed(c);
    }

    pub(crate) fn key(&mut self, key: &str) -> Action {
        self.list.key(key);
        Action::None
    }

    pub(crate) fn scroll(&mut self, lines: f32) {
        self.list.scroll_by(lines);
    }
}

// ------------------------------------------------------------- highlight --

/// "Biome Highlights": the switch, the selection and the biome list.
#[derive(Debug, Default, PartialEq)]
pub struct BiomeHighlightScreen {
    list: ListView,
}

impl BiomeHighlightScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, _] = env.center();
        let (left, width) = (ListView::left(env), ListView::width(env));
        let half = ((width - 6.0) / 2.0).floor();
        let sa = &env.settings.seed_atlas;
        let entries = biome_entries(env);
        let count = sa.highlighted_biomes.len();
        let status = if sa.biome_highlight {
            format!("{count} biomes highlighted")
        } else {
            format!("Highlighting off \u{b7} {count} biomes selected")
        };
        let mut ws = vec![
            Widget::label(cx, 8.0, "Biome Highlights", Align::Center),
            Widget {
                id: String::new(),
                rect: [cx, 23.0, 0.0, 9.0],
                text: status,
                kind: ui::Kind::Label {
                    color: 0xFFB8_B8B8,
                    align: Align::Center,
                },
            },
            Widget::button(
                "highlight",
                [left, 36.0, half, 20.0],
                format!("Biome highlighting: {}", on_off(sa.biome_highlight)),
                !entries.is_empty(),
            ),
            Widget::button(
                "clear",
                [left + half + 6.0, 36.0, width - half - 6.0, 20.0],
                "Clear selection",
                count > 0,
            ),
            self.list.search(env, 62.0, "Search biomes..."),
        ];
        if entries.is_empty() {
            ws.push(Widget {
                id: String::new(),
                rect: [cx, LIST_TOP + 16.0, 0.0, 9.0],
                text: "Set a seed first: the biome list comes from the generation engine.".into(),
                kind: ui::Kind::Label {
                    color: 0xFFB8_B8B8,
                    align: Align::Center,
                },
            });
        }
        ws.extend(self.list.widgets(env, LIST_TOP, &entries, |e| {
            let on =
                e.id.parse::<u8>()
                    .is_ok_and(|id| sa.highlighted_biomes.contains(&id));
            (on_off(on).into(), true, on)
        }));
        ws.push(ListView::done(env));
        ws
    }

    pub(crate) fn click(&mut self, id: Option<&str>, env: &mut Env<'_>) -> Action {
        self.list.filter_focused = false;
        match id {
            Some("search") => self.list.filter_focused = true,
            Some("highlight") => {
                let sa = &mut env.settings.seed_atlas;
                sa.biome_highlight = !sa.biome_highlight;
                env.settings_changed = true;
            }
            Some("clear") => {
                env.settings.seed_atlas.highlighted_biomes.clear();
                env.settings_changed = true;
            }
            Some("done") => return parent(),
            Some(b) if b.starts_with("btn:") => {
                if let Ok(id) = b[4..].parse::<u8>() {
                    let set = &mut env.settings.seed_atlas.highlighted_biomes;
                    if !set.remove(&id) {
                        set.insert(id);
                    }
                    env.settings_changed = true;
                }
            }
            _ => {}
        }
        Action::None
    }

    pub(crate) fn typed(&mut self, c: char) {
        self.list.typed(c);
    }

    pub(crate) fn key(&mut self, key: &str) -> Action {
        self.list.key(key);
        Action::None
    }

    pub(crate) fn scroll(&mut self, lines: f32) {
        self.list.scroll_by(lines);
    }
}

// ------------------------------------------------------ world generation --

const VERSION_ROWS: usize = 6;
const VERSIONS_PER_PAGE: usize = VERSION_ROWS * 2;

/// "World Generation": the Minecraft version of the open world or server
/// (Auto and every version with world generation changes) and the world
/// type.
#[derive(Debug, Default, PartialEq)]
pub struct WorldGenerationScreen {
    pub page: usize,
}

impl WorldGenerationScreen {
    /// Opens on the page holding the selected version.
    pub fn new(env: &Env<'_>) -> Self {
        let sa = &env.settings.seed_atlas;
        let (version, auto) = game_version(sa, &sa.context_key);
        let index = if auto {
            0
        } else {
            1 + GameVersion::ALL
                .iter()
                .position(|v| *v == version)
                .unwrap_or(0)
        };
        Self {
            page: index / VERSIONS_PER_PAGE,
        }
    }

    fn pages() -> usize {
        (GameVersion::ALL.len() + 1).div_ceil(VERSIONS_PER_PAGE)
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, c] = env.center();
        let sa = &env.settings.seed_atlas;
        let has_context = !sa.context_key.is_empty();
        let (selected, auto) = game_version(sa, &sa.context_key);
        let heading = if has_context {
            let name = crate::seedatlas_state::game_version_name(sa, &sa.context_key);
            format!("Minecraft version of this world or server: {name}")
        } else {
            "Join a world or server to choose its version".into()
        };
        let page = self.page.min(Self::pages() - 1);
        let mut ws = vec![
            Widget::label(
                cx,
                c - 114.0,
                format!("World Generation ({}/{})", page + 1, Self::pages()),
                Align::Center,
            ),
            Widget {
                id: String::new(),
                rect: [cx, c - 100.0, 0.0, 9.0],
                text: heading,
                kind: ui::Kind::Label {
                    color: 0xFFA0_A0A0,
                    align: Align::Center,
                },
            },
            Widget::button(
                "type",
                [cx - 205.0, c - 86.0, 410.0, 20.0],
                format!("World type: {}", world_type_label(sa.large_biomes)),
                true,
            ),
        ];
        for slot in 0..VERSIONS_PER_PAGE {
            let index = page * VERSIONS_PER_PAGE + slot;
            let Some(version) = (index <= GameVersion::ALL.len())
                .then(|| index.checked_sub(1).map(|i| GameVersion::ALL[i]))
            else {
                break;
            };
            let x = cx - 205.0 + (slot % 2) as f32 * 210.0;
            let y = c - 58.0 + (slot / 2) as f32 * 24.0;
            let is_selected = match version {
                None => auto,
                Some(v) => !auto && v == selected,
            };
            let label = match version {
                None => format!("Auto ({})", GameVersion::NEWEST.name),
                Some(v) => v.label(),
            };
            let color = if !has_context {
                0xFF6A_6A6A
            } else if is_selected {
                0xFF7C_E07C
            } else {
                0xFFFF_FFFF
            };
            ws.push(Widget::row(
                &format!("ver:{index}"),
                [x, y, 200.0, 20.0],
                is_selected,
            ));
            ws.push(Widget {
                id: String::new(),
                rect: [x + 8.0, y + 6.0, 0.0, 9.0],
                text: label,
                kind: ui::Kind::Label {
                    color,
                    align: Align::Left,
                },
            });
            if version.is_some_and(|v| !v.has_terrain()) {
                ws.push(Widget {
                    id: String::new(),
                    rect: [x + 194.0, y + 6.0, 0.0, 9.0],
                    text: "no terrain".into(),
                    kind: ui::Kind::Label {
                        color: 0xFF8A_8A8A,
                        align: Align::Right,
                    },
                });
            }
        }
        ws.push(Widget::button(
            "prev",
            [cx - 205.0, c + 86.0, 74.0, ui::BUTTON_H],
            "<< Prev",
            page > 0,
        ));
        ws.push(Widget::button(
            "done",
            [cx - 100.0, c + 86.0, 200.0, ui::BUTTON_H],
            "Done",
            true,
        ));
        ws.push(Widget::button(
            "next",
            [cx + 131.0, c + 86.0, 74.0, ui::BUTTON_H],
            "Next >>",
            page + 1 < Self::pages(),
        ));
        ws.push(Widget {
            id: String::new(),
            rect: [cx, c + 110.0, 0.0, 9.0],
            text: "Only versions whose world generation differs. Terrain look from 1.18.".into(),
            kind: ui::Kind::Label {
                color: 0xFFA0_A0A0,
                align: Align::Center,
            },
        });
        ws
    }

    pub(crate) fn click(&mut self, id: Option<&str>, env: &mut Env<'_>) -> Action {
        match id {
            Some("type") => {
                let sa = &mut env.settings.seed_atlas;
                sa.large_biomes = !sa.large_biomes;
                env.settings_changed = true;
            }
            Some("prev") => self.page = self.page.saturating_sub(1),
            Some("next") => self.page = (self.page + 1).min(Self::pages() - 1),
            Some("done") => return parent(),
            Some(v) if v.starts_with("ver:") => {
                let sa = &mut env.settings.seed_atlas;
                let key = sa.context_key.clone();
                if let Ok(index) = v[4..].parse::<usize>()
                    && index <= GameVersion::ALL.len()
                {
                    let version = index.checked_sub(1).map(|i| GameVersion::ALL[i]);
                    // The choice belongs to the open world or server only.
                    if set_game_version(sa, &key, version) {
                        env.settings_changed = true;
                    }
                }
            }
            _ => {}
        }
        Action::None
    }
}

// --------------------------------------------------------- confirm reset --

/// "Reset completed structures?": yes resets, no goes back.
#[derive(Debug, PartialEq)]
pub struct ConfirmResetScreen {
    title: String,
    message: String,
    /// The marker id reset, or every kind.
    kind: Option<String>,
}

impl ConfirmResetScreen {
    /// The question for resetting the completed structures of one kind (a
    /// marker id), or of every kind, of the open world or server.
    pub fn new(env: &Env<'_>, kind: Option<&str>) -> Self {
        let message = match kind {
            None => format!(
                "All {} completed structures of {} become not completed again.",
                env.seed_atlas.completed_count.unwrap_or(0),
                env.world_label
            ),
            Some(id) => format!(
                "{} completed of type {} in {} become not completed again.",
                env.seed_atlas
                    .completed_by_kind
                    .get(id)
                    .copied()
                    .unwrap_or(0),
                crate::seedatlas::MARKERS
                    .iter()
                    .find(|m| m.id == id)
                    .map_or(id, |m| m.name),
                env.world_label
            ),
        };
        Self {
            title: "Reset completed structures?".into(),
            message,
            kind: kind.map(str::to_owned),
        }
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, c] = env.center();
        vec![
            Widget::label(cx, c - 30.0, self.title.clone(), Align::Center),
            Widget {
                id: String::new(),
                rect: [cx, c - 14.0, 0.0, 9.0],
                text: self.message.clone(),
                kind: ui::Kind::Label {
                    color: 0xFFB8_B8B8,
                    align: Align::Center,
                },
            },
            Widget::button("yes", [cx - 154.0, c + 10.0, 150.0, 20.0], "Yes", true),
            Widget::button("no", [cx + 4.0, c + 10.0, 150.0, 20.0], "No", true),
        ]
    }

    pub(crate) fn click(&mut self, id: Option<&str>) -> Action {
        match id {
            Some("yes") => Action::ResetCompleted(self.kind.clone()),
            Some("no") => Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::Structures))),
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Settings;
    use crate::nearest::WaypointMode;
    use crate::waypoint::DimensionWaypoints;

    fn env<'a>(w: &'a mut DimensionWaypoints, s: &'a mut Settings) -> Env<'a> {
        Env {
            gui: [432.0, 246.0],
            mouse: [-1.0, -1.0],
            world_label: "My World",
            dimension_label: "overworld (auto)",
            player_block: [0, 64, 0],
            waypoints: w,
            settings: s,
            waypoints_changed: false,
            settings_changed: false,
            seed_atlas: SeedAtlasEnv {
                has_seed: true,
                completed_count: Some(3),
                ..SeedAtlasEnv::default()
            },
            profiles: vec!["default".into()],
            profile: "default".into(),
            minimap: None,
        }
    }

    fn center_of(ws: &[Widget], id: &str) -> [f32; 2] {
        let w = ws
            .iter()
            .find(|w| w.id == id)
            .unwrap_or_else(|| panic!("no widget {id}"));
        [w.rect[0] + w.rect[2] / 2.0, w.rect[1] + w.rect[3] / 2.0]
    }

    fn click(screen: &mut Screen, id: &str, e: &mut Env<'_>) -> Action {
        let at = center_of(&screen.widgets(e), id);
        screen.click(at, false, e)
    }

    fn type_text(screen: &mut Screen, text: &str, e: &mut Env<'_>) {
        for c in text.chars() {
            screen.typed(c, e);
        }
    }

    fn within_gui(ws: &[Widget], gui: [f32; 2]) {
        for w in ws {
            if matches!(w.kind, ui::Kind::Separator) {
                continue;
            }
            let [x, y, width, height] = w.rect;
            assert!(
                x >= 0.0 && y >= 0.0 && x + width <= gui[0] && y + height <= gui[1],
                "{:?} outside {gui:?}",
                w
            );
        }
    }

    fn names(ws: &[Widget]) -> Vec<String> {
        ws.iter()
            .filter(|w| w.id.is_empty() && matches!(w.kind, ui::Kind::Label { .. }))
            .map(|w| w.text.clone())
            .collect()
    }

    #[test]
    fn nearest_biome_lists_biomes_and_finds_one() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::Nearest(NearestScreen::biome(&e));
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(names(&ws).contains(&"Go to Nearest Biome".to_owned()));
        assert!(ws.iter().any(|w| w.text == "Waypoint: Temporary"));
        assert!(!ws.iter().any(|w| w.id == "skip"), "no skip in biome mode");
        // Search narrows to the plains.
        click(&mut screen, "search", &mut e);
        type_text(&mut screen, "plains", &mut e);
        let ws = screen.widgets(&e);
        assert!(names(&ws).iter().any(|n| n == "Plains"), "{:?}", names(&ws));
        assert!(
            names(&ws)
                .iter()
                .all(|n| { n == "Go to Nearest Biome" || n.to_lowercase().contains("plains") })
        );
        // The waypoint option cycles through the three modes.
        click(&mut screen, "waypoint", &mut e);
        click(&mut screen, "waypoint", &mut e);
        assert!(
            screen
                .widgets(&e)
                .iter()
                .any(|w| w.text == "Waypoint: Permanent (Disabled)")
        );
        let Action::FindNearest {
            kind,
            waypoint,
            skip_completed,
        } = click(&mut screen, "btn:1", &mut e)
        else {
            panic!("no search")
        };
        assert_eq!(
            kind,
            NearestKind::Biome {
                id: 1,
                name: "Plains".into()
            }
        );
        assert_eq!(waypoint, WaypointMode::Disabled);
        assert!(!skip_completed);
        // Busy: the buttons are inactive until the host is done.
        assert_eq!(click(&mut screen, "btn:1", &mut e), Action::None);
        let Screen::Nearest(n) = &mut screen else {
            unreachable!()
        };
        assert!(n.busy && n.status == "Searching for Plains...");
        n.finish(NearestOutcome::NotFound);
        assert!(n.status.starts_with("Plains was not found near you"));
        assert!(!n.busy);
    }

    #[test]
    fn nearest_structure_lists_the_dimensions_kinds() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        e.seed_atlas.nearest.skip_completed = true;
        let mut screen = Screen::Nearest(NearestScreen::structure(&e));
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(ws.iter().any(|w| w.text == "Skip completed: On"));
        click(&mut screen, "skip", &mut e);
        assert!(
            screen
                .widgets(&e)
                .iter()
                .any(|w| w.text == "Skip completed: Off")
        );
        click(&mut screen, "skip", &mut e);
        // Overworld: villages yes, fortresses no.
        click(&mut screen, "search", &mut e);
        type_text(&mut screen, "fort", &mut e);
        assert!(!screen.widgets(&e).iter().any(|w| w.id.starts_with("btn:")));
        e.seed_atlas.dimension = Dimension::Nether;
        assert!(screen.widgets(&e).iter().any(|w| w.id == "btn:fortress"));
        let Action::FindNearest {
            kind,
            skip_completed,
            ..
        } = click(&mut screen, "btn:fortress", &mut e)
        else {
            panic!("no search")
        };
        assert_eq!(
            kind,
            NearestKind::Structure {
                marker: "fortress",
                name: "Nether Fortress".into()
            }
        );
        assert!(skip_completed);
    }

    #[test]
    fn nearest_without_a_seed_says_so() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        e.seed_atlas.has_seed = false;
        let mut screen = Screen::Nearest(NearestScreen::biome(&e));
        let ws = screen.widgets(&e);
        assert!(
            ws.iter()
                .any(|w| w.text == "No seed configured for this world/server")
        );
        assert!(!ws.iter().any(|w| w.id == "search"));
        assert_eq!(
            click(&mut screen, "done", &mut e),
            Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::SeedAtlas)))
        );
    }

    #[test]
    fn nearest_list_scrolls_within_bounds() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let e = env(&mut w, &mut s);
        let mut screen = Screen::Nearest(NearestScreen::biome(&e));
        let first = screen.widgets(&e);
        let first_row = first
            .iter()
            .find(|w| w.id.starts_with("btn:"))
            .unwrap()
            .id
            .clone();
        screen.scroll(-3.0, &e);
        let moved = screen.widgets(&e);
        assert_ne!(
            moved.iter().find(|w| w.id.starts_with("btn:")).unwrap().id,
            first_row
        );
        within_gui(&moved, e.gui);
        // Scrolling far past the end stays on the last row.
        screen.scroll(-10_000.0, &e);
        let end = screen.widgets(&e);
        assert!(end.iter().any(|w| w.id.starts_with("btn:")));
        screen.scroll(1.0, &e);
        screen.scroll(1.0, &e);
        assert!(screen.widgets(&e).iter().any(|w| w.id.starts_with("btn:")));
    }

    #[test]
    fn biome_highlights_toggle_and_clear() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::BiomeHighlight(BiomeHighlightScreen::new());
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(ws.iter().any(|w| w.text == "Biome highlighting: Off"));
        assert!(
            ws.iter()
                .any(|w| w.text == "Highlighting off \u{b7} 0 biomes selected")
        );
        assert!(
            ws.iter()
                .any(|w| w.id == "clear" && w.kind == ui::Kind::Button { enabled: false })
        );
        click(&mut screen, "highlight", &mut e);
        assert!(e.settings.seed_atlas.biome_highlight && e.settings_changed);
        // Pick the desert via the search.
        click(&mut screen, "search", &mut e);
        type_text(&mut screen, "desert", &mut e);
        assert!(screen.widgets(&e).iter().any(|w| w.id == "btn:2"));
        click(&mut screen, "btn:2", &mut e);
        assert!(e.settings.seed_atlas.highlighted_biomes.contains(&2));
        let ws = screen.widgets(&e);
        assert!(ws.iter().any(|w| w.text == "1 biomes highlighted"));
        assert!(ws.iter().any(|w| w.id == "btn:2" && w.text == "On"));
        click(&mut screen, "btn:2", &mut e);
        assert!(e.settings.seed_atlas.highlighted_biomes.is_empty());
        click(&mut screen, "btn:2", &mut e);
        click(&mut screen, "clear", &mut e);
        assert!(e.settings.seed_atlas.highlighted_biomes.is_empty());
        click(&mut screen, "highlight", &mut e);
        assert!(!e.settings.seed_atlas.biome_highlight);
        assert_eq!(
            click(&mut screen, "done", &mut e),
            Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::SeedAtlas)))
        );
    }

    #[test]
    fn biome_highlights_without_a_seed_are_inactive() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        e.seed_atlas.has_seed = false;
        let screen = Screen::BiomeHighlight(BiomeHighlightScreen::new());
        let ws = screen.widgets(&e);
        assert!(
            ws.iter()
                .any(|w| w.id == "highlight" && w.kind == ui::Kind::Button { enabled: false })
        );
        assert!(ws.iter().any(|w| w.text.starts_with("Set a seed first")));
        assert!(!ws.iter().any(|w| w.id.starts_with("btn:")));
    }

    #[test]
    fn world_generation_picks_version_and_type() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        s.seed_atlas.context_key = "sp-w".into();
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::WorldGeneration(WorldGenerationScreen::new(&e));
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(ws.iter().any(|w| w.text == "World Generation (1/3)"));
        assert!(
            ws.iter()
                .any(|w| w.text == "Minecraft version of this world or server: Auto (26.3)")
        );
        assert!(ws.iter().any(|w| w.text == "World type: Normal"));
        assert!(ws.iter().any(|w| w.text.starts_with("Only versions whose")));
        click(&mut screen, "type", &mut e);
        assert!(e.settings.seed_atlas.large_biomes);
        // Index 2 is 26.2; the choice is stored for the open world only.
        click(&mut screen, "ver:2", &mut e);
        assert_eq!(e.settings.seed_atlas.game_versions["sp-w"], "26.2");
        assert!(e.settings_changed);
        click(&mut screen, "ver:0", &mut e);
        assert!(e.settings.seed_atlas.game_versions.is_empty());
        // Next page: older versions, "no terrain" hints from before 1.18.
        click(&mut screen, "next", &mut e);
        click(&mut screen, "next", &mut e);
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(ws.iter().any(|w| w.text == "World Generation (3/3)"));
        assert!(ws.iter().any(|w| w.text == "no terrain"));
        assert!(ws.iter().any(|w| w.id == "ver:25"));
        assert!(!ws.iter().any(|w| w.id == "ver:26"));
        assert!(
            ws.iter()
                .any(|w| w.id == "next" && w.kind == ui::Kind::Button { enabled: false })
        );
        click(&mut screen, "ver:25", &mut e);
        assert_eq!(e.settings.seed_atlas.game_versions["sp-w"], "1.1");
        // Opens on the selected version's page.
        let again = WorldGenerationScreen::new(&e);
        assert_eq!(again.page, 2);
        assert_eq!(
            click(&mut screen, "done", &mut e),
            Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::SeedAtlas)))
        );
    }

    #[test]
    fn world_generation_needs_a_world() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::WorldGeneration(WorldGenerationScreen::new(&e));
        assert!(
            screen
                .widgets(&e)
                .iter()
                .any(|w| w.text == "Join a world or server to choose its version")
        );
        click(&mut screen, "ver:3", &mut e);
        assert!(e.settings.seed_atlas.game_versions.is_empty());
        assert!(!e.settings_changed);
    }

    #[test]
    fn confirm_reset_asks_and_answers() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::ConfirmReset(ConfirmResetScreen::new(&e, None));
        let ws = screen.widgets(&e);
        within_gui(&ws, e.gui);
        assert!(ws.iter().any(|w| w.text == "Reset completed structures?"));
        assert!(ws.iter().any(|w| {
            w.text == "All 3 completed structures of My World become not completed again."
        }));
        assert_eq!(
            click(&mut screen, "no", &mut e),
            Action::Open(Screen::Settings(crate::settings_screen::SettingsScreen::at(crate::settings_screen::Category::Structures)))
        );
        assert_eq!(
            click(&mut screen, "yes", &mut e),
            Action::ResetCompleted(None)
        );
    }

}

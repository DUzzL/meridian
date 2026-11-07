//! Meridian's screens: the waypoint editor (B), the waypoint list (U) and
//! the settings (Y), laid out like the reference mod's (`docs/DECISIONS.md`
//! D-021) in GUI pixels around the screen's centre.
//!
//! A screen lays out [`Widget`]s for the frame, takes clicks, typing and
//! keys, and answers an [`Action`] for what only the host can do (close
//! the screen, run a command, open another screen).

use crate::config::{
    AutoCaveMode, BlockColors, ClockInfo, Corner, LightInfo, MinimapShape, MinimapSize, Settings,
    Slopes, WaypointSort,
};
use crate::minimap::Align;
use crate::nearest::WaypointMode;
use crate::screens_seedatlas::{
    BiomeHighlightScreen, ConfirmResetScreen, NearestKind, NearestScreen, SeedAtlasEnv,
    WorldGenerationScreen,
};
use crate::seedatlas_state::{
    MARKER_SIZE_STEPS, completed_opacity_label, cycle_game_version, game_version_name,
    next_marker_size, world_type_label,
};
use crate::ui::{self, Widget};
use crate::waypoint::{self, COLORS, DimensionWaypoints, Kind, Waypoint};

/// The names of [`COLORS`], as the game names its chat colours.
pub const COLOR_NAMES: [&str; 16] = [
    "Black",
    "Dark Blue",
    "Dark Green",
    "Dark Aqua",
    "Dark Red",
    "Dark Purple",
    "Gold",
    "Gray",
    "Dark Gray",
    "Blue",
    "Green",
    "Aqua",
    "Red",
    "Light Purple",
    "Yellow",
    "White",
];

/// What a screen sees of the game, and may change.
#[derive(Debug)]
pub struct Env<'a> {
    pub gui: [f32; 2],
    /// The cursor in GUI pixels (rows highlight under it).
    pub mouse: [f32; 2],
    /// "MeridianTest (auto)", "overworld (auto)".
    pub world_label: &'a str,
    pub dimension_label: &'a str,
    pub player_block: [i32; 3],
    pub waypoints: &'a mut DimensionWaypoints,
    pub settings: &'a mut Settings,
    /// Set when a screen changed `waypoints` / `settings`.
    pub waypoints_changed: bool,
    pub settings_changed: bool,
    /// What the Seed Atlas screens need of the game (dimension, seed,
    /// completed count, nearest options).
    pub seed_atlas: SeedAtlasEnv,
    /// The settings profiles by name, and the current one.
    pub profiles: Vec<String>,
    pub profile: String,
    /// Where the minimap is drawn now (for the position editor).
    pub minimap: Option<MinimapPlace>,
    /// The worlds and dimensions the waypoint screens can show.
    pub places: Places,
}

/// The choices of the waypoint screens' "World/Server" and
/// "Sub-World/Dimension" dropdowns: labels, the first being the automatic
/// one (where the player is), and which is shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    pub worlds: Vec<String>,
    pub world: usize,
    pub dimensions: Vec<String>,
    pub dimension: usize,
}

/// An open dropdown of the waypoint screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dropdown {
    World,
    Dimension,
}

/// The entries of the open dropdown, under its selector (`world` or
/// `dimension` rect), over everything else.
fn dropdown_widgets(
    open: Option<Dropdown>,
    world: [f32; 4],
    dimension: [f32; 4],
    env: &Env<'_>,
) -> Vec<Widget> {
    let Some(open) = open else { return Vec::new() };
    let (rect, entries, chosen, tag) = match open {
        Dropdown::World => (world, &env.places.worlds, env.places.world, "w"),
        Dropdown::Dimension => (dimension, &env.places.dimensions, env.places.dimension, "d"),
    };
    let row = 12.0;
    let room = ((env.gui[1] - rect[1] - rect[3] - 4.0) / row)
        .floor()
        .max(1.0) as usize;
    let shown = entries.len().min(room);
    let top = rect[1] + rect[3];
    let mut ws = vec![Widget {
        id: "dd:panel".into(),
        rect: [rect[0], top, rect[2], shown as f32 * row + 2.0],
        text: String::new(),
        kind: ui::Kind::Panel { fill: 0xF010_1010 },
    }];
    for (i, label) in entries.iter().take(shown).enumerate() {
        let y = top + 1.0 + i as f32 * row;
        ws.push(Widget::row(
            &format!("dd:{tag}:{i}"),
            [rect[0] + 1.0, y, rect[2] - 2.0, row],
            i == chosen,
        ));
        ws.push(Widget::label(
            rect[0] + rect[2] / 2.0,
            y + 2.0,
            label.clone(),
            Align::Center,
        ));
    }
    ws
}

/// A click while a dropdown may be open: opens or closes one, or picks an
/// entry. `Some` when the click was the dropdown's.
fn dropdown_click(open: &mut Option<Dropdown>, id: Option<&str>) -> Option<Action> {
    let was = open.take();
    match id {
        Some("world") => {
            *open = (was != Some(Dropdown::World)).then_some(Dropdown::World);
            Some(Action::None)
        }
        Some("dimension") => {
            *open = (was != Some(Dropdown::Dimension)).then_some(Dropdown::Dimension);
            Some(Action::None)
        }
        Some(entry) if entry.starts_with("dd:") => {
            let mut parts = entry.split(':').skip(1);
            let (tag, i) = (parts.next()?, parts.next()?.parse::<usize>().ok());
            match (tag, i) {
                ("w", Some(i)) => Some(Action::ShowPlace {
                    world: Some(i),
                    dimension: None,
                }),
                ("d", Some(i)) => Some(Action::ShowPlace {
                    world: None,
                    dimension: Some(i),
                }),
                _ => Some(Action::None),
            }
        }
        // A click elsewhere only closes an open dropdown.
        _ if was.is_some() => Some(Action::None),
        _ => None,
    }
}

/// The minimap's rectangle on screen and its margin from the corner (GUI
/// pixels), as the layout placed it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapPlace {
    pub rect: [f32; 4],
    pub margin: f32,
}

/// A change to the settings profiles, applied by the host.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileOp {
    Select(String),
    /// A copy of the current profile under a new name, then selected.
    Create(String),
    Rename(String),
    Delete(String),
}

impl Env<'_> {
    pub(crate) fn center(&self) -> [f32; 2] {
        [(self.gui[0] / 2.0).floor(), (self.gui[1] / 2.0).floor()]
    }
}

/// What the host does after an input.
#[derive(Debug, PartialEq)]
pub enum Action {
    None,
    Close,
    Open(Screen),
    /// Add (`edit` none) or replace a waypoint, in `set`.
    Save {
        edit: Option<u64>,
        set: String,
        waypoint: Waypoint,
    },
    Teleport(u64),
    Share(u64),
    /// Run a nearest search off the render thread: from the player's
    /// position in the current dimension (`nearest_biome` /
    /// `nearest_structure`), then show the result on the map and leave a
    /// waypoint as `waypoint` says. The host reports back with
    /// `NearestScreen::finish`.
    FindNearest {
        kind: NearestKind,
        waypoint: WaypointMode,
        skip_completed: bool,
    },
    /// Reset the completed structures of one kind (a marker id) or of every
    /// kind of the open world or server (`CompletedStore::reset_prompt` +
    /// `confirm_reset`), then show the marker settings again.
    ResetCompleted(Option<String>),
    Profile(ProfileOp),
    /// Show the waypoints of another world (index into `Places::worlds`)
    /// or dimension (into `Places::dimensions`); 0 is the automatic one.
    ShowPlace {
        world: Option<usize>,
        dimension: Option<usize>,
    },
}

#[derive(Debug, PartialEq)]
pub enum Screen {
    Editor(Editor),
    List(List),
    Settings(SettingsScreen),
    Nearest(NearestScreen),
    BiomeHighlight(BiomeHighlightScreen),
    WorldGeneration(WorldGenerationScreen),
    ConfirmReset(ConfirmResetScreen),
    Position(PositionScreen),
}

impl Screen {
    pub fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        match self {
            Screen::Editor(s) => s.widgets(env),
            Screen::List(s) => s.widgets(env),
            Screen::Settings(s) => s.widgets(env),
            Screen::Nearest(s) => s.widgets(env),
            Screen::BiomeHighlight(s) => s.widgets(env),
            Screen::WorldGeneration(s) => s.widgets(env),
            Screen::ConfirmReset(s) => s.widgets(env),
            Screen::Position(s) => s.widgets(env),
        }
    }

    /// The tooltip of what is under `at`, if it has one.
    pub fn tooltip(&self, at: [f32; 2], env: &Env<'_>) -> Option<String> {
        let widgets = self.widgets(env);
        let id = widgets
            .iter()
            .rev()
            .find(|w| !w.id.is_empty() && w.contains(at))?
            .id
            .clone();
        match self {
            Screen::Settings(s) => s.tooltip(&id, env),
            _ => None,
        }
    }

    /// A click at `at`; `right` for the right button.
    pub fn click(&mut self, at: [f32; 2], right: bool, env: &mut Env<'_>) -> Action {
        let widgets = self.widgets(env);
        let id = ui::hit(&widgets, at).map(|w| w.id.clone());
        match self {
            Screen::Position(s) => s.click(id.as_deref(), at, env),
            Screen::Editor(s) => s.click(id.as_deref(), right, env),
            Screen::List(s) => s.click(id.as_deref(), right, env),
            Screen::Settings(s) => s.click(id.as_deref(), at, right, env),
            Screen::Nearest(s) => s.click(id.as_deref(), env),
            Screen::BiomeHighlight(s) => s.click(id.as_deref(), env),
            Screen::WorldGeneration(s) => s.click(id.as_deref(), env),
            Screen::ConfirmReset(s) => s.click(id.as_deref()),
        }
    }

    pub fn typed(&mut self, c: char, env: &mut Env<'_>) {
        match self {
            Screen::Editor(s) => s.typed(c),
            Screen::List(s) => s.typed(c),
            Screen::Settings(s) => s.typed(c),
            Screen::Nearest(s) => s.typed(c),
            Screen::BiomeHighlight(s) => s.typed(c),
            _ => {}
        }
        let _ = env;
    }

    /// The mouse moved with the left button down.
    pub fn drag(&mut self, at: [f32; 2], env: &mut Env<'_>) {
        match self {
            Screen::Position(s) => s.drag(at, env),
            Screen::Settings(s) => s.drag(at, env),
            _ => {}
        }
    }

    /// A drag that changes settings is under way (a slider, the minimap
    /// being moved): the host applies its changes at once but writes the
    /// settings file only once it ends.
    pub fn dragging(&self) -> bool {
        match self {
            Screen::Position(s) => s.grab.is_some(),
            Screen::Settings(s) => s.dragging.is_some(),
            _ => false,
        }
    }

    /// The left button went up.
    pub fn release(&mut self) {
        match self {
            Screen::Position(s) => s.grab = None,
            Screen::Settings(s) => s.release(),
            _ => {}
        }
    }

    /// A key by Minecraft name (`key.keyboard.backspace`).
    pub fn key(&mut self, key: &str, env: &mut Env<'_>) -> Action {
        match self {
            Screen::Editor(s) => s.key(key, env),
            Screen::List(s) => s.key(key, env),
            Screen::Settings(s) => s.key(key, env),
            Screen::Nearest(s) => s.key(key),
            Screen::BiomeHighlight(s) => s.key(key),
            _ => Action::None,
        }
    }

    /// The quick confirm key: confirms what the screen asks (the editor's
    /// waypoint, the list's pending set name or deletion, a reset).
    pub fn quick_confirm(&mut self, env: &mut Env<'_>) -> Action {
        match self {
            Screen::Editor(s) => s.confirm(env),
            Screen::List(s) => s.quick_confirm(env),
            Screen::ConfirmReset(s) => s.click(Some("yes")),
            _ => Action::None,
        }
    }

    /// The modifier keys held during the next click: Shift selects a range
    /// of waypoints, Ctrl (or Cmd) adds or removes one.
    pub fn set_modifiers(&mut self, shift: bool, ctrl: bool) {
        if let Screen::List(s) = self {
            s.shift = shift;
            s.ctrl = ctrl;
        }
    }

    pub fn scroll(&mut self, lines: f32, env: &Env<'_>) {
        match self {
            Screen::Settings(s) => s.scroll(lines, env),
            Screen::List(s) => s.scroll = (s.scroll as f32 - lines).max(0.0) as usize,
            Screen::Nearest(s) => s.scroll(lines),
            Screen::BiomeHighlight(s) => s.scroll(lines),
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- editor --

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    X,
    Y,
    Z,
    Yaw,
    Initials,
}

const FIELD_ORDER: [Field; 6] = [
    Field::Name,
    Field::X,
    Field::Y,
    Field::Z,
    Field::Yaw,
    Field::Initials,
];

#[derive(Debug, PartialEq)]
pub struct Editor {
    pub edit: Option<u64>,
    pub name: String,
    pub x: String,
    pub y: String,
    pub z: String,
    pub yaw: String,
    pub initials: String,
    pub color: u8,
    pub global: bool,
    pub enabled: bool,
    pub set: String,
    pub focus: Option<Field>,
    /// Back to the list after confirming or cancelling.
    pub from_list: bool,
    pub dropdown: Option<Dropdown>,
}

impl Editor {
    /// A new waypoint at the player, in a random bright colour.
    pub fn new(waypoints: &DimensionWaypoints, seed: u64, from_list: bool) -> Self {
        // Bright colours only: skip black and the dark ones.
        const BRIGHT: [u8; 10] = [6, 7, 9, 10, 11, 12, 13, 14, 15, 3];
        Self {
            edit: None,
            name: String::new(),
            x: String::new(),
            y: String::new(),
            z: String::new(),
            yaw: String::new(),
            initials: String::new(),
            color: BRIGHT[(seed % BRIGHT.len() as u64) as usize],
            global: false,
            enabled: true,
            set: waypoints.current_set.clone(),
            focus: Some(Field::Name),
            from_list,
            dropdown: None,
        }
    }

    pub fn edit(w: &Waypoint, set: &str, from_list: bool) -> Self {
        Self {
            edit: Some(w.id),
            name: w.name.clone(),
            x: w.pos[0].to_string(),
            y: w.height_text(),
            z: w.pos[2].to_string(),
            yaw: w.yaw.map(|y| format!("{y:.0}")).unwrap_or_default(),
            initials: w.initials.clone(),
            color: w.color,
            global: w.global,
            enabled: !w.disabled,
            set: set.to_owned(),
            focus: Some(Field::Name),
            from_list,
            dropdown: None,
        }
    }

    fn text_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::Name => &mut self.name,
            Field::X => &mut self.x,
            Field::Y => &mut self.y,
            Field::Z => &mut self.z,
            Field::Yaw => &mut self.yaw,
            Field::Initials => &mut self.initials,
        }
    }

    /// The waypoint the fields describe, if they are valid; empty
    /// coordinates take the player's, and a Y of `~` is an unknown height.
    pub fn waypoint(&self, player: [i32; 3]) -> Option<Waypoint> {
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        let coord = |s: &str, default: i32| {
            if s.trim().is_empty() {
                Some(default)
            } else {
                s.trim().parse().ok()
            }
        };
        let unknown = self.y.trim() == "~";
        let y = if unknown {
            player[1]
        } else {
            coord(&self.y, player[1])?
        };
        let pos = [coord(&self.x, player[0])?, y, coord(&self.z, player[2])?];
        let yaw = if self.yaw.trim().is_empty() {
            None
        } else {
            Some(self.yaw.trim().parse::<f32>().ok()?)
        };
        let mut w = Waypoint::new(name, pos, self.color);
        w.y_unknown = unknown;
        if !self.initials.trim().is_empty() {
            w.initials = self.initials.trim().to_owned();
        }
        w.yaw = yaw;
        w.global = self.global;
        w.disabled = !self.enabled;
        Some(w)
    }

    fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, c] = env.center();
        let field = |id: &str, f: Field, rect: [f32; 4], text: &str, placeholder: &str| {
            Widget::field(id, rect, text, placeholder, self.focus == Some(f))
        };
        let [px, py, pz] = env.player_block;
        let title = if self.edit.is_some() {
            "Edit Waypoint"
        } else {
            "New Waypoint"
        };
        let mut ws = vec![
            Widget::label(cx, c - 99.0, title, Align::Center),
            Widget::selector(
                "world",
                [cx - 203.0, c - 85.0, 201.0, ui::SELECTOR_H],
                env.world_label,
                0xFFFF_FFFF,
            ),
            Widget::selector(
                "dimension",
                [cx + 2.0, c - 85.0, 201.0, ui::SELECTOR_H],
                env.dimension_label,
                0xFFFF_FFFF,
            ),
            Widget::button(
                "reset",
                [cx - 203.0, c - 66.0, 97.0, ui::BUTTON_H],
                "Reset",
                true,
            ),
            Widget::selector(
                "set",
                [cx - 100.0, c - 63.0, 200.0, ui::SELECTOR_H],
                set_label(&self.set),
                0xFFFF_FFFF,
            ),
            Widget::selector(
                "color",
                [cx - 60.0, c - 41.0, 120.0, ui::SELECTOR_H],
                COLOR_NAMES[(self.color % 16) as usize],
                0xFF00_0000 | COLORS[(self.color % 16) as usize],
            ),
            field(
                "name",
                Field::Name,
                [cx - 100.0, c - 19.0, 200.0, ui::BUTTON_H],
                &self.name,
                "",
            ),
        ];
        for (i, (id, f, text, ph)) in [
            ("x", Field::X, &self.x, px.to_string()),
            ("y", Field::Y, &self.y, py.to_string()),
            ("z", Field::Z, &self.z, pz.to_string()),
            ("yaw", Field::Yaw, &self.yaw, "yaw".to_owned()),
        ]
        .into_iter()
        .enumerate()
        {
            ws.push(field(
                id,
                f,
                [cx - 109.0 + i as f32 * 56.0, c + 11.0, 50.0, ui::BUTTON_H],
                text,
                &ph,
            ));
        }
        ws.push(Widget::button(
            "global",
            [cx - 109.0, c + 41.0, 79.0, ui::BUTTON_H],
            if self.global { "Global" } else { "Local" },
            true,
        ));
        ws.push(field(
            "initials",
            Field::Initials,
            [cx - 25.0, c + 41.0, 50.0, ui::BUTTON_H],
            &self.initials,
            "initials",
        ));
        ws.push(Widget::button(
            "enabled",
            [cx + 31.0, c + 41.0, 79.0, ui::BUTTON_H],
            if self.enabled { "Enabled" } else { "Disabled" },
            true,
        ));
        let valid = self.waypoint(env.player_block).is_some();
        ws.push(Widget::button(
            "confirm",
            [cx - 155.0, c + 86.0, 150.0, ui::BUTTON_H],
            "Confirm",
            valid,
        ));
        ws.push(Widget::button(
            "cancel",
            [cx + 5.0, c + 86.0, 150.0, ui::BUTTON_H],
            "Cancel",
            true,
        ));
        ws.extend(dropdown_widgets(
            self.dropdown,
            [cx - 203.0, c - 85.0, 201.0, ui::SELECTOR_H],
            [cx + 2.0, c - 85.0, 201.0, ui::SELECTOR_H],
            env,
        ));
        ws
    }

    fn leave(&self) -> Action {
        if self.from_list {
            Action::Open(Screen::List(List::default()))
        } else {
            Action::Close
        }
    }

    pub(crate) fn confirm(&mut self, env: &Env<'_>) -> Action {
        match self.waypoint(env.player_block) {
            Some(mut w) => {
                if let Some(id) = self.edit {
                    w.id = id;
                }
                let save = Action::Save {
                    edit: self.edit,
                    set: self.set.clone(),
                    waypoint: w,
                };
                // The host saves, then this screen leaves.
                save
            }
            None => Action::None,
        }
    }

    fn click(&mut self, id: Option<&str>, right: bool, env: &mut Env<'_>) -> Action {
        self.focus = None;
        if let Some(action) = dropdown_click(&mut self.dropdown, id) {
            if matches!(action, Action::ShowPlace { .. }) {
                // The set list belongs to the place shown.
                self.set = env.waypoints.current_set.clone();
            }
            return action;
        }
        match id {
            Some("name") => self.focus = Some(Field::Name),
            Some("x") => self.focus = Some(Field::X),
            Some("y") => self.focus = Some(Field::Y),
            Some("z") => self.focus = Some(Field::Z),
            Some("yaw") => self.focus = Some(Field::Yaw),
            Some("initials") => self.focus = Some(Field::Initials),
            Some("color") => self.color = (self.color + if right { 15 } else { 1 }) % 16,
            Some("set") => {
                let names: Vec<&String> = env.waypoints.sets.keys().collect();
                let i = names.iter().position(|n| **n == self.set).unwrap_or(0);
                let n = names.len().max(1);
                let j = if right { (i + n - 1) % n } else { (i + 1) % n };
                if let Some(name) = names.get(j) {
                    self.set = (*name).clone();
                }
            }
            Some("global") => self.global = !self.global,
            Some("enabled") => self.enabled = !self.enabled,
            Some("reset") => {
                for f in [Field::X, Field::Y, Field::Z, Field::Yaw, Field::Initials] {
                    self.text_mut(f).clear();
                }
            }
            Some("confirm") => return self.confirm(env),
            Some("cancel") => return self.leave(),
            _ => {}
        }
        Action::None
    }

    fn typed(&mut self, c: char) {
        let Some(f) = self.focus else { return };
        let (max, accept): (usize, fn(char) -> bool) = match f {
            Field::Name => (32, |_| true),
            Field::X | Field::Z => (9, |c| c.is_ascii_digit() || c == '-'),
            // `~`: the height is not known.
            Field::Y => (9, |c| c.is_ascii_digit() || c == '-' || c == '~'),
            Field::Yaw => (6, |c| c.is_ascii_digit() || c == '-' || c == '.'),
            Field::Initials => (2, |c| !c.is_whitespace()),
        };
        ui::type_char(self.text_mut(f), c, max, accept);
    }

    fn key(&mut self, key: &str, env: &mut Env<'_>) -> Action {
        match key {
            "key.keyboard.backspace" => {
                if let Some(f) = self.focus {
                    ui::backspace(self.text_mut(f));
                }
            }
            "key.keyboard.tab" => {
                let i = self
                    .focus
                    .and_then(|f| FIELD_ORDER.iter().position(|o| *o == f))
                    .map_or(0, |i| (i + 1) % 6);
                self.focus = Some(FIELD_ORDER[i]);
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => return self.confirm(env),
            _ => {}
        }
        Action::None
    }

    /// After the host saved: where to go.
    pub fn after_save(&self) -> Action {
        self.leave()
    }
}

fn set_label(set: &str) -> String {
    if set == waypoint::DEFAULT_SET {
        "Default".to_owned()
    } else {
        set.to_owned()
    }
}

// ------------------------------------------------------------------ list --

/// A set name being typed in the list: a new set, or the current one
/// renamed.
#[derive(Debug, Default, PartialEq)]
pub struct SetName {
    pub rename: bool,
    pub text: String,
}

#[derive(Debug, Default, PartialEq)]
pub struct List {
    /// The selected waypoints, in the order they were picked.
    pub selected: Vec<u64>,
    /// Where a Shift click's range starts.
    pub anchor: Option<u64>,
    /// Modifier keys held during the click being handled (the host sets
    /// them, [`Screen::set_modifiers`]).
    pub shift: bool,
    pub ctrl: bool,
    pub filter: String,
    pub filter_focused: bool,
    pub scroll: usize,
    /// "Clear Set" was clicked once; a second click clears.
    pub confirm_clear: bool,
    /// "Delete Set" was clicked once; a second click deletes.
    pub confirm_delete_set: bool,
    /// A set name being typed.
    pub naming: Option<SetName>,
    pub dropdown: Option<Dropdown>,
}

const ROW_H: f32 = 12.0;

const SORTS: [WaypointSort; 4] = [
    WaypointSort::Creation,
    WaypointSort::Name,
    WaypointSort::Distance,
    WaypointSort::Initials,
];

fn sort_label(sort: WaypointSort) -> &'static str {
    match sort {
        WaypointSort::Creation => "Sort: Newest",
        WaypointSort::Name => "Sort: Name",
        WaypointSort::Distance => "Sort: Distance",
        WaypointSort::Initials => "Sort: Initials",
    }
}

impl List {
    /// The player's position, when the list shows the player's own world
    /// and dimension (distances mean nothing elsewhere).
    fn player(env: &Env<'_>) -> Option<[f64; 3]> {
        (env.places.world == 0 && env.places.dimension == 0)
            .then(|| env.player_block.map(|v| v as f64 + 0.5))
    }

    /// The current set's waypoints matching the filter, in the chosen
    /// order (ties keep the set's own order).
    fn rows<'a>(&self, env: &'a Env<'_>) -> Vec<&'a Waypoint> {
        let filter = self.filter.to_lowercase();
        let mut rows: Vec<&Waypoint> = env
            .waypoints
            .current()
            .iter()
            .filter(|w| filter.is_empty() || w.name.to_lowercase().contains(&filter))
            .collect();
        match env.settings.waypoints.sort {
            WaypointSort::Creation => rows.sort_by_key(|w| std::cmp::Reverse(w.created)),
            WaypointSort::Name => rows.sort_by_cached_key(|w| w.name.to_lowercase()),
            WaypointSort::Initials => rows.sort_by_cached_key(|w| w.initials.to_lowercase()),
            WaypointSort::Distance => {
                if let Some(p) = Self::player(env) {
                    rows.sort_by(|a, b| a.distance(p).total_cmp(&b.distance(p)))
                }
            }
        }
        rows
    }

    fn visible_rows(env: &Env<'_>) -> usize {
        (((env.gui[1] - 62.0) - 62.0) / ROW_H).max(1.0) as usize
    }

    /// The one selected waypoint, when exactly one is.
    fn single(&self) -> Option<u64> {
        match self.selected.as_slice() {
            [one] => Some(*one),
            _ => None,
        }
    }

    fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, _] = env.center();
        let h = env.gui[1];
        let (any, one) = (!self.selected.is_empty(), self.single().is_some());
        let mut ws = vec![
            Widget::label(cx - 102.0, 9.0, "World/Server", Align::Center),
            Widget::label(cx + 102.0, 9.0, "Sub-World/Dimension", Align::Center),
            Widget::selector(
                "world",
                [cx - 203.0, 17.0, 201.0, ui::SELECTOR_H],
                env.world_label,
                0xFFFF_FFFF,
            ),
            Widget::selector(
                "dimension",
                [cx + 2.0, 17.0, 201.0, ui::SELECTOR_H],
                env.dimension_label,
                0xFFFF_FFFF,
            ),
            Widget::button(
                "options",
                [cx - 203.0, 33.0, 74.0, ui::BUTTON_H],
                "Options",
                true,
            ),
            Widget::selector(
                "set",
                [cx - 100.0, 33.0, 200.0, ui::SELECTOR_H],
                set_label(&env.waypoints.current_set),
                0xFFFF_FFFF,
            ),
            Widget::button(
                "clear",
                [cx + 130.0, 33.0, 74.0, ui::BUTTON_H],
                if self.confirm_clear {
                    "Confirm?"
                } else {
                    "Clear Set"
                },
                !env.waypoints.current().is_empty(),
            ),
            Widget::separator(57.0, env.gui[0]),
            Widget::separator(h - 60.0, env.gui[0]),
            Widget::field(
                "filter",
                [cx - 203.0, 60.0, 74.0, ui::BUTTON_H],
                &self.filter,
                "Filter...",
                self.filter_focused,
            ),
            Widget::selector(
                "sort",
                [cx - 203.0, 84.0, 74.0, ui::SELECTOR_H],
                sort_label(env.settings.waypoints.sort),
                0xFFFF_FFFF,
            ),
        ];
        // The sets: new, rename, delete; a name field while one is typed.
        match &self.naming {
            Some(n) => {
                ws.push(Widget::field(
                    "set_name",
                    [cx + 130.0, 60.0, 74.0, ui::BUTTON_H],
                    &n.text,
                    "Set name",
                    true,
                ));
                ws.push(Widget::button(
                    "set_name_ok",
                    [cx + 130.0, 84.0, 74.0, ui::BUTTON_H],
                    if n.rename { "Rename" } else { "Create" },
                    !n.text.trim().is_empty(),
                ));
            }
            None => {
                ws.push(Widget::button(
                    "new_set",
                    [cx + 130.0, 60.0, 74.0, ui::BUTTON_H],
                    "New Set",
                    true,
                ));
                ws.push(Widget::button(
                    "rename_set",
                    [cx + 130.0, 84.0, 74.0, ui::BUTTON_H],
                    "Rename Set",
                    true,
                ));
            }
        }
        ws.push(Widget::button(
            "delete_set",
            [cx + 130.0, 108.0, 74.0, ui::BUTTON_H],
            if self.confirm_delete_set {
                "Confirm?"
            } else {
                "Delete Set"
            },
            env.waypoints.sets.len() > 1 || !env.waypoints.current().is_empty(),
        ));
        let player = Self::player(env);
        let km_from = env.settings.waypoints.distance_in_km_from;
        for (i, w) in self
            .rows(env)
            .into_iter()
            .skip(self.scroll)
            .take(Self::visible_rows(env))
            .enumerate()
        {
            let y = 62.0 + i as f32 * ROW_H;
            ws.push(Widget::row(
                &format!("row:{}", w.id),
                [cx - 110.0, y, 220.0, ROW_H],
                self.selected.contains(&w.id),
            ));
            let color = if w.disabled { 0xFF80_8080 } else { 0xFFFF_FFFF };
            ws.push(Widget {
                id: String::new(),
                rect: [cx, y + 2.0, 0.0, 9.0],
                text: w.name.clone(),
                kind: ui::Kind::Label {
                    color,
                    align: Align::Center,
                },
            });
            ws.push(Widget {
                id: format!("mark:{}", w.id),
                rect: [cx - 102.0, y + 1.0, 10.0, 10.0],
                text: w.initials.chars().take(1).collect(),
                kind: ui::Kind::Mark {
                    color: 0xFF00_0000 | w.rgb(),
                },
            });
            if let Some(p) = player {
                ws.push(Widget {
                    id: String::new(),
                    rect: [cx + 107.0, y + 2.0, 0.0, 9.0],
                    text: waypoint::distance_label(w.distance(p), km_from, 1),
                    kind: ui::Kind::Label {
                        color: 0xFFA0_A0A0,
                        align: Align::Right,
                    },
                });
            }
        }
        let y = h - 52.0;
        let all_disabled = self
            .selected
            .iter()
            .filter_map(|s| env.waypoints.find(*s))
            .all(|w| w.disabled);
        for (i, (id, label)) in [
            ("add", "Add/Edit"),
            ("teleport", "Teleport (T)"),
            ("share", "Share"),
            ("toggle", "Enable"),
            ("delete", "Delete"),
        ]
        .into_iter()
        .enumerate()
        {
            let enabled = match id {
                "add" => true,
                "teleport" | "share" => one,
                _ => any,
            };
            let label = match id {
                "toggle" if any && !all_disabled => "Disable",
                "add" if !one => "Add",
                _ => label,
            };
            ws.push(Widget::button(
                id,
                [cx - 203.0 + i as f32 * 83.0, y, 74.0, ui::BUTTON_H],
                label,
                enabled,
            ));
        }
        ws.push(Widget::button(
            "done",
            [cx - 100.0, h - 28.0, 200.0, ui::BUTTON_H],
            "Done",
            true,
        ));
        ws.extend(dropdown_widgets(
            self.dropdown,
            [cx - 203.0, 17.0, 201.0, ui::SELECTOR_H],
            [cx + 2.0, 17.0, 201.0, ui::SELECTOR_H],
            env,
        ));
        ws
    }

    /// A click on a row: alone it selects the row (or clears it when it was
    /// the only one); with Ctrl it adds or removes the row; with Shift it
    /// selects the rows from the last one clicked.
    fn pick(&mut self, id: u64, env: &Env<'_>) {
        if self.shift
            && let Some(anchor) = self.anchor
        {
            let ids: Vec<u64> = self.rows(env).iter().map(|w| w.id).collect();
            if let (Some(a), Some(b)) = (
                ids.iter().position(|x| *x == anchor),
                ids.iter().position(|x| *x == id),
            ) {
                self.selected = ids[a.min(b)..=a.max(b)].to_vec();
                return;
            }
        }
        if self.ctrl {
            match self.selected.iter().position(|x| *x == id) {
                Some(i) => {
                    self.selected.remove(i);
                }
                None => self.selected.push(id),
            }
        } else if self.selected == [id] {
            self.selected.clear();
        } else {
            self.selected = vec![id];
        }
        self.anchor = Some(id);
    }

    fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    fn delete_selected(&mut self, env: &mut Env<'_>) {
        for id in std::mem::take(&mut self.selected) {
            env.waypoints.remove(id);
            env.waypoints_changed = true;
        }
        self.anchor = None;
    }

    /// Creates or renames the set being named.
    fn finish_naming(&mut self, env: &mut Env<'_>) {
        let Some(n) = self.naming.take() else { return };
        let name = n.text.trim();
        let current = env.waypoints.current_set.clone();
        let done = if n.rename {
            env.waypoints.rename_set(&current, name)
        } else {
            env.waypoints.create_set(name)
        };
        if done {
            env.waypoints.current_set = name.to_owned();
            env.waypoints_changed = true;
            self.clear_selection();
        }
    }

    fn quick_confirm(&mut self, env: &mut Env<'_>) -> Action {
        if self.naming.is_some() {
            self.finish_naming(env);
        } else if self.confirm_clear {
            return self.click(Some("clear"), false, env);
        } else if self.confirm_delete_set {
            return self.click(Some("delete_set"), false, env);
        }
        Action::None
    }

    fn click(&mut self, id: Option<&str>, right: bool, env: &mut Env<'_>) -> Action {
        self.filter_focused = false;
        if let Some(action) = dropdown_click(&mut self.dropdown, id) {
            if matches!(action, Action::ShowPlace { .. }) {
                self.clear_selection();
            }
            return action;
        }
        if id != Some("clear") {
            self.confirm_clear = false;
        }
        if id != Some("delete_set") {
            self.confirm_delete_set = false;
        }
        if !matches!(id, Some("set_name" | "set_name_ok")) {
            self.naming = None;
        }
        match id {
            Some("filter") => self.filter_focused = true,
            Some(r) if r.starts_with("row:") => {
                if let Ok(id) = r[4..].parse() {
                    self.pick(id, env);
                }
            }
            Some("set") => {
                if right {
                    env.waypoints.previous_set();
                } else {
                    env.waypoints.next_set();
                }
                self.clear_selection();
                env.waypoints_changed = true;
            }
            Some("sort") => {
                let sort = &mut env.settings.waypoints.sort;
                let i = SORTS.iter().position(|s| s == sort).unwrap_or(0);
                let n = SORTS.len();
                *sort = SORTS[if right { (i + n - 1) % n } else { (i + 1) % n }];
                env.settings_changed = true;
            }
            Some("clear") => {
                if self.confirm_clear {
                    let set = env.waypoints.current_set.clone();
                    if let Some(list) = env.waypoints.sets.get_mut(&set) {
                        list.clear();
                    }
                    env.waypoints_changed = true;
                    self.clear_selection();
                    self.confirm_clear = false;
                } else {
                    self.confirm_clear = true;
                }
            }
            Some("new_set") => self.naming = Some(SetName::default()),
            Some("rename_set") => {
                self.naming = Some(SetName {
                    rename: true,
                    text: set_label(&env.waypoints.current_set),
                })
            }
            Some("set_name_ok") => self.finish_naming(env),
            Some("delete_set") => {
                if self.confirm_delete_set {
                    let set = env.waypoints.current_set.clone();
                    env.waypoints.delete_set(&set);
                    env.waypoints_changed = true;
                    self.clear_selection();
                    self.confirm_delete_set = false;
                } else {
                    self.confirm_delete_set = true;
                }
            }
            Some("add") => {
                let editor = match self.single().and_then(|s| env.waypoints.find(s)) {
                    Some(w) => Editor::edit(w, &env.waypoints.current_set.clone(), true),
                    None => Editor::new(env.waypoints, waypoint::now(), true),
                };
                return Action::Open(Screen::Editor(editor));
            }
            Some("teleport") => return self.single().map_or(Action::None, Action::Teleport),
            Some("share") => return self.single().map_or(Action::None, Action::Share),
            Some("toggle") => {
                let all_disabled = self
                    .selected
                    .iter()
                    .filter_map(|s| env.waypoints.find(*s))
                    .all(|w| w.disabled);
                for id in &self.selected {
                    if let Some(w) = env.waypoints.find_mut(*id) {
                        w.disabled = !all_disabled;
                        env.waypoints_changed = true;
                    }
                }
            }
            Some("delete") => self.delete_selected(env),
            Some("options") => {
                return Action::Open(Screen::Settings(SettingsScreen::at(Category::Waypoints)));
            }
            Some("done") => return Action::Close,
            _ => {}
        }
        Action::None
    }

    fn typed(&mut self, c: char) {
        if let Some(n) = &mut self.naming {
            ui::type_char(&mut n.text, c, 32, |_| true);
        } else if self.filter_focused && ui::type_char(&mut self.filter, c, 24, |_| true) {
            self.scroll = 0;
        }
    }

    fn key(&mut self, key: &str, env: &mut Env<'_>) -> Action {
        if let Some(n) = &mut self.naming {
            match key {
                "key.keyboard.backspace" => {
                    ui::backspace(&mut n.text);
                }
                "key.keyboard.enter" | "key.keyboard.keypad.enter" => self.finish_naming(env),
                _ => {}
            }
            return Action::None;
        }
        match key {
            "key.keyboard.backspace" if self.filter_focused => {
                ui::backspace(&mut self.filter);
            }
            "key.keyboard.t" if !self.filter_focused => {
                return self.single().map_or(Action::None, Action::Teleport);
            }
            "key.keyboard.delete" if !self.filter_focused => self.delete_selected(env),
            _ => {}
        }
        Action::None
    }
}

// -------------------------------------------------------------- settings --

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    View,
    Radar,
    Information,
    Misc,
    BlockMap,
    Overlay,
    Waypoints,
    SeedAtlas,
    WorldMap,
    Performance,
}

/// One setting: its label, its value as shown, and how a click changes it
/// (`back` for a right click).
pub struct Opt {
    pub label: &'static str,
    pub group: Group,
    pub value: fn(&Settings) -> String,
    pub step: fn(&mut Settings, bool),
}

impl std::fmt::Debug for Opt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

fn clock_label(c: ClockInfo) -> String {
    match c {
        ClockInfo::Off => "OFF".into(),
        ClockInfo::H24 => "24h".into(),
        ClockInfo::H12 => "12h".into(),
    }
}

fn on_off(b: bool) -> String {
    if b { "ON".into() } else { "OFF".into() }
}

fn cycle<T: Copy + PartialEq>(all: &[T], now: T, back: bool) -> T {
    let i = all.iter().position(|x| *x == now).unwrap_or(0);
    let n = all.len();
    all[if back { (i + n - 1) % n } else { (i + 1) % n }]
}

macro_rules! toggle {
    ($label:literal, $group:expr, $($path:ident).+) => {
        Opt { label: $label, group: $group, value: |s| on_off(s.$($path).+), step: |s, _| s.$($path).+ = !s.$($path).+ }
    };
}

/// Every setting the screens offer, by group.
pub fn options() -> Vec<Opt> {
    use Group::*;
    let mut all = vec![
        Opt {
            label: "Minimap Shape",
            group: View,
            value: |s| match s.minimap.shape {
                MinimapShape::Square => "Square".into(),
                MinimapShape::Round => "Round".into(),
            },
            step: |s, b| {
                s.minimap.shape = cycle(
                    &[MinimapShape::Square, MinimapShape::Round],
                    s.minimap.shape,
                    b,
                )
            },
        },
        Opt {
            label: "Minimap Size",
            group: View,
            value: |s| format!("{:?}", s.minimap.size),
            step: |s, b| {
                use MinimapSize::*;
                s.minimap.size = cycle(&[Auto, Small, Medium, Large, Huge], s.minimap.size, b)
            },
        },
        Opt {
            label: "Zoom",
            group: View,
            value: |s| format!("{}x", 2u32.pow(s.minimap.zoom as u32) as f32 / 2.0),
            step: |s, b| {
                let n = crate::minimap::zoom_steps(s.minimap.further_zoom_out).len() as u8;
                s.minimap.zoom = if b {
                    (s.minimap.zoom + n - 1) % n
                } else {
                    (s.minimap.zoom + 1) % n
                }
            },
        },
        toggle!("Lock North", View, minimap.north_locked),
        Opt {
            label: "Opacity",
            group: View,
            value: |s| format!("{}%", s.minimap.opacity),
            step: |s, b| {
                let o = s.minimap.opacity as i32 + if b { -10 } else { 10 };
                s.minimap.opacity = if o > 100 {
                    10
                } else if o < 10 {
                    100
                } else {
                    o as u8
                };
            },
        },
        Opt {
            label: "Position",
            group: View,
            value: |s| match s.minimap.corner {
                Corner::TopLeft => "Top Left".into(),
                Corner::TopRight => "Top Right".into(),
                Corner::BottomLeft => "Bottom Left".into(),
                Corner::BottomRight => "Bottom Right".into(),
            },
            step: |s, b| {
                use Corner::*;
                s.minimap.corner = cycle(
                    &[TopLeft, TopRight, BottomRight, BottomLeft],
                    s.minimap.corner,
                    b,
                )
            },
        },
        Opt {
            label: "Enlarge",
            group: View,
            value: |s| {
                if s.minimap.enlarge_toggles {
                    "Toggle".into()
                } else {
                    "Hold".into()
                }
            },
            step: |s, _| s.minimap.enlarge_toggles = !s.minimap.enlarge_toggles,
        },
        toggle!("Center When Enlarged", View, minimap.enlarged_centered),
        toggle!("Entity Radar", Radar, radar.enabled),
        toggle!("Hide Invisible Entities", Radar, radar.hide_invisible),
        toggle!("Mob Faces", Radar, radar.faces),
        toggle!("Faces While Tab Held", Radar, radar.faces_while_held),
        Opt {
            label: "Entity Names",
            group: Radar,
            value: |s| match s.radar.names {
                crate::config::NameMode::Off => "OFF".into(),
                crate::config::NameMode::Players => "Players".into(),
                crate::config::NameMode::All => "All".into(),
            },
            step: |s, b| {
                use crate::config::NameMode::*;
                s.radar.names = cycle(&[Off, Players, All], s.radar.names, b);
            },
        },
        toggle!("Display Players", Radar, radar.players),
        toggle!("Display Hostile Mobs", Radar, radar.hostile),
        toggle!("Display Friendly Mobs", Radar, radar.friendly),
        toggle!("Display Items", Radar, radar.items),
        toggle!("Display Other Entities", Radar, radar.other),
        toggle!("Height-Based Fade", Radar, radar.height_fade),
        toggle!("Reverse Filter", Radar, radar.reversed),
        Opt {
            label: "Height Limit",
            group: Radar,
            value: |s| format!("{}", s.radar.height_limit),
            step: |s, b| {
                const STEPS: [u32; 6] = [5, 10, 20, 40, 80, 256];
                s.radar.height_limit = cycle(&STEPS, s.radar.height_limit, b);
            },
        },
        Opt {
            label: "Dot Size",
            group: Radar,
            value: |s| format!("{}", s.radar.dot_size),
            step: |s, b| s.radar.dot_size = cycle(&[1u8, 2, 3, 4], s.radar.dot_size, b),
        },
        toggle!("Coordinates", Information, minimap.show_coords),
        toggle!("Biome", Information, minimap.show_biome),
        toggle!(
            "Overworld Coordinates",
            Information,
            minimap.info.overworld_coords
        ),
        toggle!("Chunk Coordinates", Information, minimap.info.chunk_coords),
        toggle!("Angles", Information, minimap.info.angles),
        toggle!("Dimension", Information, minimap.info.dimension),
        toggle!("Weather", Information, minimap.info.weather),
        Opt {
            label: "Light Level",
            group: Information,
            value: |s| format!("{:?}", s.minimap.info.light),
            step: |s, b| {
                use LightInfo::*;
                s.minimap.info.light = cycle(&[Off, Block, Sky, Both], s.minimap.info.light, b)
            },
        },
        Opt {
            label: "Game Time",
            group: Information,
            value: |s| clock_label(s.minimap.info.game_time),
            step: |s, b| {
                s.minimap.info.game_time = cycle(
                    &[ClockInfo::Off, ClockInfo::H24, ClockInfo::H12],
                    s.minimap.info.game_time,
                    b,
                )
            },
        },
        Opt {
            label: "Real Time",
            group: Information,
            value: |s| clock_label(s.minimap.info.real_time),
            step: |s, b| {
                s.minimap.info.real_time = cycle(
                    &[ClockInfo::Off, ClockInfo::H24, ClockInfo::H12],
                    s.minimap.info.real_time,
                    b,
                )
            },
        },
        toggle!("Fair Play (No Radar, No Caves)", Misc, minimap.fair_play),
        toggle!("Cave Mode", Misc, minimap.cave.allowed),
        Opt {
            label: "Auto Cave Mode",
            group: Misc,
            value: |s| match s.minimap.cave.auto {
                AutoCaveMode::Off => "OFF".into(),
                AutoCaveMode::Roof1x1 => "1x1 Roof".into(),
                AutoCaveMode::Roof3x3 => "3x3 Roof".into(),
                AutoCaveMode::Roof5x5 => "5x5 Roof".into(),
            },
            step: |s, b| {
                use AutoCaveMode::*;
                s.minimap.cave.auto =
                    cycle(&[Off, Roof1x1, Roof3x3, Roof5x5], s.minimap.cave.auto, b);
            },
        },
        Opt {
            label: "Cave Mode Toggle Delay",
            group: Misc,
            value: |s| format!("{:.1}s", s.minimap.cave.toggle_delay_ms as f32 / 1000.0),
            step: |s, b| {
                s.minimap.cave.toggle_delay_ms = cycle(
                    &[0u32, 500, 1000, 2000, 4000],
                    s.minimap.cave.toggle_delay_ms,
                    b,
                )
            },
        },
        toggle!("Legible Cave Maps", Misc, minimap.cave.legible_caves),
        Opt {
            label: "Cave Mode Depth",
            group: Misc,
            value: |s| format!("{}", s.minimap.cave.depth),
            step: |s, b| {
                s.minimap.cave.depth =
                    cycle(&[10u32, 20, 30, 50, 80, 128, 256], s.minimap.cave.depth, b)
            },
        },
        Opt {
            label: "Cave Mode Extra Zoom",
            group: Misc,
            value: |s| format!("{}", s.minimap.cave.extra_zoom),
            step: |s, b| {
                s.minimap.cave.extra_zoom = cycle(&[0u8, 1, 2, 3], s.minimap.cave.extra_zoom, b)
            },
        },
        toggle!("Hide Under Screens", Misc, minimap.hide_under_screens),
        toggle!("Hide Under F3", Misc, minimap.hide_under_debug),
        toggle!("Further Zoom Out", Misc, minimap.further_zoom_out),
        Opt {
            label: "Block Colors",
            group: BlockMap,
            value: |s| format!("{:?}", s.world_map.terrain.block_colors),
            step: |s, b| {
                let next = cycle(
                    &[BlockColors::Accurate, BlockColors::Vanilla],
                    s.world_map.terrain.block_colors,
                    b,
                );
                s.world_map.terrain.block_colors = next;
            },
        },
        Opt {
            label: "Terrain Slopes",
            group: BlockMap,
            value: |s| format!("{:?}", s.world_map.terrain.slopes),
            step: |s, b| {
                use Slopes::*;
                let next = cycle(
                    &[Off, Vanilla, Smooth, Strong],
                    s.world_map.terrain.slopes,
                    b,
                );
                s.world_map.terrain.slopes = next;
            },
        },
        toggle!("Terrain Depth", BlockMap, world_map.terrain.terrain_depth),
        toggle!("Lighting", BlockMap, world_map.terrain.lighting),
        toggle!("Biome Blending", BlockMap, world_map.terrain.biome_blending),
        toggle!(
            "Biome Colors In Vanilla Mode",
            BlockMap,
            world_map.terrain.biome_colors_in_vanilla
        ),
        toggle!(
            "Block Transparency",
            BlockMap,
            world_map.terrain.block_transparency
        ),
        toggle!("Display Flowers", BlockMap, world_map.terrain.show_flowers),
        toggle!(
            "Display Redstone",
            BlockMap,
            world_map.terrain.show_redstone
        ),
        toggle!(
            "Display Stained Glass",
            BlockMap,
            world_map.terrain.stained_glass
        ),
        toggle!(
            "Adjust Height For Short Blocks",
            BlockMap,
            world_map.terrain.adjust_short_blocks
        ),
        toggle!("Chunk Grid", Overlay, minimap.chunk_grid),
        toggle!("Slime Chunks", Overlay, minimap.slime_chunks),
        toggle!("Light Overlay", Overlay, minimap.light_overlay),
        Opt {
            label: "Light Overlay Min",
            group: Overlay,
            value: |s| format!("{}", s.minimap.light_overlay_range[0]),
            step: |s, b| {
                let v = s.minimap.light_overlay_range[0] as i32 + if b { -1 } else { 1 };
                s.minimap.light_overlay_range[0] = v.rem_euclid(16) as u8;
            },
        },
        Opt {
            label: "Light Overlay Max",
            group: Overlay,
            value: |s| format!("{}", s.minimap.light_overlay_range[1]),
            step: |s, b| {
                let v = s.minimap.light_overlay_range[1] as i32 + if b { -1 } else { 1 };
                s.minimap.light_overlay_range[1] = v.rem_euclid(16) as u8;
            },
        },
        toggle!("Waypoints In World", Waypoints, waypoints.in_world),
        toggle!("Waypoints On Minimap", Waypoints, waypoints.on_minimap),
        toggle!("Display All Sets", Waypoints, waypoints.all_sets),
        toggle!("Deathpoints", Waypoints, waypoints.deathpoints),
        toggle!(
            "Keep Old Deathpoints",
            Waypoints,
            waypoints.keep_old_deathpoints
        ),
        toggle!(
            "Delete Reached Deathpoints",
            Waypoints,
            waypoints.delete_reached_deathpoints
        ),
        toggle!("Waypoint Beam", Waypoints, waypoints.beam),
        Opt {
            label: "Max WP Draw Distance",
            group: Waypoints,
            value: |s| {
                if s.waypoints.max_distance == 0 {
                    "Unlimited".into()
                } else {
                    format!("{}", s.waypoints.max_distance)
                }
            },
            step: |s, b| {
                const STEPS: [u32; 8] = [0, 100, 250, 500, 1000, 2500, 5000, 10000];
                s.waypoints.max_distance = cycle(&STEPS, s.waypoints.max_distance, b);
            },
        },
    ];
    all.extend(world_map_options());
    all.extend(performance_options());
    all.extend(seed_atlas_options());
    all
}

/// The world map's own options (its settings key, `]`).
fn world_map_options() -> Vec<Opt> {
    use Group::WorldMap;
    vec![
        toggle!("World Map", WorldMap, world_map.enabled),
        toggle!("Load New Chunks", WorldMap, world_map.load_new_chunks),
        toggle!("Update Chunks", WorldMap, world_map.update_chunks),
        toggle!("Footsteps", WorldMap, world_map.footsteps),
        toggle!("Display Coordinates", WorldMap, world_map.show_coordinates),
        toggle!("Display Zoom", WorldMap, world_map.show_zoom),
        toggle!(
            "Display Hovered Biome",
            WorldMap,
            world_map.show_hovered_biome
        ),
        toggle!("Player As Arrow", WorldMap, world_map.player_as_arrow),
        toggle!("Opening Animation", WorldMap, world_map.opening_animation),
        toggle!("Unlimited Zoom In", WorldMap, world_map.unlimited_zoom_in),
        toggle!("Unlimited Zoom Out", WorldMap, world_map.unlimited_zoom_out),
        Opt {
            label: "Max Loaded Regions",
            group: WorldMap,
            value: |s| format!("{}", s.world_map.max_loaded_regions),
            step: |s, b| {
                const STEPS: [usize; 7] = [100, 200, 300, 500, 800, 1200, 2000];
                s.world_map.max_loaded_regions = cycle(&STEPS, s.world_map.max_loaded_regions, b);
            },
        },
        Opt {
            label: "Multiplayer Maps",
            group: WorldMap,
            value: |s| match s.world_map.multiplayer_maps {
                crate::config::MapSelection::Single => "One Per Server".into(),
                crate::config::MapSelection::WorldSpawn => "By World Spawn".into(),
                crate::config::MapSelection::Manual => "Manual".into(),
            },
            step: |s, b| {
                use crate::config::MapSelection::*;
                s.world_map.multiplayer_maps = cycle(
                    &[Single, WorldSpawn, Manual],
                    s.world_map.multiplayer_maps,
                    b,
                );
            },
        },
    ]
}

/// Map work budgets.
fn performance_options() -> Vec<Opt> {
    use Group::Performance;
    vec![
        Opt {
            label: "Map Frame Budget",
            group: Performance,
            value: |s| format!("{:.1} ms", s.performance.frame_budget_us as f32 / 1000.0),
            step: |s, b| {
                s.performance.frame_budget_us = cycle(
                    &[500u32, 1000, 2000, 4000, 8000],
                    s.performance.frame_budget_us,
                    b,
                )
            },
        },
        Opt {
            label: "Chunk Reads Per Tick",
            group: Performance,
            value: |s| format!("{}", s.performance.chunk_reads_per_tick),
            step: |s, b| {
                s.performance.chunk_reads_per_tick = cycle(
                    &[4usize, 8, 16, 32, 64],
                    s.performance.chunk_reads_per_tick,
                    b,
                )
            },
        },
        Opt {
            label: "Region Save Interval",
            group: Performance,
            value: |s| format!("{}s", s.performance.save_interval_s),
            step: |s, b| {
                s.performance.save_interval_s =
                    cycle(&[10u32, 30, 60, 120, 300], s.performance.save_interval_s, b)
            },
        },
        Opt {
            label: "Map Worker Threads",
            group: Performance,
            value: |s| match s.performance.worker_threads {
                0 => "Auto".into(),
                n => format!("{n}"),
            },
            step: |s, b| {
                s.performance.worker_threads =
                    cycle(&[0usize, 1, 2, 3, 4, 6, 8], s.performance.worker_threads, b)
            },
        },
    ]
}

pub use crate::settings_screen::{Category, SettingsScreen};

/// The kinds a waypoint of the list can be (re-exported for hosts).
pub type WaypointKind = Kind;

// ------------------------------------------------------- position editor --

/// "Change Position": the minimap is dragged where it should be; it snaps
/// to the nearest corner and keeps its distance from it.
#[derive(Debug, Default, PartialEq)]
pub struct PositionScreen {
    /// Where in the minimap the cursor grabbed it, while dragging.
    pub grab: Option<[f32; 2]>,
}

impl PositionScreen {
    fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let [cx, c] = env.center();
        vec![
            Widget::label(cx, c - 30.0, "Drag the minimap to move it", Align::Center),
            Widget::button(
                "reset",
                [cx - 100.0, c - 10.0, 98.0, ui::BUTTON_H],
                "Reset",
                true,
            ),
            Widget::button(
                "done",
                [cx + 2.0, c - 10.0, 98.0, ui::BUTTON_H],
                "Done",
                true,
            ),
        ]
    }

    fn click(&mut self, id: Option<&str>, at: [f32; 2], env: &mut Env<'_>) -> Action {
        match id {
            Some("done") => return Action::Open(Screen::Settings(SettingsScreen::default())),
            Some("reset") => {
                env.settings.minimap.corner = Corner::TopLeft;
                env.settings.minimap.offset = [0, 0];
                env.settings_changed = true;
            }
            _ => {
                if let Some(MinimapPlace { rect, .. }) = env.minimap
                    && at[0] >= rect[0]
                    && at[0] < rect[0] + rect[2]
                    && at[1] >= rect[1]
                    && at[1] < rect[1] + rect[3]
                {
                    self.grab = Some([at[0] - rect[0], at[1] - rect[1]]);
                }
            }
        }
        Action::None
    }

    fn drag(&mut self, at: [f32; 2], env: &mut Env<'_>) {
        let (Some(grab), Some(place)) = (self.grab, env.minimap) else {
            return;
        };
        let (corner, offset) = place_minimap([at[0] - grab[0], at[1] - grab[1]], place, env.gui);
        let m = &mut env.settings.minimap;
        if m.corner != corner || m.offset != offset {
            m.corner = corner;
            m.offset = offset;
            env.settings_changed = true;
        }
    }
}

/// The corner and offset that put the minimap's top left at `top_left`,
/// kept on screen.
pub fn place_minimap(top_left: [f32; 2], place: MinimapPlace, gui: [f32; 2]) -> (Corner, [i32; 2]) {
    let size = place.rect[2];
    let x = top_left[0].clamp(0.0, (gui[0] - size).max(0.0));
    let y = top_left[1].clamp(0.0, (gui[1] - place.rect[3]).max(0.0));
    let right = x + size / 2.0 > gui[0] / 2.0;
    let bottom = y + place.rect[3] / 2.0 > gui[1] / 2.0;
    let ox = if right {
        gui[0] - size - place.margin - x
    } else {
        x - place.margin
    };
    let oy = if bottom {
        gui[1] - size - place.margin - y
    } else {
        y - place.margin
    };
    let corner = match (right, bottom) {
        (false, false) => Corner::TopLeft,
        (true, false) => Corner::TopRight,
        (false, true) => Corner::BottomLeft,
        (true, true) => Corner::BottomRight,
    };
    (corner, [ox.round() as i32, oy.round() as i32])
}

macro_rules! marker {
    ($label:literal, $id:literal) => {
        Opt {
            label: $label,
            group: Group::SeedAtlas,
            value: |s| on_off(crate::seedatlas::marker_enabled(&s.seed_atlas, $id)),
            step: |s, _| {
                let on = crate::seedatlas::marker_enabled(&s.seed_atlas, $id);
                s.seed_atlas.markers.insert($id.to_owned(), !on);
            },
        }
    };
}

fn y_steps(now: i32, back: bool) -> i32 {
    const YS: [i32; 9] = [-64, -32, 0, 32, 64, 128, 192, 255, 320];
    let i = YS.iter().position(|y| *y >= now).unwrap_or(YS.len() - 1);
    YS[if back {
        (i + YS.len() - 1) % YS.len()
    } else {
        (i + 1) % YS.len()
    }]
}

/// The Seed Atlas options, as the Fabric version's settings offer them.
fn seed_atlas_options() -> Vec<Opt> {
    use Group::SeedAtlas;
    vec![
        toggle!("Seed Atlas", SeedAtlas, seed_atlas.enabled),
        toggle!("Seed map layer", SeedAtlas, seed_atlas.biome_layer),
        Opt {
            label: "Layer opacity",
            group: SeedAtlas,
            value: |s| format!("{}%", (s.seed_atlas.biome_opacity as u32 * 100 + 127) / 255),
            step: |s, b| {
                let pct = (s.seed_atlas.biome_opacity as i32 * 100 + 127) / 255;
                let next = if b {
                    (pct - 10).rem_euclid(110)
                } else {
                    (pct + 10) % 110
                };
                s.seed_atlas.biome_opacity = ((next * 255 + 50) / 100) as u8;
            },
        },
        toggle!("Overlay explored", SeedAtlas, seed_atlas.overlay_explored),
        toggle!("Biome highlighting", SeedAtlas, seed_atlas.biome_highlight),
        toggle!("Structures", SeedAtlas, seed_atlas.structures),
        Opt {
            label: "Completed",
            group: SeedAtlas,
            value: |s| completed_opacity_label(s.seed_atlas.completed_opacity_pct()),
            step: |s, b| {
                // 0 % (hidden) to 100 % in steps of 10; the legacy hide
                // switch is folded in.
                let pct = i32::from(s.seed_atlas.completed_opacity_pct());
                let next = if b {
                    (pct - 10).rem_euclid(110)
                } else {
                    (pct + 10) % 110
                };
                s.seed_atlas.hide_completed = false;
                s.seed_atlas.completed_opacity = next.min(100) as u8;
            },
        },
        Opt {
            label: "Marker size",
            group: SeedAtlas,
            value: |s| format!("{}%", s.seed_atlas.marker_size),
            step: |s, b| {
                s.seed_atlas.marker_size = if b {
                    let i = MARKER_SIZE_STEPS
                        .iter()
                        .position(|&v| v == s.seed_atlas.marker_size)
                        .unwrap_or(3);
                    MARKER_SIZE_STEPS[(i + MARKER_SIZE_STEPS.len() - 1) % MARKER_SIZE_STEPS.len()]
                } else {
                    next_marker_size(s.seed_atlas.marker_size)
                };
            },
        },
        Opt {
            label: "World",
            group: SeedAtlas,
            value: |s| {
                format!(
                    "{}, {}",
                    world_type_label(s.seed_atlas.large_biomes),
                    game_version_name(&s.seed_atlas, &s.seed_atlas.context_key)
                )
            },
            // A click changes the version of the open world or server; the
            // world type has its own option.
            step: |s, b| {
                if !s.seed_atlas.context_key.is_empty() {
                    let key = s.seed_atlas.context_key.clone();
                    cycle_game_version(&mut s.seed_atlas, &key, b);
                }
            },
        },
        Opt {
            label: "Biome resolution",
            group: SeedAtlas,
            value: |s| match s.seed_atlas.biome_resolution {
                1 => "Full Detail (1x)".into(),
                2 => "High (2x)".into(),
                4 => "Reduced (4x)".into(),
                8 => "Low (8x)".into(),
                _ => "Very Low (16x)".into(),
            },
            step: |s, b| {
                s.seed_atlas.biome_resolution =
                    cycle(&[1, 2, 4, 8, 16], s.seed_atlas.biome_resolution, b)
            },
        },
        Opt {
            label: "Worker threads",
            group: SeedAtlas,
            value: |s| {
                if s.seed_atlas.worker_threads == 0 {
                    "Auto".into()
                } else {
                    s.seed_atlas.worker_threads.to_string()
                }
            },
            step: |s, b| {
                s.seed_atlas.worker_threads =
                    cycle(&[0, 1, 2, 4, 8], s.seed_atlas.worker_threads, b)
            },
        },
        Opt {
            label: "Background prefetch",
            group: SeedAtlas,
            value: |s| {
                ["Off", "Near", "Medium", "Far"][s.seed_atlas.prefetch_radius.min(3) as usize]
                    .into()
            },
            step: |s, b| {
                s.seed_atlas.prefetch_radius = cycle(&[0, 1, 2, 3], s.seed_atlas.prefetch_radius, b)
            },
        },
        Opt {
            label: "World type",
            group: SeedAtlas,
            value: |s| {
                if s.seed_atlas.large_biomes {
                    "Large Biomes".into()
                } else {
                    "Normal".into()
                }
            },
            step: |s, _| s.seed_atlas.large_biomes = !s.seed_atlas.large_biomes,
        },
        Opt {
            label: "Overworld Y",
            group: SeedAtlas,
            value: |s| s.seed_atlas.overworld_y.to_string(),
            step: |s, b| s.seed_atlas.overworld_y = y_steps(s.seed_atlas.overworld_y, b),
        },
        Opt {
            label: "Nether Y",
            group: SeedAtlas,
            value: |s| s.seed_atlas.nether_y.to_string(),
            step: |s, b| s.seed_atlas.nether_y = y_steps(s.seed_atlas.nether_y, b),
        },
        Opt {
            label: "End Y",
            group: SeedAtlas,
            value: |s| s.seed_atlas.end_y.to_string(),
            step: |s, b| s.seed_atlas.end_y = y_steps(s.seed_atlas.end_y, b),
        },
        marker!("Abandoned Camp", "camp"),
        marker!("Ancient City", "ancient_city"),
        marker!("Bastion Remnant", "bastion"),
        marker!("Trial Chambers", "chambers"),
        marker!("Desert Pyramid", "desert"),
        marker!("End City", "endcity"),
        marker!("End Ship", "end_ship"),
        marker!("Nether Fortress", "fortress"),
        marker!("End Gateway", "gateway"),
        marker!("Amethyst Geode", "geode"),
        marker!("Swamp Hut", "hut"),
        marker!("Igloo", "igloo"),
        marker!("Jungle Temple", "jungle"),
        marker!("Woodland Mansion", "mansion"),
        marker!("Mineshaft", "mineshaft"),
        marker!("Ocean Monument", "monument"),
        marker!("Ore Vein", "orevein"),
        marker!("Pillager Outpost", "outpost"),
        marker!("Ruined Portal", "portal"),
        marker!("Ocean Ruins", "ruins"),
        marker!("Shipwreck", "shipwreck"),
        marker!("Slime Chunk", "slime"),
        marker!("World Spawn", "spawn"),
        marker!("Stronghold", "stronghold"),
        marker!("Trail Ruins", "trails"),
        marker!("Buried Treasure", "treasure"),
        marker!("Village", "village"),
        marker!("Desert Well", "well"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(w: &'a mut DimensionWaypoints, s: &'a mut Settings) -> Env<'a> {
        Env {
            gui: [432.0, 246.0],
            mouse: [-1.0, -1.0],
            world_label: "Test (auto)",
            dimension_label: "overworld (auto)",
            player_block: [130, 64, 10],
            waypoints: w,
            settings: s,
            waypoints_changed: false,
            settings_changed: false,
            seed_atlas: SeedAtlasEnv::default(),
            profiles: vec!["default".into()],
            profile: "default".into(),
            minimap: None,
            places: Default::default(),
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

    #[test]
    fn waypoint_world_and_dimension_dropdowns() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        e.places = Places {
            worlds: vec!["Test (auto)".into(), "Other".into()],
            world: 0,
            dimensions: vec![
                "overworld (auto)".into(),
                "overworld".into(),
                "the_end".into(),
                "the_nether".into(),
            ],
            dimension: 0,
        };
        let mut list = Screen::List(List::default());
        let ids =
            |sc: &Screen, e: &Env<'_>| sc.widgets(e).into_iter().map(|w| w.id).collect::<Vec<_>>();
        assert!(!ids(&list, &e).iter().any(|i| i.starts_with("dd:")));
        // The selector opens its list over the screen.
        let rect = |sc: &Screen, e: &Env<'_>, id: &str| {
            sc.widgets(e).into_iter().find(|w| w.id == id).unwrap().rect
        };
        let r = rect(&list, &e, "dimension");
        assert_eq!(
            list.click([r[0] + 5.0, r[1] + 5.0], false, &mut e),
            Action::None
        );
        assert!(ids(&list, &e).contains(&"dd:d:3".to_owned()));
        // Picking an entry asks the host to show that dimension.
        let r = rect(&list, &e, "dd:d:3");
        assert_eq!(
            list.click([r[0] + 5.0, r[1] + 5.0], false, &mut e),
            Action::ShowPlace {
                world: None,
                dimension: Some(3)
            }
        );
        assert!(
            !ids(&list, &e).iter().any(|i| i.starts_with("dd:")),
            "closed after picking"
        );
        // The world selector likewise; a click elsewhere only closes it.
        let r = rect(&list, &e, "world");
        list.click([r[0] + 5.0, r[1] + 5.0], false, &mut e);
        assert!(ids(&list, &e).contains(&"dd:w:1".to_owned()));
        assert_eq!(
            list.click([1.0, e.gui[1] - 1.0], false, &mut e),
            Action::None
        );
        assert!(!ids(&list, &e).iter().any(|i| i.starts_with("dd:")));
        // The editor has the same dropdowns.
        let mut editor = Screen::Editor(Editor::new(e.waypoints, 1, true));
        let r = rect(&editor, &e, "world");
        editor.click([r[0] + 5.0, r[1] + 5.0], false, &mut e);
        let r = rect(&editor, &e, "dd:w:1");
        assert_eq!(
            editor.click([r[0] + 5.0, r[1] + 5.0], false, &mut e),
            Action::ShowPlace {
                world: Some(1),
                dimension: None
            }
        );
    }

    #[test]
    fn editor_layout_matches_the_reference() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let e = env(&mut w, &mut s);
        let ws = Screen::Editor(Editor::new(e.waypoints, 0, false)).widgets(&e);
        let rect = |id: &str| ws.iter().find(|w| w.id == id).unwrap().rect;
        // Measured on a 432 x 246 GUI.
        assert_eq!(rect("world"), [13.0, 38.0, 201.0, 11.0]);
        assert_eq!(rect("name"), [116.0, 104.0, 200.0, 20.0]);
        assert_eq!(rect("x")[0], 107.0);
        assert_eq!(rect("yaw")[0], 275.0);
        assert_eq!(rect("confirm"), [61.0, 209.0, 150.0, 20.0]);
        assert_eq!(rect("cancel")[0], 221.0);
    }

    #[test]
    fn editor_creates_a_waypoint_at_the_player() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::Editor(Editor::new(e.waypoints, 3, false));
        let confirm_enabled = |sc: &Screen, e: &Env<'_>| {
            sc.widgets(e)
                .iter()
                .any(|w| w.id == "confirm" && w.kind == ui::Kind::Button { enabled: true })
        };
        assert!(!confirm_enabled(&screen, &e), "no name yet");
        for c in "Home".chars() {
            screen.typed(c, &mut e);
        }
        assert!(confirm_enabled(&screen, &e));
        let Action::Save {
            edit,
            set,
            waypoint,
        } = screen.key("key.keyboard.enter", &mut e)
        else {
            panic!()
        };
        assert_eq!((edit, set.as_str()), (None, waypoint::DEFAULT_SET));
        assert_eq!(
            (
                waypoint.name.as_str(),
                waypoint.pos,
                waypoint.initials.as_str()
            ),
            ("Home", [130, 64, 10], "H")
        );
        assert!(
            !matches!(waypoint.color, 0 | 1 | 2 | 4 | 5 | 8),
            "a bright colour"
        );
    }

    #[test]
    fn editor_fields_and_toggles() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::Editor(Editor::new(e.waypoints, 0, true));
        for c in "Base".chars() {
            screen.typed(c, &mut e);
        }
        screen.key("key.keyboard.tab", &mut e);
        for c in "-5x0".chars() {
            screen.typed(c, &mut e);
        }
        click(&mut screen, "global", &mut e);
        click(&mut screen, "enabled", &mut e);
        let before = match &screen {
            Screen::Editor(ed) => ed.color,
            _ => unreachable!(),
        };
        click(&mut screen, "color", &mut e);
        let Screen::Editor(ed) = &screen else {
            unreachable!()
        };
        assert_eq!(ed.x, "-50");
        assert!(ed.global && !ed.enabled);
        assert_eq!(ed.color, (before + 1) % 16);
        let wp = ed.waypoint([1, 2, 3]).unwrap();
        assert_eq!(wp.pos, [-50, 2, 3]);
        assert!(wp.disabled && wp.global);
        assert_eq!(
            click(&mut screen, "cancel", &mut e),
            Action::Open(Screen::List(List::default()))
        );
    }

    #[test]
    fn list_selects_teleports_and_deletes() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut a = Waypoint::new("Alpha", [1, 2, 3], 4);
        a.id = 7;
        let mut b = Waypoint::new("Beta", [4, 5, 6], 5);
        b.id = 9;
        w.sets
            .get_mut(waypoint::DEFAULT_SET)
            .unwrap()
            .extend([a, b]);
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::List(List::default());
        let ws = screen.widgets(&e);
        assert!(
            ws.iter()
                .any(|w| w.id == "teleport" && w.kind == ui::Kind::Button { enabled: false })
        );
        click(&mut screen, "row:9", &mut e);
        assert_eq!(click(&mut screen, "teleport", &mut e), Action::Teleport(9));
        assert_eq!(screen.key("key.keyboard.t", &mut e), Action::Teleport(9));
        click(&mut screen, "delete", &mut e);
        assert!(e.waypoints_changed);
        assert!(e.waypoints.find(9).is_none());
        // Filter
        click(&mut screen, "filter", &mut e);
        for c in "zzz".chars() {
            screen.typed(c, &mut e);
        }
        assert!(!screen.widgets(&e).iter().any(|w| w.id.starts_with("row:")));
        assert_eq!(click(&mut screen, "done", &mut e), Action::Close);
    }

    fn list_with(names: &[(&str, u64, i32, u64)]) -> DimensionWaypoints {
        let mut w = DimensionWaypoints::default();
        for (name, id, x, created) in names {
            let mut p = Waypoint::new(*name, [*x, 64, 10], 4);
            p.id = *id;
            p.created = *created;
            w.sets.get_mut(waypoint::DEFAULT_SET).unwrap().push(p);
        }
        w
    }

    fn row_ids(screen: &Screen, e: &Env<'_>) -> Vec<u64> {
        screen
            .widgets(e)
            .iter()
            .filter_map(|w| w.id.strip_prefix("row:")?.parse().ok())
            .collect()
    }

    #[test]
    fn list_sorts_and_shows_distances() {
        let mut w = list_with(&[
            ("beta", 1, 140, 5),
            ("Alpha", 2, 1130, 9),
            ("gamma", 3, 131, 1),
        ]);
        let mut s = Settings::default();
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::List(List::default());
        assert_eq!(row_ids(&screen, &e), [2, 1, 3], "newest first");
        click(&mut screen, "sort", &mut e);
        assert!(e.settings_changed);
        assert_eq!(e.settings.waypoints.sort, WaypointSort::Name);
        assert_eq!(row_ids(&screen, &e), [2, 1, 3]);
        click(&mut screen, "sort", &mut e);
        assert_eq!(row_ids(&screen, &e), [3, 1, 2], "nearest first");
        // A distance on every row, from the player at x 130.
        let labels: Vec<String> = screen
            .widgets(&e)
            .into_iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    ui::Kind::Label {
                        align: Align::Right,
                        ..
                    }
                )
            })
            .map(|w| w.text)
            .collect();
        assert_eq!(labels, ["1m", "10m", "1000m"]);
        // Another world shown: no distances.
        e.places.world = 1;
        assert!(!screen.widgets(&e).iter().any(|w| matches!(
            w.kind,
            ui::Kind::Label {
                align: Align::Right,
                ..
            }
        )));
    }

    #[test]
    fn list_selects_several_and_acts_on_all() {
        let mut w = list_with(&[
            ("a", 1, 0, 4),
            ("b", 2, 0, 3),
            ("c", 3, 0, 2),
            ("d", 4, 0, 1),
        ]);
        let mut s = Settings::default();
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::List(List::default());
        click(&mut screen, "row:1", &mut e);
        screen.set_modifiers(true, false);
        click(&mut screen, "row:3", &mut e);
        let Screen::List(list) = &screen else {
            unreachable!()
        };
        assert_eq!(list.selected, [1, 2, 3], "Shift selects the range");
        screen.set_modifiers(false, true);
        click(&mut screen, "row:2", &mut e);
        screen.set_modifiers(false, false);
        let Screen::List(list) = &screen else {
            unreachable!()
        };
        assert_eq!(list.selected, [1, 3], "Ctrl takes one out");
        let enabled = |screen: &Screen, e: &Env<'_>, id: &str| {
            screen
                .widgets(e)
                .iter()
                .any(|w| w.id == id && w.kind == ui::Kind::Button { enabled: true })
        };
        assert!(
            !enabled(&screen, &e, "teleport"),
            "teleport needs one waypoint"
        );
        click(&mut screen, "toggle", &mut e);
        assert!(e.waypoints.find(1).unwrap().disabled && e.waypoints.find(3).unwrap().disabled);
        assert!(!e.waypoints.find(2).unwrap().disabled);
        click(&mut screen, "toggle", &mut e);
        assert!(!e.waypoints.find(1).unwrap().disabled);
        click(&mut screen, "delete", &mut e);
        assert_eq!(
            e.waypoints
                .current()
                .iter()
                .map(|w| w.id)
                .collect::<Vec<_>>(),
            [2, 4]
        );
        click(&mut screen, "row:4", &mut e);
        assert!(enabled(&screen, &e, "teleport"));
        assert_eq!(screen.key("key.keyboard.delete", &mut e), Action::None);
        assert!(e.waypoints.find(4).is_none());
    }

    #[test]
    fn list_creates_renames_and_deletes_sets() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::List(List::default());
        click(&mut screen, "new_set", &mut e);
        for c in "Nether hub".chars() {
            screen.typed(c, &mut e);
        }
        screen.key("key.keyboard.enter", &mut e);
        assert_eq!(e.waypoints.current_set, "Nether hub");
        assert!(e.waypoints_changed);
        click(&mut screen, "rename_set", &mut e);
        for _ in 0.."hub".len() {
            screen.key("key.keyboard.backspace", &mut e);
        }
        for c in "base".chars() {
            screen.typed(c, &mut e);
        }
        assert_eq!(screen.quick_confirm(&mut e), Action::None);
        assert_eq!(e.waypoints.current_set, "Nether base");
        assert!(e.waypoints.sets.contains_key(waypoint::DEFAULT_SET));
        // Deleting asks first; the quick confirm key answers.
        click(&mut screen, "delete_set", &mut e);
        assert!(e.waypoints.sets.contains_key("Nether base"));
        screen.quick_confirm(&mut e);
        assert!(!e.waypoints.sets.contains_key("Nether base"));
        assert_eq!(e.waypoints.current_set, waypoint::DEFAULT_SET);
        // A click elsewhere drops a name being typed.
        click(&mut screen, "new_set", &mut e);
        click(&mut screen, "filter", &mut e);
        assert!(!screen.widgets(&e).iter().any(|w| w.id == "set_name"));
    }

    #[test]
    fn editor_takes_an_unknown_height_and_quick_confirms() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        let mut editor = Editor::new(e.waypoints, 0, false);
        editor.name = "Far".into();
        editor.y = "~".into();
        let mut screen = Screen::Editor(editor);
        let Action::Save { waypoint, .. } = screen.quick_confirm(&mut e) else {
            panic!("the quick confirm key saves")
        };
        assert_eq!(waypoint.height(), None);
        assert_eq!(Editor::edit(&waypoint, "default", false).y, "~");
        // `~` can be typed into the Y field.
        let Screen::Editor(editor) = &mut screen else {
            unreachable!()
        };
        editor.y.clear();
        editor.focus = Some(Field::Y);
        screen.typed('~', &mut e);
        let Screen::Editor(editor) = &screen else {
            unreachable!()
        };
        assert_eq!(editor.y, "~");
    }

    #[test]
    fn clearing_a_set_asks_first() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        w.sets
            .get_mut(waypoint::DEFAULT_SET)
            .unwrap()
            .push(Waypoint::new("A", [0, 0, 0], 1));
        let mut e = env(&mut w, &mut s);
        let mut screen = Screen::List(List::default());
        click(&mut screen, "clear", &mut e);
        assert_eq!(e.waypoints.current().len(), 1);
        click(&mut screen, "clear", &mut e);
        assert!(e.waypoints.current().is_empty());
    }

    #[test]
    fn every_option_cycles_back_to_its_start() {
        for o in options() {
            let mut s = Settings::default();
            let start = (o.value)(&s);
            (o.step)(&mut s, false);
            (o.step)(&mut s, true);
            assert_eq!((o.value)(&s), start, "{}", o.label);
        }
    }

    #[test]
    fn seed_atlas_options_match_the_fabric_labels() {
        let opts = options();
        let find = |label: &str| opts.iter().find(|o| o.label == label).unwrap();
        let mut s = Settings::default();
        assert_eq!((find("Completed").value)(&s), "100%");
        assert_eq!((find("Marker size").value)(&s), "150%");
        assert_eq!((find("Overlay explored").value)(&s), "OFF");
        assert_eq!((find("Biome highlighting").value)(&s), "OFF");
        assert_eq!((find("World").value)(&s), "Normal, Auto (26.3)");

        // Completed steps down to Hidden and wraps to 100 %.
        let completed = find("Completed");
        (completed.step)(&mut s, true);
        assert_eq!((completed.value)(&s), "90%");
        s.seed_atlas.completed_opacity = 10;
        (completed.step)(&mut s, true);
        assert_eq!((completed.value)(&s), "Hidden");
        assert!(s.seed_atlas.completed_hidden());
        (completed.step)(&mut s, false);
        assert_eq!((completed.value)(&s), "10%");
        // The legacy hide switch reads as hidden and is folded in.
        s.seed_atlas.hide_completed = true;
        assert_eq!((completed.value)(&s), "Hidden");
        (completed.step)(&mut s, false);
        assert!(!s.seed_atlas.hide_completed);

        // Marker size walks the Fabric steps both ways.
        let size = find("Marker size");
        (size.step)(&mut s, false);
        assert_eq!((size.value)(&s), "200%");
        (size.step)(&mut s, true);
        (size.step)(&mut s, true);
        assert_eq!((size.value)(&s), "100%");

        // The world option shows the type and the version of the open
        // world, and changes that version.
        s.seed_atlas.large_biomes = true;
        s.seed_atlas.context_key = "sp-w".into();
        let world = find("World");
        (world.step)(&mut s, false);
        assert_eq!((world.value)(&s), "Large Biomes, 26.3");
        (world.step)(&mut s, false);
        assert_eq!((world.value)(&s), "Large Biomes, 26.2");
        (world.step)(&mut s, true);
        (world.step)(&mut s, true);
        assert_eq!((world.value)(&s), "Large Biomes, Auto (26.3)");
    }

    #[test]
    fn minimap_snaps_to_the_nearest_corner() {
        let gui = [400.0, 300.0];
        let place = MinimapPlace {
            rect: [5.0, 5.0, 80.0, 80.0],
            margin: 5.0,
        };
        assert_eq!(
            place_minimap([25.0, 15.0], place, gui),
            (Corner::TopLeft, [20, 10])
        );
        assert_eq!(
            place_minimap([300.0, 5.0], place, gui),
            (Corner::TopRight, [15, 0])
        );
        assert_eq!(
            place_minimap([10.0, 200.0], place, gui),
            (Corner::BottomLeft, [5, 15])
        );
        assert_eq!(
            place_minimap([900.0, 900.0], place, gui),
            (Corner::BottomRight, [-5, -5]),
            "kept on screen"
        );
    }

    #[test]
    fn dragging_the_minimap_moves_it() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s);
        e.minimap = Some(MinimapPlace {
            rect: [5.0, 5.0, 80.0, 80.0],
            margin: 5.0,
        });
        let mut sc = Screen::Position(PositionScreen::default());
        sc.click([20.0, 20.0], false, &mut e);
        sc.drag([60.0, 30.0], &mut e);
        assert!(e.settings_changed);
        assert!(sc.dragging(), "saved only once the drag ends");
        assert_eq!(e.settings.minimap.offset, [40, 10]);
        sc.release();
        assert!(!sc.dragging());
        e.settings_changed = false;
        sc.drag([200.0, 200.0], &mut e);
        assert!(!e.settings_changed, "no drag after release");
    }
}

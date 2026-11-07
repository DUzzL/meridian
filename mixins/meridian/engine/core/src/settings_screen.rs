//! Meridian's settings: one screen for the minimap, the world map, the
//! radar, waypoints and Seed Atlas. Categories in a sidebar on the left; the
//! chosen one's options in a scrolling list on the right, each with a line
//! saying what it does and its control at the row's end; a search field
//! that looks through every category.
//!
//! The options themselves (value and change) are the catalogue in
//! [`crate::screens::options`]; this screen arranges them and adds what the
//! catalogue has no room for (sliders, the structure list, profiles,
//! buttons that open other screens).

use crate::config::Settings;
use crate::minimap::Align;
use crate::screens::{Action, Env, List, Opt, PositionScreen, ProfileOp, Screen, options};
use crate::screens_seedatlas::{
    BiomeHighlightScreen, ConfirmResetScreen, NearestScreen, WorldGenerationScreen,
};
use crate::seedatlas::MARKERS;
use crate::ui::{self, Widget};

const WHITE: u32 = 0xFFFF_FFFF;
const GREY: u32 = 0xFFA0_A0A0;
const YELLOW: u32 = 0xFFFF_FF55;
const PAD: f32 = 8.0;
const TOP: f32 = 30.0;
const ROW_H: f32 = 24.0;
const ROW_STEP: f32 = 25.0;

/// The sidebar's categories, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    General,
    Minimap,
    Information,
    Radar,
    Waypoints,
    Terrain,
    Overlays,
    WorldMap,
    SeedAtlas,
    Structures,
    Performance,
}

impl Category {
    pub const ALL: [Category; 11] = [
        Category::General,
        Category::Minimap,
        Category::Information,
        Category::Radar,
        Category::Waypoints,
        Category::Terrain,
        Category::Overlays,
        Category::WorldMap,
        Category::SeedAtlas,
        Category::Structures,
        Category::Performance,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Category::General => "General",
            Category::Minimap => "Minimap",
            Category::Information => "Information",
            Category::Radar => "Entity Radar",
            Category::Waypoints => "Waypoints",
            Category::Terrain => "Terrain",
            Category::Overlays => "Overlays",
            Category::WorldMap => "World Map",
            Category::SeedAtlas => "Seed Atlas",
            Category::Structures => "Structures",
            Category::Performance => "Performance",
        }
    }
}

/// How a row is changed.
#[derive(Clone, Copy)]
enum Control {
    /// An option of the catalogue, found by its label: left click next,
    /// right click previous.
    Opt(&'static str),
    /// A switch outside the catalogue.
    Switch(fn(&Settings) -> bool, fn(&mut Settings, bool)),
    /// A slider: its value 0..=1, the text on it, and how a value is set.
    Slider(fn(&Settings) -> f32, fn(&Settings) -> String, fn(&mut Settings, f32)),
    /// A text field: the value as text, and how typed text is set (false
    /// when it is not valid: the old value stays). Enter or a click
    /// elsewhere sets it, Escape drops it.
    Text(fn(&Settings) -> String, fn(&mut Settings, &str) -> bool),
    /// A button with this id and its text.
    Button(&'static str, fn(&Env<'_>) -> String),
    /// A structure kind: on/off, its completed count, a reset button.
    Marker(&'static str),
    /// Text only.
    Info(fn(&Env<'_>) -> String),
}

struct Row {
    label: String,
    desc: &'static str,
    control: Control,
    category: Category,
}

fn row(category: Category, label: &str, desc: &'static str, control: Control) -> Row {
    Row {
        label: label.to_owned(),
        desc,
        control,
        category,
    }
}

/// An option of the catalogue under a display label.
fn opt(category: Category, label: &'static str, desc: &'static str) -> Row {
    row(category, display_label(label), desc, Control::Opt(label))
}

/// The catalogue's labels as this screen shows them.
fn display_label(label: &str) -> &str {
    match label {
        "Fair Play (No Radar, No Caves)" => "Fair Play",
        "Seed map layer" => "Seed Map Layer",
        "Overlay explored" => "Over Explored Terrain",
        "Biome highlighting" => "Biome Highlighting",
        "Biome resolution" => "Biome Resolution",
        "Worker threads" => "Worker Threads",
        "Background prefetch" => "Background Prefetch",
        "World type" => "World Type",
        "Marker size" => "Marker Size",
        "Max WP Draw Distance" => "Waypoint Draw Distance",
        other => other,
    }
}

fn pct_text(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as u32)
}

/// A 0..=100 percentage as a slider's 0..=1.
fn pct(v: u8) -> f32 {
    f32::from(v.min(100)) / 100.0
}

/// A slider's 0..=1 as a 0..=100 percentage.
fn to_pct(v: f32) -> u8 {
    (v * 100.0).round() as u8
}

/// A scale from 0.5 to 3.0 in steps of 0.1 as a slider's 0..=1, and back.
fn scale_value(v: f32) -> f32 {
    ((v - 0.5) / 2.5).clamp(0.0, 1.0)
}

fn to_scale(v: f32) -> f32 {
    ((0.5 + v * 2.5) * 10.0).round() / 10.0
}

/// A whole number from `text` within `range`.
fn number<T: std::str::FromStr + PartialOrd>(
    text: &str,
    range: std::ops::RangeInclusive<T>,
) -> Option<T> {
    text.trim().parse().ok().filter(|v| range.contains(v))
}

/// A command format: it must be a command, or the line would go to the chat.
fn command(text: &str) -> Option<String> {
    let t = text.trim();
    (t.len() > 1 && t.starts_with('/')).then(|| t.to_owned())
}

/// Dimension ids as the cave mode list shows them: short, comma separated.
fn dimension_list(ids: &[String]) -> String {
    ids.iter()
        .map(|d| d.strip_prefix("minecraft:").unwrap_or(d))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A typed dimension list (`overworld, the_nether`); empty means all.
fn parse_dimensions(text: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for id in text.split([',', ' ']).map(str::trim).filter(|d| !d.is_empty()) {
        if !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-./:".contains(c))
        {
            return None;
        }
        let id = if id.contains(':') {
            id.to_owned()
        } else {
            format!("minecraft:{id}")
        };
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Some(out)
}

/// Every row of a category (a table: one row per line).
#[rustfmt::skip]
fn rows(c: Category) -> Vec<Row> {
    use Category::*;
    use Control::*;
    match c {
        General => vec![
            row(General, "Minimap", "Shows the minimap in the corner of the screen.", Switch(|s| s.minimap.enabled, |s, v| s.minimap.enabled = v)),
            row(General, "Minimap Position", "Drag the minimap to where it should be; it snaps to the nearest corner.", Button("position", |_| "Change...".into())),
            row(General, "Waypoints", "Lists, edits and shares the waypoints of this world.", Button("waypoints", |_| "Open...".into())),
            opt(General, "Fair Play (No Radar, No Caves)", "Turns off the entity radar and cave maps, as servers that forbid them expect."),
            row(General, "Profile", "Settings profile in use. Click for the next one, right click for the previous one.", Button("profile", |e| profile_title(&e.profile))),
            row(General, "New Profile", "Copies the current settings into a new profile and switches to it.", Button("new_profile", |_| "Create".into())),
            row(General, "Rename Profile", "Gives the current profile a new name.", Button("rename_profile", |e| e.profile.clone())),
            row(General, "Delete Profile", "Removes the current profile (the last one stays).", Button("delete_profile", |_| "Delete".into())),
        ],
        Minimap => vec![
            opt(Minimap, "Minimap Shape", "Square or round."),
            opt(Minimap, "Minimap Size", "Auto follows the GUI scale and window size."),
            opt(Minimap, "Zoom", "How close the minimap shows the terrain. The zoom keys change it too."),
            opt(Minimap, "Lock North", "Keeps north up and turns the player arrow instead of the map."),
            opt(Minimap, "Opacity", "How see-through the minimap is."),
            opt(Minimap, "Position", "The corner the minimap sits in."),
            opt(Minimap, "Enlarge", "Whether the enlarge key (Z) works while held or toggles."),
            opt(Minimap, "Center When Enlarged", "Moves the enlarged minimap to the middle of the screen."),
            opt(Minimap, "Further Zoom Out", "Adds zoom steps further out."),
            opt(Minimap, "Hide Under Screens", "Hides the minimap while a menu or inventory is open."),
            opt(Minimap, "Hide Under F3", "Hides the minimap while the debug screen is shown."),
            row(Minimap, "Arrow Scale", "Size of your arrow on the minimap.", Slider(|s| scale_value(s.minimap.arrow_scale), |s| format!("{:.1}x", s.minimap.arrow_scale), |s, v| s.minimap.arrow_scale = to_scale(v))),
            row(Minimap, "Undiscovered Opacity", "How dark ground nobody explored yet shows on the minimap.", Slider(|s| pct(s.minimap.undiscovered_opacity), |s| pct_text(pct(s.minimap.undiscovered_opacity)), |s, v| s.minimap.undiscovered_opacity = to_pct(v))),
        ],
        Information => vec![
            opt(Information, "Coordinates", "X, Y and Z under the minimap."),
            opt(Information, "Overworld Coordinates", "In the Nether, also where you would be in the Overworld."),
            opt(Information, "Chunk Coordinates", "The chunk and the position in it."),
            opt(Information, "Angles", "Which way you look (yaw and pitch)."),
            opt(Information, "Dimension", "The dimension's name."),
            opt(Information, "Biome", "The biome you stand in."),
            opt(Information, "Weather", "Clear, rain or thunder."),
            opt(Information, "Light Level", "Block light, sky light or both where you stand."),
            opt(Information, "Game Time", "The time of day in the game."),
            opt(Information, "Real Time", "Your computer's clock."),
        ],
        Radar => vec![
            opt(Radar, "Entity Radar", "Shows the entities around you on the minimap and the world map."),
            row(Radar, "Icons", "Faces instead of dots: never, while the faces key (Tab) is held, or always.", Button("icons", |e| icons_label(e.settings).into())),
            opt(Radar, "Entity Names", "Whose names are written under their mark."),
            opt(Radar, "Display Players", "Other players, white."),
            opt(Radar, "Display Hostile Mobs", "Monsters, yellow."),
            opt(Radar, "Display Friendly Mobs", "Animals and villagers, yellow."),
            opt(Radar, "Display Items", "Dropped items, red."),
            opt(Radar, "Display Other Entities", "Boats, minecarts, armor stands, purple."),
            opt(Radar, "Height Limit", "Entities further above or below are left out (players up to 2050 blocks)."),
            opt(Radar, "Height-Based Fade", "Marks get darker the further above or below you they are."),
            opt(Radar, "Dot Size", "Size of the dots."),
            opt(Radar, "Hide Invisible Entities", "Leaves out entities that are invisible."),
            opt(Radar, "Smooth Entity Dots", "Moves marks smoothly between game ticks instead of tick by tick."),
            opt(Radar, "Reverse Filter", "Shows what the filters hide and hides what they show."),
            row(Radar, "Name Scale", "Size of the names under the marks.", Slider(|s| scale_value(s.radar.name_scale), |s| format!("{:.1}x", s.radar.name_scale), |s, v| s.radar.name_scale = to_scale(v))),
            row(Radar, "Max Entities", "Entities shown at most; the nearest ones stay.", Text(|s| s.radar.max_entities.to_string(), |s, t| number(t, 1..=100_000).map(|v| s.radar.max_entities = v).is_some())),
        ],
        Waypoints => vec![
            opt(Waypoints, "Waypoints In World", "Names and distances of waypoints in the world."),
            opt(Waypoints, "Waypoints On Minimap", "Waypoints on the minimap and the world map."),
            opt(Waypoints, "Display All Sets", "Waypoints of every set, not only the current one."),
            opt(Waypoints, "Waypoint Beam", "A beam of light over waypoints far away."),
            opt(Waypoints, "Max WP Draw Distance", "Waypoints further away are not drawn in the world."),
            opt(Waypoints, "Deathpoints", "A waypoint where you died."),
            opt(Waypoints, "Keep Old Deathpoints", "Keeps earlier deathpoints when you die again."),
            opt(Waypoints, "Delete Reached Deathpoints", "Removes a deathpoint once you get back to it."),
            row(Waypoints, "Waypoint Opacity In World", "How see-through waypoints in the world are.", Slider(|s| pct(s.waypoints.opacity_in_world), |s| pct_text(pct(s.waypoints.opacity_in_world)), |s, v| s.waypoints.opacity_in_world = to_pct(v))),
            row(Waypoints, "Waypoint Opacity On Minimap", "How see-through waypoints on the minimap are.", Slider(|s| pct(s.waypoints.opacity_on_minimap), |s| pct_text(pct(s.waypoints.opacity_on_minimap)), |s, v| s.waypoints.opacity_on_minimap = to_pct(v))),
            row(Waypoints, "Distance In km From", "Distances from this many blocks on are shown in km; 0 never.", Text(|s| s.waypoints.distance_in_km_from.to_string(), |s, t| number(t, 0..=u32::MAX).map(|v| s.waypoints.distance_in_km_from = v).is_some())),
            row(Waypoints, "Waypoint Teleport Command", "The command Teleport sends: {x} {y} {z} are the waypoint's coordinates.", Text(|s| s.waypoints.teleport_format.clone(), |s, t| command(t).map(|c| s.waypoints.teleport_format = c).is_some())),
        ],
        Terrain => vec![
            opt(Terrain, "Block Colors", "Accurate uses the block textures and biome tints; Vanilla the colours of paper maps."),
            opt(Terrain, "Biome Colors In Vanilla Mode", "Biome tints for grass, leaves and water with Vanilla colours."),
            opt(Terrain, "Terrain Slopes", "Shading of slopes."),
            opt(Terrain, "Terrain Depth", "Lower ground is drawn darker."),
            opt(Terrain, "Lighting", "Night and block light on the minimap and the world map."),
            opt(Terrain, "Biome Blending", "Smooth biome tints at biome borders."),
            opt(Terrain, "Block Transparency", "Water and glass show what is below them."),
            opt(Terrain, "Display Flowers", "Flowers and small plants on the maps."),
            opt(Terrain, "Display Redstone", "Redstone dust, repeaters and the like on the maps."),
            opt(Terrain, "Display Stained Glass", "Stained glass on the maps."),
            opt(Terrain, "Adjust Height For Short Blocks", "Slabs, carpets and the like count as lower than full blocks."),
        ],
        Overlays => vec![
            opt(Overlays, "Chunk Grid", "Chunk borders on the minimap."),
            opt(Overlays, "Slime Chunks", "Chunks where slimes spawn, in green (needs the seed)."),
            opt(Overlays, "Light Overlay", "Marks blocks dark enough for monsters to spawn."),
            opt(Overlays, "Light Overlay Min", "The lowest light level the overlay marks."),
            opt(Overlays, "Light Overlay Max", "The highest light level the overlay marks."),
            opt(Overlays, "Cave Mode", "Allows cave maps when you are underground."),
            opt(Overlays, "Auto Cave Mode", "How large the roof above you must be before the cave map shows."),
            opt(Overlays, "Cave Mode Toggle Delay", "How long you must be under a roof (or out) before the map switches."),
            opt(Overlays, "Legible Cave Maps", "Brighter cave maps that are easier to read."),
            opt(Overlays, "Cave Mode Extra Zoom", "Zooms the minimap in further in caves."),
            opt(Overlays, "Cave Mode Depth", "How far below where you are the cave map shows the ground."),
            row(Overlays, "Cave Mode Dimensions", "Dimensions where cave maps may show, comma separated (overworld, the_nether); empty for all.", Text(|s| dimension_list(&s.minimap.cave.allowed_dimensions), |s, t| parse_dimensions(t).map(|d| s.minimap.cave.allowed_dimensions = d).is_some())),
        ],
        WorldMap => vec![
            opt(WorldMap, "World Map", "The world map (M)."),
            opt(WorldMap, "Load New Chunks", "Adds chunks you load to the map."),
            opt(WorldMap, "Update Chunks", "Redraws chunks that change."),
            opt(WorldMap, "Display Coordinates", "The coordinates under the cursor at the top."),
            opt(WorldMap, "Display Hovered Biome", "The biome under the cursor at the top."),
            opt(WorldMap, "Display Zoom", "The zoom at the bottom."),
            opt(WorldMap, "Player As Arrow", "You as an arrow pointing where you look, or as a dot."),
            opt(WorldMap, "Footsteps", "The path you walked recently."),
            opt(WorldMap, "Opening Animation", "Zooms out from your position when the map opens."),
            opt(WorldMap, "Unlimited Zoom In", "Zoom in further than 1 block a pixel steps."),
            opt(WorldMap, "Unlimited Zoom Out", "Zoom out to 1/64."),
            opt(WorldMap, "Multiplayer Maps", "How the maps of a server with several worlds are kept apart."),
            row(WorldMap, "Map Teleport Command", "The command Teleport Here sends: {x} {y} {z} are the coordinates.", Text(|s| s.world_map.map_teleport_format.clone(), |s, t| command(t).map(|c| s.world_map.map_teleport_format = c).is_some())),
            row(WorldMap, "Map Teleport Command (Dimension)", "Teleport Here on another dimension's map; {d} is the dimension.", Text(|s| s.world_map.map_teleport_dimension_format.clone(), |s, t| command(t).map(|c| s.world_map.map_teleport_dimension_format = c).is_some())),
            row(WorldMap, "Player Teleport Command", "The command Teleport To sends for another player; {name} is their name.", Text(|s| s.world_map.player_teleport_format.clone(), |s, t| command(t).map(|c| s.world_map.player_teleport_format = c).is_some())),
        ],
        SeedAtlas => vec![
            row(SeedAtlas, "Seed", "The seed the Seed Atlas uses for this world or server.", Info(|e| e.seed_atlas.seed_status.clone())),
            opt(SeedAtlas, "Seed Atlas", "Computes biomes and structures from the world's seed."),
            opt(SeedAtlas, "Seed map layer", "The computed biome map under the explored terrain."),
            row(SeedAtlas, "Layer Opacity", "How strongly the computed map shows.", Slider(|s| s.seed_atlas.biome_opacity as f32 / 255.0, |s| pct_text(s.seed_atlas.biome_opacity as f32 / 255.0), |s, v| s.seed_atlas.biome_opacity = (v * 255.0).round() as u8)),
            opt(SeedAtlas, "Overlay explored", "Draws the computed map over terrain you explored too."),
            row(SeedAtlas, "Terrain Look", "Relief and surface colours instead of flat biome colours (1.18 and later).", Switch(|s| s.seed_atlas.terrain_look, |s, v| s.seed_atlas.terrain_look = v)),
            row(SeedAtlas, "World & Seed", "The seed, game version and world type of this world or server.", Button("sa:world", |_| "Change...".into())),
            opt(SeedAtlas, "World", "Game version of the open world (the generation differs between versions)."),
            opt(SeedAtlas, "World type", "Normal or Large Biomes."),
            opt(SeedAtlas, "Overworld Y", "Height the Overworld's biomes are taken at."),
            opt(SeedAtlas, "Nether Y", "Height the Nether's biomes are taken at."),
            opt(SeedAtlas, "End Y", "Height the End's biomes are taken at."),
            opt(SeedAtlas, "Biome highlighting", "Dims every biome but the chosen ones."),
            row(SeedAtlas, "Highlighted Biomes", "Chooses the biomes biome highlighting keeps bright.", Button("sa:highlight", |_| "Choose...".into())),
            row(SeedAtlas, "Nearest Biome", "Finds the closest biome of a kind and shows it on the map.", Button("sa:biome", |_| "Find...".into())),
            row(SeedAtlas, "Nearest Structure", "Finds the closest structure of a kind and shows it on the map.", Button("sa:structure", |_| "Find...".into())),
        ],
        Structures => {
            let mut v = vec![
                opt(Structures, "Structures", "Structure markers on the world map."),
                opt(Structures, "Marker size", "Size of the structure icons."),
                row(Structures, "Completed", "Fades structures you marked as completed; 0% hides them.", Slider(|s| s.seed_atlas.completed_opacity_pct() as f32 / 100.0, |s| pct_text(s.seed_atlas.completed_opacity_pct() as f32 / 100.0), |s, v| {
                    s.seed_atlas.hide_completed = false;
                    s.seed_atlas.completed_opacity = (v * 100.0).round() as u8;
                })),
                row(Structures, "Reset Completed", "Marks every completed structure of this world or server as not completed.", Button("mk:reset", |e| format!("Reset ({})", e.seed_atlas.completed_count.unwrap_or(0)))),
            ];
            v.extend(MARKERS.iter().map(|m| row(Structures, m.name, "", Marker(m.id))));
            v
        }
        Performance => vec![
            opt(Performance, "Biome resolution", "The finest detail of the computed map; coarser is faster."),
            opt(Performance, "Worker threads", "Threads computing the Seed Atlas map. Auto leaves cores for the game."),
            opt(Performance, "Background prefetch", "Computes the map around the view in advance while the world map is open."),
            opt(Performance, "Max Loaded Regions", "Explored regions the world map keeps in memory."),
            opt(Performance, "Map Frame Budget", "Time per frame for drawing explored terrain; more fills the map faster."),
            opt(Performance, "Chunk Reads Per Tick", "Chunks read from the game per tick at most."),
            opt(Performance, "Region Save Interval", "How often changed regions are written to disk."),
            opt(Performance, "Map Worker Threads", "Threads building map tiles (after a restart). Auto takes half the cores, up to 4."),
        ],
    }
}

/// The radar's icon mode as one value.
fn icons_label(s: &Settings) -> &'static str {
    match (s.radar.faces, s.radar.faces_while_held) {
        (true, _) => "Always",
        (false, true) => "While Tab Held",
        (false, false) => "Never",
    }
}

fn step_icons(s: &mut Settings, back: bool) {
    // Never -> While Tab Held -> Always.
    let i = match icons_label(s) {
        "Never" => 0,
        "While Tab Held" => 1,
        _ => 2,
    };
    let next = if back { (i + 2) % 3 } else { (i + 1) % 3 };
    s.radar.faces = next == 2;
    s.radar.faces_while_held = next >= 1;
}

fn profile_title(name: &str) -> String {
    let mut c = name.chars();
    c.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// The end of `text` that fits `width` GUI pixels (a field being typed in
/// shows where the cursor is).
fn tail(text: &str, width: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() && ui::text_width(&chars[start..].iter().collect::<String>()) > width {
        start += 1;
    }
    chars[start..].iter().collect()
}

/// `text`, shortened with "..." to fit `width` GUI pixels.
fn fit(text: &str, width: f32) -> String {
    if ui::text_width(text) <= width {
        return text.to_owned();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}...");
        if ui::text_width(&candidate) > width {
            break;
        }
        out.push(ch);
    }
    format!("{}...", out.trim_end())
}

/// The settings screen.
#[derive(Debug, PartialEq)]
pub struct SettingsScreen {
    pub category: Category,
    pub search: String,
    pub search_focused: bool,
    /// The list's scroll, GUI pixels.
    pub scroll: f32,
    /// The profile name being typed.
    pub profile_name: Option<String>,
    /// The slider row being dragged.
    pub dragging: Option<usize>,
    /// The text field being typed in: its row's label and the text so far.
    pub editing: Option<(String, String)>,
    /// Opened from the world map: Done goes back to it.
    pub direct: bool,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        Self::at(Category::General)
    }
}

struct Layout {
    side: [f32; 4],
    content: [f32; 4],
    list_top: f32,
    list_bottom: f32,
    control_w: f32,
    entry_h: f32,
}

impl SettingsScreen {
    /// The settings opened on one category.
    pub fn at(category: Category) -> Self {
        Self {
            category,
            search: String::new(),
            search_focused: false,
            scroll: 0.0,
            profile_name: None,
            dragging: None,
            editing: None,
            direct: false,
        }
    }

    /// The world map's settings, as its key and button open them.
    pub fn world_map() -> Self {
        Self {
            direct: true,
            ..Self::at(Category::WorldMap)
        }
    }

    fn layout(env: &Env<'_>) -> Layout {
        let [w, h] = env.gui;
        let side_w = (w * 0.22).clamp(88.0, 130.0);
        let cx = PAD + side_w + 8.0;
        let cw = (w - cx - PAD - 6.0).max(120.0);
        let list_top = TOP + 18.0;
        let list_bottom = (h - 32.0).max(list_top + ROW_STEP);
        let n = Category::ALL.len() as f32;
        let entry_h = ((list_bottom - TOP) / n - 2.0).clamp(11.0, 18.0);
        Layout {
            side: [PAD, TOP, side_w, list_bottom - TOP],
            content: [cx, TOP, cw, list_bottom - TOP],
            list_top,
            list_bottom,
            control_w: (cw * 0.32).clamp(70.0, 140.0),
            entry_h,
        }
    }

    /// The rows shown now: the category's, or every match of the search.
    fn shown(&self) -> Vec<Row> {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return rows(self.category);
        }
        Category::ALL
            .iter()
            .flat_map(|c| rows(*c))
            .filter(|r| r.label.to_lowercase().contains(&q) || r.desc.to_lowercase().contains(&q))
            .collect()
    }

    fn max_scroll(&self, env: &Env<'_>) -> f32 {
        let l = Self::layout(env);
        let total = self.shown().len() as f32 * ROW_STEP;
        (total - (l.list_bottom - l.list_top)).max(0.0)
    }

    pub fn scroll(&mut self, lines: f32, env: &Env<'_>) {
        self.scroll = (self.scroll - lines * ROW_STEP).clamp(0.0, self.max_scroll(env));
    }

    /// The control's rectangle in row `i` (scrolled), if it is fully shown.
    fn row_rect(&self, env: &Env<'_>, i: usize) -> Option<[f32; 4]> {
        let l = Self::layout(env);
        let y = l.list_top + i as f32 * ROW_STEP - self.scroll;
        (y >= l.list_top - 0.5 && y + ROW_H <= l.list_bottom + 0.5)
            .then_some([l.content[0], y, l.content[2], ROW_H])
    }

    pub(crate) fn widgets(&self, env: &Env<'_>) -> Vec<Widget> {
        let l = Self::layout(env);
        let [w, h] = env.gui;
        let mut ws = vec![
            Widget::fill([l.side[0] - 2.0, l.side[1] - 2.0, l.side[2] + 4.0, l.side[3] + 4.0], 0x8000_0000),
            Widget::fill([l.content[0] - 2.0, l.content[1] - 2.0, l.content[2] + 10.0, l.content[3] + 4.0], 0x8000_0000),
            Widget::text(PAD, 11.0, "Meridian Settings", WHITE, Align::Left),
        ];
        // Sidebar.
        let searching = !self.search.trim().is_empty();
        for (i, c) in Category::ALL.iter().enumerate() {
            let y = TOP + i as f32 * (l.entry_h + 2.0);
            let rect = [l.side[0], y, l.side[2], l.entry_h];
            let selected = !searching && *c == self.category;
            ws.push(Widget::row(&format!("cat:{i}"), rect, selected));
            ws.push(Widget::text(
                rect[0] + 5.0,
                y + (l.entry_h - 8.0) / 2.0,
                fit(c.title(), l.side[2] - 8.0),
                if selected { YELLOW } else { WHITE },
                Align::Left,
            ));
        }
        // Header: the category and the search field.
        let field_w = (l.content[2] * 0.4).clamp(80.0, 160.0);
        ws.push(Widget::field(
            "search",
            [l.content[0] + l.content[2] - field_w, TOP - 1.0, field_w, 16.0],
            &self.search,
            "Search...",
            self.search_focused,
        ));
        let title = if searching {
            format!("Search: {}", self.search.trim())
        } else {
            self.category.title().to_owned()
        };
        ws.push(Widget::text(l.content[0] + 4.0, TOP + 3.0, fit(&title, l.content[2] - field_w - 12.0), YELLOW, Align::Left));
        // Rows.
        let rows = self.shown();
        if rows.is_empty() {
            ws.push(Widget::text(l.content[0] + 4.0, l.list_top + 6.0, "Nothing matches.", GREY, Align::Left));
        }
        for (i, r) in rows.iter().enumerate() {
            let Some(rect) = self.row_rect(env, i) else { continue };
            ws.extend(self.row_widgets(env, &l, i, r, rect, searching));
        }
        // Scrollbar.
        let max = self.max_scroll(env);
        if max > 0.0 {
            let track = [l.content[0] + l.content[2] + 2.0, l.list_top, 4.0, l.list_bottom - l.list_top];
            let visible = track[3] / (track[3] + max);
            let th = (track[3] * visible).max(12.0);
            let ty = track[1] + (track[3] - th) * (self.scroll / max);
            ws.push(Widget::fill(track, 0xFF20_2020));
            ws.push(Widget::fill([track[0], ty, track[2], th], 0xFF90_9090));
        }
        ws.push(Widget::button("done", [(w / 2.0 - 75.0).floor(), h - 26.0, 150.0, 20.0], "Done", true));
        ws
    }

    fn row_widgets(&self, env: &Env<'_>, l: &Layout, i: usize, r: &Row, rect: [f32; 4], searching: bool) -> Vec<Widget> {
        let [x, y, cw, _] = rect;
        // Text fields take more room: commands are long.
        let control_w = match r.control {
            Control::Text(..) => (cw * 0.5).clamp(l.control_w, (cw - 90.0).max(l.control_w)),
            _ => l.control_w,
        };
        let ctl = [x + cw - control_w - 2.0, y + 2.0, control_w, 20.0];
        let mut ws = vec![Widget::fill([x, y, cw, ROW_H], if i.is_multiple_of(2) { 0x3000_0000 } else { 0x1800_0000 })];
        let mut text_x = x + 4.0;
        let text_w = ctl[0] - text_x - 6.0;
        let id = format!("row:{i}");
        match r.control {
            Control::Opt(key) => {
                let value = options()
                    .into_iter()
                    .find(|o| o.label == key)
                    .map_or_else(String::new, |o: Opt| (o.value)(env.settings));
                ws.push(Widget::button(&id, ctl, fit(&value, ctl[2] - 8.0), true));
            }
            Control::Switch(get, _) => {
                ws.push(Widget::button(&id, ctl, if get(env.settings) { "ON" } else { "OFF" }, true));
            }
            Control::Slider(value, text, _) => {
                ws.push(Widget::slider(&id, ctl, text(env.settings), value(env.settings)));
            }
            Control::Text(get, _) => {
                let shown = match &self.editing {
                    Some((label, text)) if *label == r.label => Some(tail(text, ctl[2] - 14.0)),
                    _ => None,
                };
                let focused = shown.is_some();
                let text = shown.unwrap_or_else(|| fit(&get(env.settings), ctl[2] - 8.0));
                ws.push(Widget::field(&id, ctl, &text, "", focused));
            }
            Control::Button(action, text) => {
                let enabled = action != "delete_profile" || env.profiles.len() > 1;
                if action == "rename_profile"
                    && let Some(name) = &self.profile_name
                {
                    ws.push(Widget::field("profile_name", ctl, name, "Profile Name", true));
                } else {
                    ws.push(Widget::button(&id, ctl, fit(&text(env), ctl[2] - 8.0), enabled));
                }
            }
            Control::Marker(kind) => {
                let on = crate::seedatlas::marker_enabled(&env.settings.seed_atlas, kind);
                ws.push(Widget::icon(kind, [x + 4.0, y + 4.0, 16.0, 16.0], !on));
                text_x += 22.0;
                let reset_w = 46.0;
                let toggle = [ctl[0] + reset_w + 4.0, ctl[1], ctl[2] - reset_w - 4.0, 20.0];
                let count = env.seed_atlas.completed_by_kind.get(kind).copied().unwrap_or(0);
                ws.push(Widget::button(&format!("mk:rs:{kind}"), [ctl[0], ctl[1], reset_w, 20.0], "Reset", count > 0));
                ws.push(Widget::button(&id, toggle, if on { "ON" } else { "OFF" }, true));
            }
            Control::Info(text) => {
                ws.push(Widget::text(x + 4.0, y + 3.0, fit(&r.label, cw - 8.0), WHITE, Align::Left));
                ws.push(Widget::text(x + 4.0, y + 13.0, fit(&text(env), cw - 8.0), GREY, Align::Left));
                return ws;
            }
        }
        let desc = match r.control {
            Control::Marker(kind) => {
                let count = env.seed_atlas.completed_by_kind.get(kind).copied().unwrap_or(0);
                if count > 0 { format!("{count} completed") } else { String::new() }
            }
            _ if searching => format!("{} · {}", r.category.title(), r.desc),
            _ => r.desc.to_owned(),
        };
        let text_w = text_w - (text_x - x - 4.0);
        let label_y = if desc.is_empty() { y + 8.0 } else { y + 3.0 };
        ws.push(Widget::text(text_x, label_y, fit(&r.label, text_w), WHITE, Align::Left));
        if !desc.is_empty() {
            ws.push(Widget::text(text_x, y + 13.0, fit(&desc, text_w), GREY, Align::Left));
        }
        ws
    }

    /// The full description of the row under the cursor (rows shorten it).
    pub(crate) fn tooltip(&self, id: &str, _env: &Env<'_>) -> Option<String> {
        let i: usize = id.strip_prefix("row:")?.parse().ok()?;
        let r = self.shown().into_iter().nth(i)?;
        (!r.desc.is_empty()).then(|| r.desc.to_owned())
    }

    fn slider_value(rect: [f32; 4], x: f32) -> f32 {
        ((x - (rect[0] + 4.0)) / (rect[2] - 8.0)).clamp(0.0, 1.0)
    }

    fn control_rect(&self, env: &Env<'_>, i: usize) -> Option<[f32; 4]> {
        let l = Self::layout(env);
        let [x, y, cw, _] = self.row_rect(env, i)?;
        Some([x + cw - l.control_w - 2.0, y + 2.0, l.control_w, 20.0])
    }

    pub(crate) fn click(&mut self, id: Option<&str>, at: [f32; 2], right: bool, env: &mut Env<'_>) -> Action {
        self.search_focused = false;
        // A click anywhere but the field being typed in sets its value.
        let row = id
            .and_then(|id| id.strip_prefix("row:")?.parse::<usize>().ok())
            .and_then(|i| self.shown().into_iter().nth(i));
        if self
            .editing
            .as_ref()
            .is_some_and(|(label, _)| row.as_ref().is_none_or(|r| r.label != *label))
        {
            self.commit(env);
        }
        if id != Some("profile_name")
            && self.profile_name.is_some()
            && let Some(op) = self.confirm_name(env)
        {
            return Action::Profile(op);
        }
        let Some(id) = id else { return Action::None };
        if id == "search" {
            self.search_focused = true;
            return Action::None;
        }
        if id == "done" {
            return Action::Close;
        }
        if let Some(i) = id.strip_prefix("cat:").and_then(|i| i.parse::<usize>().ok()) {
            if let Some(c) = Category::ALL.get(i) {
                self.category = *c;
                self.search.clear();
                self.scroll = 0.0;
            }
            return Action::None;
        }
        if let Some(kind) = id.strip_prefix("mk:rs:") {
            return Action::Open(Screen::ConfirmReset(ConfirmResetScreen::new(env, Some(kind))));
        }
        let Some(i) = id.strip_prefix("row:").and_then(|i| i.parse::<usize>().ok()) else {
            return Action::None;
        };
        let Some(r) = self.shown().into_iter().nth(i) else { return Action::None };
        match r.control {
            Control::Opt(key) => {
                if let Some(o) = options().into_iter().find(|o| o.label == key) {
                    (o.step)(env.settings, right);
                    env.settings_changed = true;
                }
            }
            Control::Switch(get, set) => {
                let v = get(env.settings);
                set(env.settings, !v);
                env.settings_changed = true;
            }
            Control::Slider(_, _, set) => {
                if let Some(rect) = self.control_rect(env, i) {
                    set(env.settings, Self::slider_value(rect, at[0]));
                    env.settings_changed = true;
                    self.dragging = Some(i);
                }
            }
            Control::Marker(kind) => {
                let on = crate::seedatlas::marker_enabled(&env.settings.seed_atlas, kind);
                env.settings.seed_atlas.markers.insert(kind.to_owned(), !on);
                env.settings_changed = true;
            }
            Control::Info(_) => {}
            Control::Text(get, _) => {
                if self.editing.is_none() {
                    self.editing = Some((r.label.clone(), get(env.settings)));
                }
            }
            Control::Button(action, _) => return self.button(action, right, env),
        }
        Action::None
    }

    fn button(&mut self, action: &str, right: bool, env: &mut Env<'_>) -> Action {
        let open = |s: Screen| Action::Open(s);
        match action {
            "position" => open(Screen::Position(PositionScreen::default())),
            "waypoints" => open(Screen::List(List::default())),
            "icons" => {
                step_icons(env.settings, right);
                env.settings_changed = true;
                Action::None
            }
            "profile" if !env.profiles.is_empty() => {
                let i = env.profiles.iter().position(|p| *p == env.profile).unwrap_or(0);
                let n = env.profiles.len();
                let next = if right { (i + n - 1) % n } else { (i + 1) % n };
                Action::Profile(ProfileOp::Select(env.profiles[next].clone()))
            }
            "new_profile" => {
                let name = (2..)
                    .map(|i| format!("profile{i}"))
                    .find(|n| !env.profiles.contains(n))
                    .expect("a free name");
                Action::Profile(ProfileOp::Create(name))
            }
            "rename_profile" => {
                self.profile_name = Some(env.profile.clone());
                Action::None
            }
            "delete_profile" if env.profiles.len() > 1 => Action::Profile(ProfileOp::Delete(env.profile.clone())),
            "mk:reset" => open(Screen::ConfirmReset(ConfirmResetScreen::new(env, None))),
            "sa:world" => open(Screen::WorldGeneration(WorldGenerationScreen::new(env))),
            "sa:highlight" => open(Screen::BiomeHighlight(BiomeHighlightScreen::new())),
            "sa:biome" => open(Screen::Nearest(NearestScreen::biome(env))),
            "sa:structure" => open(Screen::Nearest(NearestScreen::structure(env))),
            _ => Action::None,
        }
    }

    pub(crate) fn drag(&mut self, at: [f32; 2], env: &mut Env<'_>) {
        let Some(i) = self.dragging else { return };
        let Some(rect) = self.control_rect(env, i) else { return };
        if let Some(Row { control: Control::Slider(_, _, set), .. }) = self.shown().into_iter().nth(i) {
            set(env.settings, Self::slider_value(rect, at[0]));
            env.settings_changed = true;
        }
    }

    pub(crate) fn release(&mut self) {
        self.dragging = None;
    }

    /// Sets the typed text, if it is valid and differs.
    fn commit(&mut self, env: &mut Env<'_>) {
        let Some((label, text)) = self.editing.take() else { return };
        let row = Category::ALL.iter().flat_map(|c| rows(*c)).find(|r| r.label == label);
        if let Some(Row { control: Control::Text(get, set), .. }) = row
            && get(env.settings) != text
            && set(env.settings, &text)
        {
            env.settings_changed = true;
        }
    }

    pub(crate) fn typed(&mut self, c: char) {
        if let Some((_, text)) = &mut self.editing {
            ui::type_char(text, c, 120, |_| true);
            return;
        }
        if let Some(name) = &mut self.profile_name {
            ui::type_char(name, c, 24, |c| c.is_alphanumeric() || "_- ".contains(c));
            return;
        }
        if self.search_focused && ui::type_char(&mut self.search, c, 24, |_| true) {
            self.scroll = 0.0;
        }
    }

    fn confirm_name(&mut self, env: &Env<'_>) -> Option<ProfileOp> {
        let name = self.profile_name.take()?;
        let name = name.trim();
        (!name.is_empty() && name != env.profile && !env.profiles.iter().any(|p| p == name))
            .then(|| ProfileOp::Rename(name.to_owned()))
    }

    pub(crate) fn key(&mut self, key: &str, env: &mut Env<'_>) -> Action {
        if let Some((_, text)) = &mut self.editing {
            match key {
                "key.keyboard.backspace" => {
                    ui::backspace(text);
                }
                "key.keyboard.enter" | "key.keyboard.keypad.enter" => self.commit(env),
                "key.keyboard.escape" => self.editing = None,
                _ => {}
            }
            return Action::None;
        }
        if let Some(name) = &mut self.profile_name {
            match key {
                "key.keyboard.backspace" => {
                    ui::backspace(name);
                }
                "key.keyboard.enter" | "key.keyboard.keypad.enter" => {
                    if let Some(op) = self.confirm_name(env) {
                        return Action::Profile(op);
                    }
                }
                "key.keyboard.escape" => self.profile_name = None,
                _ => {}
            }
            return Action::None;
        }
        if self.search_focused && key == "key.keyboard.backspace" {
            ui::backspace(&mut self.search);
            self.scroll = 0.0;
        }
        Action::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens_seedatlas::SeedAtlasEnv;
    use crate::waypoint::DimensionWaypoints;

    fn env<'a>(w: &'a mut DimensionWaypoints, s: &'a mut Settings, gui: [f32; 2]) -> Env<'a> {
        Env {
            gui,
            mouse: [-1.0, -1.0],
            world_label: "Test (auto)",
            dimension_label: "overworld (auto)",
            player_block: [0, 64, 0],
            waypoints: w,
            settings: s,
            waypoints_changed: false,
            settings_changed: false,
            seed_atlas: SeedAtlasEnv::default(),
            profiles: vec!["default".into(), "pvp".into()],
            profile: "default".into(),
            minimap: None,
            places: Default::default(),
        }
    }

    #[test]
    fn every_catalogue_option_has_a_row() {
        let shown: Vec<&'static str> = Category::ALL
            .iter()
            .flat_map(|c| rows(*c))
            .filter_map(|r| match r.control {
                Control::Opt(k) => Some(k),
                _ => None,
            })
            .collect();
        let missing: Vec<_> = options()
            .iter()
            .map(|o| o.label)
            // Folded into the "Icons" row.
            .filter(|l| !["Mob Faces", "Faces While Tab Held", "Completed", "Layer opacity"].contains(l))
            // Structure kinds have their own rows.
            .filter(|l| !MARKERS.iter().any(|m| m.name == *l))
            .filter(|l| !shown.contains(l))
            .collect();
        assert!(missing.is_empty(), "options without a row: {missing:?}");
        for k in &shown {
            assert!(options().iter().any(|o| o.label == *k), "row for unknown option {k}");
        }
        assert!(Category::ALL.iter().flat_map(|c| rows(*c)).all(|r| !r.desc.is_empty() || matches!(r.control, Control::Marker(_))));
    }

    #[test]
    fn categories_rows_and_clicks() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s, [854.0, 480.0]);
        let mut sc = SettingsScreen::default();
        // Sidebar: switch to the radar.
        let radar = Category::ALL.iter().position(|c| *c == Category::Radar).unwrap();
        sc.click(Some(&format!("cat:{radar}")), [0.0, 0.0], false, &mut e);
        assert_eq!(sc.category, Category::Radar);
        // The icons row cycles Never -> Tab -> Always.
        let icons = rows(Category::Radar).iter().position(|r| r.label == "Icons").unwrap();
        assert_eq!(icons_label(e.settings), "While Tab Held");
        sc.click(Some(&format!("row:{icons}")), [0.0, 0.0], false, &mut e);
        assert_eq!(icons_label(e.settings), "Always");
        sc.click(Some(&format!("row:{icons}")), [0.0, 0.0], false, &mut e);
        assert_eq!(icons_label(e.settings), "Never");
        assert!(e.settings_changed);
        // A catalogue option toggles.
        let first = rows(Category::Radar).iter().position(|r| r.label == "Entity Radar").unwrap();
        let before = e.settings.radar.enabled;
        sc.click(Some(&format!("row:{first}")), [0.0, 0.0], false, &mut e);
        assert_ne!(e.settings.radar.enabled, before);
        // Every shown control lies inside the screen.
        for c in Category::ALL {
            sc.category = c;
            for wd in sc.widgets(&e) {
                assert!(wd.rect[0] >= 0.0 && wd.rect[0] + wd.rect[2] <= 854.0 + 0.5, "{c:?} {wd:?}");
            }
        }
    }

    #[test]
    fn search_scroll_and_slider() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s, [427.0, 240.0]);
        let mut sc = SettingsScreen::at(Category::Structures);
        // 4 rows and 28 kinds do not fit: the list scrolls.
        assert!(sc.max_scroll(&e) > 0.0);
        sc.scroll(-3.0, &e);
        assert!(sc.scroll > 0.0);
        sc.scroll(100.0, &e);
        assert_eq!(sc.scroll, 0.0);
        // The completed slider sets the value under the cursor.
        let i = rows(Category::Structures).iter().position(|r| r.label == "Completed").unwrap();
        let rect = sc.control_rect(&e, i).unwrap();
        sc.click(Some(&format!("row:{i}")), [rect[0] + 4.0, rect[1]], false, &mut e);
        assert_eq!(e.settings.seed_atlas.completed_opacity_pct(), 0);
        sc.drag([rect[0] + rect[2] - 4.0, rect[1]], &mut e);
        assert_eq!(e.settings.seed_atlas.completed_opacity_pct(), 100);
        assert!(sc.dragging.is_some(), "the host writes the file only on release");
        sc.release();
        assert!(sc.dragging.is_none());
        // A search looks through every category.
        sc.search_focused = true;
        for ch in "slime".chars() {
            sc.typed(ch);
        }
        let found: Vec<String> = sc.shown().into_iter().map(|r| r.label).collect();
        assert!(found.iter().any(|l| l == "Slime Chunks"), "{found:?}");
        // A structure kind switches off.
        sc.search.clear();
        let camp = rows(Category::Structures).iter().position(|r| matches!(r.control, Control::Marker("camp"))).unwrap();
        let on = crate::seedatlas::marker_enabled(&e.settings.seed_atlas, "camp");
        sc.click(Some(&format!("row:{camp}")), [0.0, 0.0], false, &mut e);
        assert_ne!(crate::seedatlas::marker_enabled(&e.settings.seed_atlas, "camp"), on);
    }

    #[test]
    fn text_fields_set_valid_values() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s, [854.0, 480.0]);
        let mut sc = SettingsScreen::at(Category::Radar);
        let i = rows(Category::Radar).iter().position(|r| r.label == "Max Entities").unwrap();
        let field = format!("row:{i}");
        let type_in = |sc: &mut SettingsScreen, e: &mut Env<'_>, text: &str| {
            sc.click(Some(&field), [0.0, 0.0], false, e);
            for _ in 0..10 {
                sc.key("key.keyboard.backspace", e);
            }
            text.chars().for_each(|c| sc.typed(c));
        };
        type_in(&mut sc, &mut e, "250");
        assert_eq!(e.settings.radar.max_entities, 1000, "not before Enter");
        sc.key("key.keyboard.enter", &mut e);
        assert_eq!(e.settings.radar.max_entities, 250);
        assert!(e.settings_changed && sc.editing.is_none());
        // Invalid text keeps the old value; Escape drops the text.
        type_in(&mut sc, &mut e, "lots");
        sc.key("key.keyboard.enter", &mut e);
        assert_eq!(e.settings.radar.max_entities, 250);
        type_in(&mut sc, &mut e, "7");
        sc.key("key.keyboard.escape", &mut e);
        assert_eq!(e.settings.radar.max_entities, 250);
        // A click elsewhere sets it too.
        type_in(&mut sc, &mut e, "40");
        sc.click(Some("cat:0"), [0.0, 0.0], false, &mut e);
        assert_eq!(e.settings.radar.max_entities, 40);
        // Commands stay commands; dimension lists take short names.
        assert!(command("tp @s {x} {y} {z}").is_none());
        assert_eq!(command(" /tp {x} {y} {z} ").as_deref(), Some("/tp {x} {y} {z}"));
        let dims = parse_dimensions("overworld, the_nether mymod:deep").unwrap();
        assert_eq!(dims, ["minecraft:overworld", "minecraft:the_nether", "mymod:deep"]);
        assert_eq!(dimension_list(&dims), "overworld, the_nether, mymod:deep");
        assert_eq!(parse_dimensions(" ").unwrap(), Vec::<String>::new(), "empty means all");
        assert!(parse_dimensions("Over World!").is_none());
    }

    #[test]
    fn rows_have_unique_labels() {
        let labels: Vec<String> = Category::ALL.iter().flat_map(|c| rows(*c)).map(|r| r.label).collect();
        for (i, l) in labels.iter().enumerate() {
            assert!(!labels[i + 1..].contains(l), "{l} twice");
        }
    }

    #[test]
    fn profiles() {
        let (mut w, mut s) = (DimensionWaypoints::default(), Settings::default());
        let mut e = env(&mut w, &mut s, [854.0, 480.0]);
        let mut sc = SettingsScreen::default();
        let i = |label: &str| rows(Category::General).iter().position(|r| r.label == label).unwrap();
        assert_eq!(
            sc.click(Some(&format!("row:{}", i("Profile"))), [0.0, 0.0], false, &mut e),
            Action::Profile(ProfileOp::Select("pvp".into()))
        );
        sc.click(Some(&format!("row:{}", i("Rename Profile"))), [0.0, 0.0], false, &mut e);
        for ch in "x".chars() {
            sc.typed(ch);
        }
        assert_eq!(
            sc.key("key.keyboard.enter", &mut e),
            Action::Profile(ProfileOp::Rename("defaultx".into()))
        );
    }
}

//! Meridian's key bindings and their defaults (`docs/KEYBINDS.md`).
//!
//! The defaults match the reference mods' `options.txt` so that players who
//! switch find the same keys. The host registers them with the game; key
//! events come back to the core as [`HostEvent::Key`](meridian_host::HostEvent).

use meridian_host::KeyBinding;

/// Minecraft's notation for an unbound key.
pub const UNBOUND: &str = "key.keyboard.unknown";

macro_rules! bindings {
    ($($id:literal, $label:literal, $default:expr;)*) => {
        &[$(KeyBinding { id: $id, label: $label, default: $default },)*]
    };
}

/// Every key binding, in the order the controls screen lists them.
pub const KEY_BINDINGS: &[KeyBinding] = bindings! {
    // Minimap
    "minimap.zoom_in", "Zoom minimap in", UNBOUND;
    "minimap.zoom_out", "Zoom minimap out", UNBOUND;
    "waypoint.new", "New waypoint", "key.keyboard.b";
    "waypoint.menu", "Waypoints", "key.keyboard.u";
    "minimap.enlarge", "Enlarge minimap", "key.keyboard.z";
    "minimap.toggle", "Show/hide minimap", UNBOUND;
    "waypoint.toggle_world", "Show/hide waypoints in the world", UNBOUND;
    "waypoint.toggle_minimap", "Show/hide waypoints on the minimap", UNBOUND;
    "overlay.slime", "Show/hide slime chunks", UNBOUND;
    "overlay.grid", "Show/hide chunk grid", UNBOUND;
    "waypoint.instant", "Instant waypoint", "key.keyboard.keypad.add";
    "waypoint.next_set", "Next waypoint set", UNBOUND;
    "waypoint.all_sets", "Show all waypoint sets", UNBOUND;
    "overlay.light", "Show/hide light overlay", UNBOUND;
    "radar.toggle", "Show/hide entity radar", UNBOUND;
    "radar.reverse", "Reverse entity radar filter", UNBOUND;
    "cave.toggle_manual", "Manual cave mode", UNBOUND;
    "players.alt_list", "Alternative player list", UNBOUND;
    "players.tracked_map", "Tracked players on the map", UNBOUND;
    "players.tracked_world", "Tracked players in the world", UNBOUND;
    "overlay.claims", "Show/hide claims", UNBOUND;
    "settings", "Minimap settings", "key.keyboard.y";
    "server_settings", "Minimap server settings", UNBOUND;
    // World map
    "worldmap.open", "Open world map", "key.keyboard.m";
    "worldmap.settings", "World map settings", "key.keyboard.right.bracket";
    "worldmap.server_settings", "World map server settings", UNBOUND;
    "worldmap.zoom_in", "Zoom world map in", UNBOUND;
    "worldmap.zoom_out", "Zoom world map out", UNBOUND;
    "quick_confirm", "Quick confirm", "key.keyboard.right.shift";
    "worldmap.toggle_dimension", "Switch map dimension", UNBOUND;
};

/// The binding with this id.
pub fn binding(id: &str) -> Option<&'static KeyBinding> {
    KEY_BINDINGS.iter().find(|b| b.id == id)
}

/// The bindings a raw key press triggers, given the player's choices
/// (`chosen(id, default)` answers the key a binding is set to). Unbound
/// keys never match.
pub fn bindings_for_key<'a>(
    key: &'a str,
    chosen: impl Fn(&str, &str) -> String + 'a,
) -> impl Iterator<Item = &'static KeyBinding> + 'a {
    KEY_BINDINGS
        .iter()
        .filter(move |b| key != UNBOUND && chosen(b.id, b.default) == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn thirty_unique_bindings() {
        assert_eq!(KEY_BINDINGS.len(), 30);
        let ids: HashSet<_> = KEY_BINDINGS.iter().map(|b| b.id).collect();
        assert_eq!(ids.len(), KEY_BINDINGS.len());
    }

    #[test]
    fn defaults_match_reference_options_txt() {
        let bound: Vec<_> = KEY_BINDINGS
            .iter()
            .filter(|b| b.default != UNBOUND)
            .map(|b| (b.id, b.default))
            .collect();
        assert_eq!(
            bound,
            [
                ("waypoint.new", "key.keyboard.b"),
                ("waypoint.menu", "key.keyboard.u"),
                ("minimap.enlarge", "key.keyboard.z"),
                ("waypoint.instant", "key.keyboard.keypad.add"),
                ("settings", "key.keyboard.y"),
                ("worldmap.open", "key.keyboard.m"),
                ("worldmap.settings", "key.keyboard.right.bracket"),
                ("quick_confirm", "key.keyboard.right.shift"),
            ]
        );
    }

    #[test]
    fn lookup() {
        assert_eq!(
            binding("worldmap.open").map(|b| b.default),
            Some("key.keyboard.m")
        );
        assert!(binding("nope").is_none());
    }

    #[test]
    fn raw_keys_follow_the_players_choice() {
        let defaults = |_: &str, d: &str| d.to_owned();
        let ids = |key| {
            bindings_for_key(key, defaults)
                .map(|b| b.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("key.keyboard.m"), ["worldmap.open"]);
        assert!(ids(UNBOUND).is_empty(), "unbound actions never fire");
        assert!(ids("key.keyboard.q").is_empty());

        let rebound = |id: &str, d: &str| {
            if id == "radar.toggle" {
                "key.keyboard.r".to_owned()
            } else {
                d.to_owned()
            }
        };
        let hits: Vec<_> = bindings_for_key("key.keyboard.r", rebound)
            .map(|b| b.id)
            .collect();
        assert_eq!(hits, ["radar.toggle"]);
    }
}

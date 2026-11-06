//! The `/seedatlas` commands (`SeedAtlasCommands`) with the replies of the
//! Fabric version's English strings:
//!
//! ```text
//! /seedatlas [status]
//! /seedatlas seed <seed>
//! /seedatlas dimension-seed <nether|end> <seed>
//! /seedatlas clear [nether|end]
//! /seedatlas version [<version>|auto]
//! /seedatlas settings
//! ```
//!
//! Pure logic: the adapter passes the arguments after `seedatlas` and the
//! state, shows the lines, saves the settings when asked to and opens the
//! settings screen when the outcome says so.

use meridian_seedatlas::Dimension;

use crate::config::SeedAtlasSettings;
use crate::seedatlas::{SeedBook, SeedEntry, SeedValue, parse_seed};
use crate::seedatlas_state::{
    GameVersion, SeedContext, game_version_name, prefetch_label, resolution_label,
    set_game_version, workers_label, world_type_label,
};

const NO_CONTEXT: &str = "No active world or multiplayer server was found.";
const SEED_BLANK: &str = "The seed must not be empty.";

/// What a command works on.
#[derive(Debug)]
pub struct CommandCtx<'a> {
    pub book: &'a mut SeedBook,
    pub settings: &'a mut SeedAtlasSettings,
    /// The open world or server, `None` outside of one.
    pub context: Option<SeedContext>,
    /// The seed (and Large Biomes) the singleplayer world itself reports,
    /// used while no seed was entered.
    pub detected: Option<(i64, bool)>,
}

/// One line of a reply; errors are shown as failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandLine {
    pub text: String,
    pub error: bool,
}

/// What the adapter does after a command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandOutcome {
    pub lines: Vec<CommandLine>,
    /// Open the Seed Atlas settings (after the chat closes).
    pub open_settings: bool,
    /// The settings changed: save them.
    pub settings_changed: bool,
    /// Seeds or the version changed: resolve the seeds again
    /// (`seedatlas_state::resolve_all`) and reconfigure the service.
    pub seeds_changed: bool,
}

impl CommandOutcome {
    fn feedback(&mut self, text: impl Into<String>) {
        self.lines.push(CommandLine {
            text: text.into(),
            error: false,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        self.lines.push(CommandLine {
            text: text.into(),
            error: true,
        });
    }

    /// The reply as one text, a line each.
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The subcommands, for completion.
pub const SUBCOMMANDS: [&str; 6] = [
    "seed",
    "dimension-seed",
    "clear",
    "version",
    "status",
    "settings",
];

/// The versions `version` completes: `auto` and every span.
pub fn version_suggestions() -> Vec<&'static str> {
    std::iter::once("auto")
        .chain(GameVersion::ALL.iter().map(|v| v.name))
        .collect()
}

fn dimension_name(d: Dimension) -> &'static str {
    match d {
        Dimension::Overworld => "Overworld",
        Dimension::Nether => "Nether",
        Dimension::End => "End",
    }
}

fn dimension_arg(name: &str) -> Option<Dimension> {
    match name {
        "nether" => Some(Dimension::Nether),
        "end" => Some(Dimension::End),
        _ => None,
    }
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

/// Runs `/seedatlas <args>`.
pub fn seedatlas_command(args: &str, ctx: &mut CommandCtx<'_>) -> CommandOutcome {
    let mut out = CommandOutcome::default();
    let args = args.trim();
    let args = args.strip_prefix('/').unwrap_or(args);
    let args = args.strip_prefix("seedatlas").unwrap_or(args).trim();
    let (cmd, rest) = args
        .split_once(char::is_whitespace)
        .map_or((args, ""), |(a, b)| (a, b.trim()));
    match cmd {
        "" | "status" => status(ctx, &mut out),
        "seed" => set_seed(ctx, rest, &mut out),
        "dimension-seed" => {
            let (name, seed) = rest
                .split_once(char::is_whitespace)
                .map_or((rest, ""), |(a, b)| (a, b.trim()));
            match dimension_arg(name) {
                Some(d) => set_dimension_seed(ctx, d, seed, &mut out),
                None => out.error("Usage: /seedatlas dimension-seed <nether|end> <seed>"),
            }
        }
        "clear" => match rest {
            "" => clear_seed(ctx, &mut out),
            name => match dimension_arg(name) {
                Some(d) => clear_dimension_seed(ctx, d, &mut out),
                None => out.error("Usage: /seedatlas clear [nether|end]"),
            },
        },
        "version" => match rest.split_whitespace().next() {
            None => show_version(ctx, &mut out),
            Some(v) => set_version(ctx, v, &mut out),
        },
        "settings" => out.open_settings = true,
        _ => out.error(
            "Unknown command. Use /seedatlas seed, dimension-seed, clear, version, status or settings.",
        ),
    }
    out
}

fn set_seed(ctx: &mut CommandCtx<'_>, input: &str, out: &mut CommandOutcome) {
    let Some(seed) = parse_seed(input) else {
        out.error(SEED_BLANK);
        return;
    };
    let Some(c) = ctx.context.clone() else {
        out.error(NO_CONTEXT);
        return;
    };
    let input = input.trim();
    ctx.book.set(
        &c.key,
        SeedEntry {
            input: input.to_owned(),
            seed,
            large_biomes: ctx.settings.large_biomes,
        },
    );
    out.seeds_changed = true;
    out.feedback(format!("Seed set to {seed} ({input}) for {}.", c.label));
}

fn set_dimension_seed(
    ctx: &mut CommandCtx<'_>,
    dimension: Dimension,
    input: &str,
    out: &mut CommandOutcome,
) {
    let Some(seed) = parse_seed(input) else {
        out.error(SEED_BLANK);
        return;
    };
    let Some(c) = ctx.context.clone() else {
        out.error(NO_CONTEXT);
        return;
    };
    let input = input.trim();
    ctx.book.set_dimension_seed(
        &c.key,
        dimension,
        SeedValue {
            input: input.to_owned(),
            seed,
        },
    );
    out.seeds_changed = true;
    out.feedback(format!(
        "{} seed set to {seed} ({input}) for {}. Other dimensions keep the base seed.",
        dimension_name(dimension),
        c.label
    ));
}

fn clear_dimension_seed(ctx: &mut CommandCtx<'_>, dimension: Dimension, out: &mut CommandOutcome) {
    let Some(c) = &ctx.context else {
        out.error(NO_CONTEXT);
        return;
    };
    if ctx.book.clear_dimension_seed(&c.key, dimension) {
        out.seeds_changed = true;
        out.feedback(format!(
            "The {} uses the base seed again.",
            dimension_name(dimension)
        ));
    } else {
        out.feedback(format!(
            "The {} has no seed of its own.",
            dimension_name(dimension)
        ));
    }
}

fn clear_seed(ctx: &mut CommandCtx<'_>, out: &mut CommandOutcome) {
    let Some(c) = &ctx.context else {
        out.error(NO_CONTEXT);
        return;
    };
    // The world's own seed counts as configured until cleared once.
    let had_world_seed = ctx.detected.is_some() && !ctx.book.detected_suppressed(&c.key);
    let removed = ctx.book.clear(&c.key);
    ctx.book.suppress_detected(&c.key);
    if removed || had_world_seed {
        out.seeds_changed = true;
        out.feedback("The configured seed for this world/server was cleared.");
    } else {
        out.feedback("No seed is configured for this world/server.");
    }
}

fn set_version(ctx: &mut CommandCtx<'_>, input: &str, out: &mut CommandOutcome) {
    let version = if input.eq_ignore_ascii_case("auto") {
        None
    } else {
        match GameVersion::parse(input) {
            Some(v) => Some(v),
            None => {
                out.error(format!(
                    "Unknown version {input}. Use auto or a release from {} to {}.",
                    GameVersion::ALL[GameVersion::ALL.len() - 1].first,
                    GameVersion::NEWEST.name
                ));
                return;
            }
        }
    };
    let Some(c) = &ctx.context else {
        out.error(NO_CONTEXT);
        return;
    };
    if set_game_version(ctx.settings, &c.key, version) {
        out.settings_changed = true;
        out.seeds_changed = true;
    }
    show_version(ctx, out);
}

fn show_version(ctx: &mut CommandCtx<'_>, out: &mut CommandOutcome) {
    let Some(c) = &ctx.context else {
        out.error(NO_CONTEXT);
        return;
    };
    out.feedback(format!(
        "Minecraft version: {}",
        game_version_name(ctx.settings, &c.key)
    ));
}

fn status(ctx: &mut CommandCtx<'_>, out: &mut CommandOutcome) {
    let Some(c) = &ctx.context else {
        out.error(NO_CONTEXT);
        return;
    };
    out.feedback(format!("Seed Atlas context: {}", c.label));

    let base = match ctx.book.get(&c.key) {
        Some(e) => Some((e.input.clone(), e.seed)),
        None if !ctx.book.detected_suppressed(&c.key) => {
            ctx.detected.map(|(seed, _)| (seed.to_string(), seed))
        }
        None => None,
    };
    let nether = ctx.book.dimension_seed(&c.key, Dimension::Nether);
    let end = ctx.book.dimension_seed(&c.key, Dimension::End);
    match base {
        Some((input, seed)) => out.feedback(format!("Seed: {seed} (input: {input})")),
        None if nether.is_some() || end.is_some() => out
            .feedback("Base seed: not configured. Only dimensions with their own seed are shown."),
        None => out.feedback("Seed: not configured. Use /seedatlas seed <seed>."),
    }
    for (d, own) in [(Dimension::Nether, nether), (Dimension::End, end)] {
        if let Some(v) = own {
            out.feedback(format!(
                "{} seed: {} (input: {})",
                dimension_name(d),
                v.seed,
                v.input
            ));
        }
    }

    let s = &*ctx.settings;
    out.feedback(format!(
        "Layer: {}, opacity: {}%",
        on_off(s.biome_layer),
        (u32::from(s.biome_opacity) * 100 + 127) / 255
    ));
    out.feedback(format!("World type: {}", world_type_label(s.large_biomes)));
    out.feedback(format!(
        "Minecraft version: {}",
        game_version_name(s, &c.key)
    ));
    out.feedback(format!("Structures: {}", on_off(s.structures)));
    out.feedback(format!(
        "Performance: biome resolution {}, workers {}, prefetch {}",
        resolution_label(s.biome_resolution),
        workers_label(s.worker_threads),
        prefetch_label(s.prefetch_radius)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        dir: std::path::PathBuf,
        book: SeedBook,
        settings: SeedAtlasSettings,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("meridian-cmd-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                book: SeedBook::load(&dir),
                dir,
                settings: SeedAtlasSettings::default(),
            }
        }

        fn run(
            &mut self,
            args: &str,
            in_world: bool,
            detected: Option<(i64, bool)>,
        ) -> CommandOutcome {
            let mut ctx = CommandCtx {
                book: &mut self.book,
                settings: &mut self.settings,
                context: in_world.then(|| SeedContext {
                    key: "sp-My_World".into(),
                    label: "My World".into(),
                }),
                detected,
            };
            seedatlas_command(args, &mut ctx)
        }

        fn world(&mut self, args: &str) -> CommandOutcome {
            self.run(args, true, None)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn one(out: &CommandOutcome) -> (&str, bool) {
        assert_eq!(out.lines.len(), 1, "{:?}", out.lines);
        (&out.lines[0].text, out.lines[0].error)
    }

    #[test]
    fn seed_sets_and_replies() {
        let mut f = Fixture::new("seed");
        let out = f.world("seed 12345");
        assert_eq!(
            one(&out),
            ("Seed set to 12345 (12345) for My World.", false)
        );
        assert!(out.seeds_changed);
        let out = f.world("seed hello world");
        assert_eq!(
            one(&out).0,
            "Seed set to 1794106052 (hello world) for My World."
        );
        assert_eq!(f.book.get("sp-My_World").unwrap().input, "hello world");
        assert_eq!(
            one(&f.world("seed   ")),
            ("The seed must not be empty.", true)
        );
        assert_eq!(one(&f.world("seed")), ("The seed must not be empty.", true));
        let out = f.run("seed 1", false, None);
        assert_eq!(
            one(&out),
            ("No active world or multiplayer server was found.", true)
        );
        assert!(!out.seeds_changed);
        // The slash and the command name are tolerated.
        assert_eq!(
            one(&f.world("/seedatlas seed 5")).0,
            "Seed set to 5 (5) for My World."
        );
    }

    #[test]
    fn dimension_seeds_set_and_clear() {
        let mut f = Fixture::new("dim");
        let out = f.world("dimension-seed nether 777");
        assert_eq!(
            one(&out),
            (
                "Nether seed set to 777 (777) for My World. Other dimensions keep the base seed.",
                false
            )
        );
        let out = f.world("dimension-seed end the end seed");
        assert!(one(&out).0.starts_with("End seed set to "));
        assert!(one(&out).0.contains("(the end seed) for My World."));
        assert_eq!(
            one(&f.world("dimension-seed end")).0,
            "The seed must not be empty."
        );
        assert!(one(&f.world("dimension-seed overworld 1")).1);
        assert_eq!(
            one(&f.run("dimension-seed nether 1", false, None)).0,
            "No active world or multiplayer server was found."
        );

        let out = f.world("clear nether");
        assert_eq!(one(&out), ("The Nether uses the base seed again.", false));
        assert!(out.seeds_changed);
        let out = f.world("clear nether");
        assert_eq!(one(&out), ("The Nether has no seed of its own.", false));
        assert!(!out.seeds_changed);
        assert_eq!(
            one(&f.world("clear end")).0,
            "The End uses the base seed again."
        );
        assert_eq!(
            one(&f.run("clear end", false, None)),
            ("No active world or multiplayer server was found.", true)
        );
        assert!(one(&f.world("clear sideways")).1);
    }

    #[test]
    fn clear_removes_every_seed_and_ignores_the_world_seed() {
        let mut f = Fixture::new("clear");
        assert_eq!(
            one(&f.world("clear")).0,
            "No seed is configured for this world/server."
        );
        f.world("seed 1");
        f.world("dimension-seed end 2");
        let out = f.world("clear");
        assert_eq!(
            one(&out),
            (
                "The configured seed for this world/server was cleared.",
                false
            )
        );
        assert!(out.seeds_changed);
        assert!(!f.book.has_any("sp-My_World"));

        // The singleplayer world's own seed counts until it is cleared once.
        let mut f = Fixture::new("clear2");
        let out = f.run("clear", true, Some((9, false)));
        assert_eq!(
            one(&out).0,
            "The configured seed for this world/server was cleared."
        );
        assert_eq!(
            one(&f.run("clear", true, Some((9, false)))).0,
            "No seed is configured for this world/server."
        );
        assert!(f.book.detected_suppressed("sp-My_World"));
        // Setting a seed takes it again.
        f.world("seed 4");
        assert!(!f.book.detected_suppressed("sp-My_World"));
        assert_eq!(
            one(&f.run("clear", false, None)),
            ("No active world or multiplayer server was found.", true)
        );
    }

    #[test]
    fn version_shows_sets_and_follows_auto() {
        let mut f = Fixture::new("version");
        assert_eq!(one(&f.world("version")).0, "Minecraft version: Auto (26.3)");
        let out = f.world("version 1.21.5");
        assert_eq!(
            one(&out),
            ("Minecraft version: 1.21.5 \u{2013} 26.1.2", false)
        );
        assert!(out.settings_changed && out.seeds_changed);
        assert_eq!(
            f.settings
                .game_versions
                .get("sp-My_World")
                .map(String::as_str),
            Some("26.1.2")
        );
        assert_eq!(
            one(&f.world("version")).0,
            "Minecraft version: 1.21.5 \u{2013} 26.1.2"
        );
        let out = f.world("version 26.3");
        assert_eq!(one(&out).0, "Minecraft version: 26.3");
        let out = f.world("version AUTO");
        assert_eq!(one(&out).0, "Minecraft version: Auto (26.3)");
        assert!(out.settings_changed);
        let out = f.world("version auto");
        assert!(!out.settings_changed, "already automatic");

        let out = f.world("version 99.9");
        assert_eq!(
            one(&out),
            (
                "Unknown version 99.9. Use auto or a release from 1.1 to 26.3.",
                true
            )
        );
        assert_eq!(
            one(&f.run("version", false, None)),
            ("No active world or multiplayer server was found.", true)
        );
        assert_eq!(
            one(&f.run("version 1.20", false, None)),
            ("No active world or multiplayer server was found.", true)
        );
        assert!(version_suggestions().starts_with(&["auto", "26.3", "26.2"]));
    }

    #[test]
    fn settings_opens_the_screen() {
        let mut f = Fixture::new("settings");
        let out = f.world("settings");
        assert!(out.open_settings && out.lines.is_empty());
        assert!(f.run("settings", false, None).open_settings);
    }

    #[test]
    fn status_lists_the_state() {
        let mut f = Fixture::new("status");
        assert_eq!(
            one(&f.run("status", false, None)),
            ("No active world or multiplayer server was found.", true)
        );

        let out = f.world("");
        assert_eq!(
            out.text(),
            "Seed Atlas context: My World\n\
             Seed: not configured. Use /seedatlas seed <seed>.\n\
             Layer: On, opacity: 100%\n\
             World type: Normal\n\
             Minecraft version: Auto (26.3)\n\
             Structures: Off\n\
             Performance: biome resolution Full Detail (1x), workers Auto, prefetch Near"
        );
        assert!(out.lines.iter().all(|l| !l.error));

        f.world("seed hello");
        f.world("dimension-seed nether 5");
        f.world("dimension-seed end 6");
        f.settings.large_biomes = true;
        f.settings.structures = true;
        f.settings.biome_layer = false;
        f.settings.biome_opacity = 128;
        f.settings.biome_resolution = 4;
        f.settings.worker_threads = 2;
        f.settings.prefetch_radius = 3;
        f.world("version 1.19");
        let out = f.world("status");
        assert_eq!(
            out.text(),
            "Seed Atlas context: My World\n\
             Seed: 99162322 (input: hello)\n\
             Nether seed: 5 (input: 5)\n\
             End seed: 6 (input: 6)\n\
             Layer: Off, opacity: 50%\n\
             World type: Large Biomes\n\
             Minecraft version: 1.19 \u{2013} 1.19.2\n\
             Structures: On\n\
             Performance: biome resolution Reduced (4x), workers 2, prefetch Far"
        );

        // Only own seeds: no base seed.
        let mut f = Fixture::new("status2");
        f.world("dimension-seed end 6");
        let text = f.world("status").text();
        assert!(text.contains(
            "Base seed: not configured. Only dimensions with their own seed are shown.\nEnd seed: 6 (input: 6)"
        ));

        // The singleplayer world's seed shows as the base seed.
        let mut f = Fixture::new("status3");
        let text = f.run("status", true, Some((42, false))).text();
        assert!(text.contains("Seed: 42 (input: 42)"));
    }

    #[test]
    fn unknown_commands_say_so() {
        let mut f = Fixture::new("unknown");
        let out = f.world("frobnicate");
        assert!(out.lines[0].error);
        assert!(!out.open_settings);
    }
}

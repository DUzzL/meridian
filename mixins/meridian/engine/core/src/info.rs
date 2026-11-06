//! The info lines under the minimap, in the reference's order: coordinates,
//! overworld coordinates (in the Nether), chunk, angles, dimension, biome,
//! weather, light, game time, real time, then indicators.

use crate::config::{ClockInfo, InfoSettings, LightInfo, MinimapSettings};

/// What the lines are made from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InfoInput {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub dimension: String,
    pub biome: Option<String>,
    /// Rain and thunder levels 0..=1.
    pub weather: (f32, f32),
    /// Block and sky light at the player's feet.
    pub light: (u8, u8),
    /// Ticks; 0 is 6:00.
    pub day_time: Option<i64>,
    /// Local wall clock (hours, minutes).
    pub real_time: Option<(u32, u32)>,
    pub manual_cave: bool,
}

/// `minecraft:plains` as "Plains", `minecraft:the_nether` as "The Nether".
pub fn title_case(id: &str) -> String {
    let name = id.rsplit(':').next().unwrap_or(id);
    name.split('_')
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or(String::new(), |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn clock(h: u32, m: u32, style: ClockInfo) -> Option<String> {
    match style {
        ClockInfo::Off => None,
        ClockInfo::H24 => Some(format!("{h:02}:{m:02}")),
        ClockInfo::H12 => {
            let (h12, ampm) = match h {
                0 => (12, "AM"),
                1..=11 => (h, "AM"),
                12 => (12, "PM"),
                _ => (h - 12, "PM"),
            };
            Some(format!("{h12}:{m:02} {ampm}"))
        }
    }
}

/// Game time of day: tick 0 is 6:00, a day is 24000 ticks.
pub fn game_clock(day_time: i64) -> (u32, u32) {
    let t = (day_time + 6000).rem_euclid(24_000);
    ((t / 1000) as u32, ((t % 1000) * 60 / 1000) as u32)
}

pub fn lines(s: &MinimapSettings, i: &InfoInput) -> Vec<String> {
    let info: &InfoSettings = &s.info;
    let [x, y, z] = i.pos.map(f64::floor);
    let mut out = Vec::new();
    if s.show_coords {
        out.push(format!("{x}, {y}, {z}"));
    }
    if info.overworld_coords && i.dimension == "minecraft:the_nether" {
        out.push(format!("Overworld: {}, {}", x * 8.0, z * 8.0));
    }
    if info.chunk_coords {
        let (cx, cz) = ((x as i64).div_euclid(16), (z as i64).div_euclid(16));
        out.push(format!(
            "Chunk: {cx}, {cz} ({}, {})",
            (x as i64).rem_euclid(16),
            (z as i64).rem_euclid(16)
        ));
    }
    if info.angles {
        let yaw = (i.yaw + 180.0).rem_euclid(360.0) - 180.0;
        out.push(format!("{yaw:.1} / {:.1}", i.pitch));
    }
    if info.dimension {
        out.push(title_case(&i.dimension));
    }
    if s.show_biome
        && let Some(b) = &i.biome
    {
        out.push(title_case(b));
    }
    if info.weather {
        out.push(
            if i.weather.1 > 0.5 {
                "Thunder"
            } else if i.weather.0 > 0.2 {
                "Rain"
            } else {
                "Clear"
            }
            .to_owned(),
        );
    }
    match info.light {
        LightInfo::Off => {}
        LightInfo::Block => out.push(format!("Light: {}", i.light.0)),
        LightInfo::Sky => out.push(format!("Sky Light: {}", i.light.1)),
        LightInfo::Both => out.push(format!("Light: {} ({})", i.light.0, i.light.1)),
    }
    if let Some(t) = i.day_time {
        let (h, m) = game_clock(t);
        out.extend(clock(h, m, info.game_time));
    }
    if let Some((h, m)) = i.real_time {
        out.extend(clock(h, m, info.real_time).map(|c| format!("Real: {c}")));
    }
    if i.manual_cave {
        out.push("Manual Cave Mode".to_owned());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> InfoInput {
        InfoInput {
            pos: [130.7, 64.2, -10.5],
            yaw: 270.0,
            pitch: -12.0,
            dimension: "minecraft:the_nether".into(),
            biome: Some("minecraft:soul_sand_valley".into()),
            weather: (0.0, 0.0),
            light: (7, 0),
            day_time: Some(6000),
            real_time: Some((21, 5)),
            manual_cave: false,
        }
    }

    #[test]
    fn default_is_coordinates_only() {
        assert_eq!(
            lines(&MinimapSettings::default(), &input()),
            ["130, 64, -11"]
        );
    }

    #[test]
    fn every_line_in_order() {
        let mut s = MinimapSettings {
            show_biome: true,
            ..Default::default()
        };
        s.info = InfoSettings {
            overworld_coords: true,
            chunk_coords: true,
            angles: true,
            dimension: true,
            weather: true,
            light: LightInfo::Both,
            game_time: ClockInfo::H24,
            real_time: ClockInfo::H12,
        };
        let mut i = input();
        i.manual_cave = true;
        assert_eq!(
            lines(&s, &i),
            [
                "130, 64, -11",
                "Overworld: 1040, -88",
                "Chunk: 8, -1 (2, 5)",
                "-90.0 / -12.0",
                "The Nether",
                "Soul Sand Valley",
                "Clear",
                "Light: 7 (0)",
                "12:00",
                "Real: 9:05 PM",
                "Manual Cave Mode",
            ]
        );
    }

    #[test]
    fn clocks() {
        assert_eq!(game_clock(0), (6, 0));
        assert_eq!(game_clock(18_000), (0, 0));
        assert_eq!(game_clock(13_500), (19, 30));
        assert_eq!(clock(0, 7, ClockInfo::H12).unwrap(), "12:07 AM");
        assert_eq!(clock(12, 0, ClockInfo::H12).unwrap(), "12:00 PM");
    }
}

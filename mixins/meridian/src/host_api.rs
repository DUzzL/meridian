//! Meridian's client queries, expressed through the fork's side-specific API.
use mod_api::{BlockSprites, ClientHost, HudMode, LightLayer, Text};
use std::path::PathBuf;

pub struct Player {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f32,
    pub pitch: f32,
    pub dimension: String,
}

pub struct ClientState {
    pub gui_size: [f32; 2],
    pub gui_hidden: bool,
    pub server_address: Option<String>,
}

pub trait MeridianClient {
    fn meridian_player(&self) -> Option<Player>;
    fn meridian_client_state(&self) -> Option<ClientState>;
    fn meridian_key(&self, owner: &str, id: &str, default: &str) -> String;
    fn meridian_text_width(&self, text: &str) -> f32;
    fn meridian_block_sprites(&self, block: &str) -> Vec<Option<BlockSprites>>;
    fn meridian_action_bar(&mut self, text: Text, animate: bool);
    fn config_dir(&self) -> std::io::Result<PathBuf>;
    fn weather(&self) -> (f32, f32);
    fn light(&self, pos: [i32; 3]) -> (u8, u8);
}

impl MeridianClient for dyn ClientHost + '_ {
    fn meridian_player(&self) -> Option<Player> {
        let player = self.local_player()?;
        let entity = player.entity();
        let [x, y, z] = entity.position(self)?;
        let [yaw, pitch] = entity.rotation(self)?;
        Some(Player {
            x,
            y,
            z,
            yaw,
            pitch,
            dimension: player.dimension(self)?.to_string(),
        })
    }

    fn meridian_client_state(&self) -> Option<ClientState> {
        self.local_player()?;
        Some(ClientState {
            gui_size: crate::bridge::gui_size(),
            gui_hidden: self.hud_mode() != HudMode::AllVisible,
            server_address: self
                .client_session()
                .client_server()
                .and_then(|s| s.address),
        })
    }

    fn meridian_key(&self, owner: &str, id: &str, default: &str) -> String {
        self.mod_key(owner, id)
            .map_or_else(|| default.to_owned(), |(key, _)| key)
    }

    fn meridian_text_width(&self, text: &str) -> f32 {
        self.text_width(&Text::literal(text)).unwrap_or(0) as f32
    }

    fn meridian_block_sprites(&self, block: &str) -> Vec<Option<BlockSprites>> {
        self.block_state_id(block)
            .map_or_else(|| vec![None], |state| self.block_sprites(&[state]))
    }

    fn meridian_action_bar(&mut self, text: Text, animate: bool) {
        crate::bridge::action_bar(text, animate);
    }

    fn config_dir(&self) -> std::io::Result<PathBuf> {
        let dir = self.instance_dir().join("config");
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn weather(&self) -> (f32, f32) {
        self.level(self.dimension())
            .and_then(|l| l.weather(self))
            .unwrap_or((0.0, 0.0))
    }

    fn light(&self, pos: [i32; 3]) -> (u8, u8) {
        let Some(level) = self.level(self.dimension()) else {
            return (0, 0);
        };
        let read = |layer| level.brightness(self, layer, pos).unwrap_or(0).clamp(0, 15) as u8;
        (read(LightLayer::Block), read(LightLayer::Sky))
    }
}

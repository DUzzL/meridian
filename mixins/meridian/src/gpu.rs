//! The minimap's terrain on the GPU.
//!
//! The game thread composes a north-up image of the map around the player
//! (`meridian_core::minimap_image`: a texture that wraps around, updated a
//! cell at a time) and hands over what changed ([`send`]) and where and how
//! to draw it ([`MapFrame`]). The native `world_drawn` callback (
//! after the world and before the HUD) uploads the changed cells and draws
//! one quad into the minimap's rectangle: the shader turns the map with the
//! player, keeps the player's sub-texel position, cuts a round map to its
//! circle and fills unexplored texels with the backdrop. The HUD then draws
//! the frame, compass, marks and text over it.

use parking_lot::Mutex;

/// Texels for the render thread to upload, oldest first.
#[derive(Debug, Default)]
pub struct Uploads {
    /// The texture's side, texels.
    pub size: u32,
    /// The whole image (ARGB, row-major): after a reset, or when asked for.
    pub full: Option<Vec<u32>>,
    /// Cells: their corner in the texture (texels) and `CELL` x `CELL` texels.
    pub cells: Vec<([u32; 2], Vec<u32>)>,
    /// The render thread has nothing to draw from (a new texture): the game
    /// thread sends the whole image.
    pub want_full: bool,
}

pub static UPLOADS: Mutex<Uploads> = Mutex::new(Uploads {
    size: 0,
    full: None,
    cells: Vec::new(),
    want_full: false,
});

/// Hands texels to the render thread.
pub fn send(f: impl FnOnce(&mut Uploads)) {
    f(&mut UPLOADS.lock());
}

/// What to draw this frame.
#[derive(Debug, Clone)]
pub struct MapFrame {
    /// The map's rectangle in GUI pixels, and the GUI's size.
    pub rect: [f32; 4],
    pub gui: [f32; 2],
    /// World directions (x, z) of the screen's right and up.
    pub right: [f32; 2],
    pub forward: [f32; 2],
    /// The player's position from the window's north-west texel, texels.
    pub center: [f32; 2],
    pub blocks_per_gui_px: f32,
    /// Blocks per texel.
    pub scale: f32,
    /// The texture's side, and where the window's north-west texel is in it.
    pub size: u32,
    pub wrap: [u32; 2],
    pub round: bool,
    pub opacity: f32,
    /// Unexplored texels, premultiplied-free RGBA 0..1.
    pub backdrop: [f32; 4],
    /// When the HUD asked for it: a frame the HUD stopped refreshing (F1, a
    /// screen) stops drawing.
    pub at: std::time::Instant,
    /// The chunk grid, with its line width in blocks.
    pub grid: Option<f32>,
    /// The block at the window's north-west texel, for the grid's phase.
    pub origin: [i32; 2],
}

pub static FRAME: Mutex<Option<MapFrame>> = Mutex::new(None);

/// Sets (or clears) what the next frames draw.
pub fn set_frame(frame: Option<MapFrame>) {
    *FRAME.lock() = frame;
}

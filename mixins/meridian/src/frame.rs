// Meridian: once a frame, before trait mods see `frame`, report chunk events
// and answer the chunk reads the map asked for last frame (E1/E2).
fn meridian_frame(game: &mut crate::game::Game<'_>) {
    crate::game::meridian_bridge::frame(game);
}

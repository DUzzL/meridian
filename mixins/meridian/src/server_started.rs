fn server_started(game: &mut crate::game::Game<'_>) {
    let directory = game.world_dir();
    let world = directory
        .as_ref()
        .and_then(|d| d.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let seed = directory.and_then(|d| {
        let level = horizon_client::save::Level::read(&d)?;
        Some((
            level.seed,
            horizon_client::save::WorldGen::preset(&d) == "minecraft:large_biomes",
        ))
    });
    meridian_horizon::server_state(world, seed);
}

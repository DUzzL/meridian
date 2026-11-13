fn server_lifecycle(game: &mut crate::game::Game<'_>, phase: mod_api::WorldPhase) {
    match phase {
        mod_api::WorldPhase::WorldCloseFinal | mod_api::WorldPhase::WorldOpenFailed => {
            meridian_horizon::server_state(None, None);
        }
        _ => {}
    }
}

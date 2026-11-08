fn meridian_commands(tree: horizon_client::command::Dispatcher) -> horizon_client::command::Dispatcher {
    crate::game::meridian_bridge::complete_commands(tree)
}

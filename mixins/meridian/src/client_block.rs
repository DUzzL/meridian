// The client's shared write path: server updates, predictions and reconciliation.
fn client_block(pos: [i32; 3]) {
    meridian_horizon::bridge::changes::record(pos[0] >> 4, pos[2] >> 4);
}

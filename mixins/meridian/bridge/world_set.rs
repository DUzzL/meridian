// Meridian: note the chunk a single block write lands in (E1).
fn meridian_world_set(x: i32, z: i32) {
    crate::world::meridian_changes::record(x >> 4, z >> 4);
}

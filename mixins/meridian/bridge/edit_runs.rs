// Meridian: note every chunk a bulk edit (fill, explosion, structure) may touch (E1).
fn meridian_edit_runs(runs: &[crate::block_edit::Run]) {
    for run in runs {
        crate::world::meridian_changes::record(run.x >> 4, run.z >> 4);
    }
}

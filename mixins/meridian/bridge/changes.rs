// Meridian (transitional, docs/HORIZON_API.md E1): which loaded chunks had
// blocks written since the map last asked. Added to the client's `world`
// module by horizon-mixins; filled by HEAD injections into `World::set` and
// `World::edit_runs`, drained once a frame by `game::meridian_bridge`.
// Off until the bridge turns it on, so it costs one atomic load per write
// when Meridian is not running.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// At most this many entries wait; past it the bridge rescans everything.
const LIMIT: usize = 65_536;

static ON: AtomicBool = AtomicBool::new(false);
static CHANGED: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());
static OVERFLOWED: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        CHANGED.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Chunk (`cx`, `cz`) had a block written.
pub fn record(cx: i32, cz: i32) {
    if !ON.load(Ordering::Relaxed) {
        return;
    }
    let mut changed = CHANGED.lock().unwrap_or_else(|e| e.into_inner());
    if changed.last() == Some(&(cx, cz)) {
        return;
    }
    if changed.len() >= LIMIT {
        OVERFLOWED.store(true, Ordering::Relaxed);
        return;
    }
    changed.push((cx, cz));
}

/// The chunks written since the last call, and whether some were dropped.
pub fn take() -> (Vec<(i32, i32)>, bool) {
    let list = std::mem::take(&mut *CHANGED.lock().unwrap_or_else(|e| e.into_inner()));
    (list, OVERFLOWED.swap(false, Ordering::Relaxed))
}

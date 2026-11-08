//! The C side: the Seed Atlas engine and its C ABI for the mods
//! (`c/seedatlas-sax/seedatlas_sax.h`, ABI 6), compiled by `build.rs`. This
//! is the only module with `unsafe` code; it hands out safe wrappers.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int};
use std::ptr::NonNull;

/// `sax_context`, opaque.
#[repr(C)]
pub struct SaxContext {
    _private: [u8; 0],
}

/// `StructureConfig` of `finders.h`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct StructureConfigC {
    pub salt: i32,
    pub region_size: i8,
    pub chunk_range: i8,
    pub struct_type: u8,
    pub dim: i8,
    pub rarity: f32,
}

unsafe extern "C" {
    fn sax_abi_version() -> i32;
    fn sax_create_versioned(
        seed: i64,
        world_flags: u32,
        mc_version: *const c_char,
    ) -> *mut SaxContext;
    fn sax_destroy(context: *mut SaxContext);
    fn sax_biome_at(c: *const SaxContext, dim: i32, x: i32, y: i32, z: i32, out: *mut i32) -> i32;
    fn sax_biome_chunk_sampled(
        c: *const SaxContext,
        dim: i32,
        cx: i32,
        cz: i32,
        y: i32,
        step: i32,
        out_ids: *mut i32,
        out_argb: *mut u32,
    ) -> i32;
    fn sax_biome_area_sampled(
        c: *const SaxContext,
        dim: i32,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        out_ids: *mut i32,
        out_argb: *mut u32,
        capacity: i32,
    ) -> i32;
    fn sax_height_area_sampled(
        c: *const SaxContext,
        dim: i32,
        ox: i32,
        oz: i32,
        step: i32,
        w: i32,
        h: i32,
        out: *mut f32,
        capacity: i32,
    ) -> i32;
    fn sax_nether_floor_area_sampled(
        c: *const SaxContext,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        out: *mut f32,
        capacity: i32,
    ) -> i32;
    fn sax_nether_floor_area_carved_sampled(
        c: *const SaxContext,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: *const u64,
        carved_capacity: i32,
        out: *mut f32,
        capacity: i32,
    ) -> i32;
    fn sax_context_has_terrain(c: *const SaxContext) -> i32;
    fn sax_scan_structures(
        c: *const SaxContext,
        kind: i32,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
        check_biome: i32,
        out: *mut i32,
        capacity: i32,
    ) -> i32;
    fn sax_scan_strongholds(
        c: *const SaxContext,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
        check_biome: i32,
        out: *mut i32,
        capacity: i32,
    ) -> i32;
    fn sax_spawn(c: *const SaxContext, detailed: i32, out_xyz: *mut i32) -> i32;
    fn sax_scan_slime_chunks(
        c: *const SaxContext,
        x0: i32,
        z0: i32,
        x1: i32,
        z1: i32,
        out: *mut i32,
        capacity: i32,
    ) -> i32;

    fn getStructureConfig(kind: c_int, mc: c_int, out: *mut StructureConfigC) -> c_int;
    fn biome2str(mc: c_int, id: c_int) -> *const c_char;
    fn biomeExists(mc: c_int, id: c_int) -> c_int;
    fn initBiomeColors(colors: *mut [u8; 3]);
    fn mc2str(mc: c_int) -> *const c_char;
    fn str2mc(s: *const c_char) -> c_int;
    fn meridian_is_slime_chunk(seed: u64, cx: c_int, cz: c_int) -> c_int;
}

/// The ABI version of the compiled interface (`SAX_ABI_VERSION`).
pub fn abi_version() -> i32 {
    // SAFETY: no arguments, no state.
    unsafe { sax_abi_version() }
}

/// An engine string with static storage (string literals of `util.c`).
fn static_str(p: *const c_char) -> Option<&'static str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the engine returns NUL-terminated literals, which live as long
    // as the program.
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// `getStructureConfig`.
pub fn structure_config(kind: i32, mc: i32) -> Option<StructureConfigC> {
    let mut c = StructureConfigC::default();
    // SAFETY: `c` is a valid StructureConfig to write.
    (unsafe { getStructureConfig(kind, mc, &mut c) } != 0).then_some(c)
}

/// `biome2str`.
pub fn biome_name(mc: i32, id: i32) -> Option<&'static str> {
    // SAFETY: plain integer arguments; the result is a static literal or NULL.
    static_str(unsafe { biome2str(mc, id) })
}

/// `biomeExists`.
pub fn biome_exists(mc: i32, id: i32) -> bool {
    // SAFETY: plain integer arguments.
    unsafe { biomeExists(mc, id) != 0 }
}

/// `initBiomeColors`: the engine's map colours by biome id.
pub fn biome_colors() -> [[u8; 3]; 256] {
    let mut colors = [[0u8; 3]; 256];
    // SAFETY: the engine fills exactly 256 entries of three bytes.
    unsafe { initBiomeColors(colors.as_mut_ptr()) };
    colors
}

/// `mc2str`.
pub fn mc_name(mc: i32) -> Option<&'static str> {
    // SAFETY: a plain integer argument; the result is a static literal.
    static_str(unsafe { mc2str(mc) }).filter(|s| *s != "?")
}

/// `str2mc`.
pub fn mc_from_name(s: &str) -> i32 {
    let Ok(s) = CString::new(s) else {
        return 0;
    };
    // SAFETY: `s` is NUL-terminated and outlives the call.
    unsafe { str2mc(s.as_ptr()) }
}

/// `isSlimeChunk` (an inline function of `finders.h`, see `c/shim.c`).
pub fn is_slime_chunk(seed: u64, cx: i32, cz: i32) -> bool {
    // SAFETY: plain integer arguments.
    unsafe { meridian_is_slime_chunk(seed, cx, cz) != 0 }
}

/// A `sax_context`: one seed's generators. It is read-only after
/// `sax_create_versioned`, so it may be used from several threads at once.
#[derive(Debug)]
pub struct Context(NonNull<SaxContext>);

// SAFETY: the C functions only read a context after it is created
// (`seedatlas_sax.h`); generators that the engine modifies temporarily are
// copied first.
unsafe impl Send for Context {}
// SAFETY: as for Send.
unsafe impl Sync for Context {}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: created by sax_create_versioned and dropped once.
        unsafe { sax_destroy(self.0.as_ptr()) }
    }
}

/// A length for a C capacity (the outputs never exceed `i32`).
fn cap(len: usize) -> i32 {
    len.min(i32::MAX as usize) as i32
}

impl Context {
    /// `sax_create_versioned`; `None` for versions it does not know.
    pub fn new(seed: i64, large_biomes: bool, mc_version: &str) -> Option<Self> {
        let name = CString::new(mc_version).ok()?;
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let p = unsafe { sax_create_versioned(seed, u32::from(large_biomes), name.as_ptr()) };
        NonNull::new(p).map(Self)
    }

    fn ptr(&self) -> *const SaxContext {
        self.0.as_ptr()
    }

    pub fn biome_at(&self, dim: i32, x: i32, y: i32, z: i32) -> Result<i32, i32> {
        let mut id = 0;
        // SAFETY: a valid context and an output of one i32.
        let rc = unsafe { sax_biome_at(self.ptr(), dim, x, y, z, &mut id) };
        if rc < 0 { Err(rc) } else { Ok(id) }
    }

    pub fn biome_chunk(
        &self,
        dim: i32,
        cx: i32,
        cz: i32,
        y: i32,
        step: i32,
    ) -> Result<[i32; 256], i32> {
        let mut ids = [0i32; 256];
        // SAFETY: SAX_CHUNK_SAMPLE_COUNT ids; no colours.
        let rc = unsafe {
            sax_biome_chunk_sampled(
                self.ptr(),
                dim,
                cx,
                cz,
                y,
                step,
                ids.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if rc < 0 { Err(rc) } else { Ok(ids) }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn biome_area(
        &self,
        dim: i32,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        out: &mut [i32],
    ) -> i32 {
        // SAFETY: `out` holds `capacity` ids; no colours.
        unsafe {
            sax_biome_area_sampled(
                self.ptr(),
                dim,
                ox,
                oz,
                y,
                step,
                w,
                h,
                out.as_mut_ptr(),
                std::ptr::null_mut(),
                cap(out.len()),
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn height_area(
        &self,
        dim: i32,
        ox: i32,
        oz: i32,
        step: i32,
        w: i32,
        h: i32,
        out: &mut [f32],
    ) -> i32 {
        // SAFETY: `out` holds `capacity` heights.
        unsafe {
            sax_height_area_sampled(
                self.ptr(),
                dim,
                ox,
                oz,
                step,
                w,
                h,
                out.as_mut_ptr(),
                cap(out.len()),
            )
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn nether_floor_area(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: Option<&[u64]>,
        out: &mut [f32],
    ) -> i32 {
        // SAFETY: `out` holds `capacity` heights, `carved` its words.
        unsafe {
            match carved {
                None => sax_nether_floor_area_sampled(
                    self.ptr(),
                    ox,
                    oz,
                    y,
                    step,
                    w,
                    h,
                    out.as_mut_ptr(),
                    cap(out.len()),
                ),
                Some(m) => sax_nether_floor_area_carved_sampled(
                    self.ptr(),
                    ox,
                    oz,
                    y,
                    step,
                    w,
                    h,
                    m.as_ptr(),
                    cap(m.len()),
                    out.as_mut_ptr(),
                    cap(out.len()),
                ),
            }
        }
    }

    pub fn has_terrain(&self) -> bool {
        // SAFETY: a valid context.
        unsafe { sax_context_has_terrain(self.ptr()) != 0 }
    }

    /// `sax_scan_structures` into `out` (`SAX_RESULT_STRIDE` values a row).
    pub fn scan_structures(
        &self,
        kind: i32,
        b: [i32; 4],
        check_biome: bool,
        out: &mut [i32],
    ) -> i32 {
        // SAFETY: `out` holds `capacity` rows of six values.
        unsafe {
            sax_scan_structures(
                self.ptr(),
                kind,
                b[0],
                b[1],
                b[2],
                b[3],
                i32::from(check_biome),
                out.as_mut_ptr(),
                cap(out.len() / 6),
            )
        }
    }

    pub fn scan_strongholds(&self, b: [i32; 4], check_biome: bool, out: &mut [i32]) -> i32 {
        // SAFETY: `out` holds `capacity` rows of six values.
        unsafe {
            sax_scan_strongholds(
                self.ptr(),
                b[0],
                b[1],
                b[2],
                b[3],
                i32::from(check_biome),
                out.as_mut_ptr(),
                cap(out.len() / 6),
            )
        }
    }

    pub fn scan_slime_chunks(&self, b: [i32; 4], out: &mut [i32]) -> i32 {
        // SAFETY: `out` holds `capacity` rows of six values.
        unsafe {
            sax_scan_slime_chunks(
                self.ptr(),
                b[0],
                b[1],
                b[2],
                b[3],
                out.as_mut_ptr(),
                cap(out.len() / 6),
            )
        }
    }

    pub fn spawn(&self, detailed: bool) -> Result<[i32; 3], i32> {
        let mut xyz = [0i32; 3];
        // SAFETY: an output of three i32.
        let rc = unsafe { sax_spawn(self.ptr(), i32::from(detailed), xyz.as_mut_ptr()) };
        if rc < 0 { Err(rc) } else { Ok(xyz) }
    }
}

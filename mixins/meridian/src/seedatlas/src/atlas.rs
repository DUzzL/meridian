//! The Seed Atlas API: the engine's C ABI for the mods (`seedatlas_sax.h`,
//! ABI 6), which the Fabric version calls as well, call for call.

use std::sync::Arc;

use crate::Dimension;
use crate::ffi::Context;
use crate::structures::{config, kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtlasError {
    Argument,
    RangeTooLarge,
    Unsupported,
    /// The engine could not allocate its buffers.
    Allocation,
    /// The engine failed to generate the area.
    Generation,
}

impl AtlasError {
    /// The `sax_error` of a negative return code.
    fn from_code(rc: i32) -> Self {
        match rc {
            -1 => AtlasError::Argument,
            -2 => AtlasError::Allocation,
            -3 => AtlasError::Unsupported,
            -4 => AtlasError::RangeTooLarge,
            _ => AtlasError::Generation,
        }
    }
}

/// Most samples one area call may produce.
pub const MAX_AREA_SAMPLES: i64 = 4 * 1024 * 1024;

/// One seed's generators for the three dimensions (a `sax_context`, shared
/// by the clones).
#[derive(Clone, Debug)]
pub struct Atlas {
    pub seed: i64,
    pub large_biomes: bool,
    /// The Minecraft version generated (`crate::mc`).
    pub mc: i32,
    context: Arc<Context>,
}

impl Atlas {
    /// `sax_create`: the newest version.
    pub fn new(seed: i64, large_biomes: bool) -> Self {
        Self::versioned(seed, large_biomes, crate::mc::MC_NEWEST)
    }

    /// `sax_create_versioned` with a version already resolved (`mc::str2mc`).
    /// Versions the interface does not offer (before 1.1) get the newest.
    pub fn versioned(seed: i64, large_biomes: bool, mc: i32) -> Self {
        let named = crate::mc::mc2str(mc);
        if let Some(c) = Context::new(seed, large_biomes, named) {
            return Self {
                seed,
                large_biomes,
                mc,
                context: Arc::new(c),
            };
        }
        let newest = crate::mc::MC_NEWEST;
        let c = Context::new(seed, large_biomes, crate::mc::mc2str(newest))
            .expect("the interface generates the newest version");
        Self {
            seed,
            large_biomes,
            mc: newest,
            context: Arc::new(c),
        }
    }

    /// `sax_biome_at`: the biome of one block (`biomes::NONE` on errors).
    pub fn biome_at(&self, dimension: Dimension, x: i32, y: i32, z: i32) -> i32 {
        self.context
            .biome_at(dimension.id(), x, y, z)
            .unwrap_or(crate::biomes::NONE)
    }

    /// `sax_biome_area_sampled`: a `w` x `h` grid of samples `step` blocks
    /// apart from the block (`ox`, `oz`), row-major.
    #[allow(clippy::too_many_arguments)]
    pub fn biome_area(
        &self,
        dimension: Dimension,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<i32>, AtlasError> {
        if w <= 0 || h <= 0 {
            return Err(AtlasError::Argument);
        }
        let count = w as i64 * h as i64;
        if count > MAX_AREA_SAMPLES {
            return Err(AtlasError::RangeTooLarge);
        }
        let mut out = vec![0i32; count as usize];
        let rc = self
            .context
            .biome_area(dimension.id(), ox, oz, y, step, w, h, &mut out);
        if rc < 0 {
            return Err(AtlasError::from_code(rc));
        }
        Ok(out)
    }

    /// `sax_biome_chunk_sampled`: one chunk's 16 x 16 blocks, each
    /// `step`-sized cell filled with the biome at its centre.
    pub fn biome_chunk(
        &self,
        dimension: Dimension,
        cx: i32,
        cz: i32,
        y: i32,
        step: i32,
    ) -> Result<[i32; 256], AtlasError> {
        self.context
            .biome_chunk(dimension.id(), cx, cz, y, step)
            .map_err(AtlasError::from_code)
    }
}

// ----------------------------------------------------------- structures --

/// Marker kinds beyond the engine's structures (the Fabric interface's).
pub mod marker {
    pub const STRONGHOLD: i32 = 1001;
    pub const SPAWN: i32 = 1002;
    pub const SLIME_CHUNK: i32 = 1003;
    pub const ORE_VEIN_COPPER: i32 = 1004;
    pub const ORE_VEIN_IRON: i32 = 1005;
    pub const END_SHIP: i32 = 1006;
    pub const ORE_VEIN: i32 = 1007;
}

pub const RESULT_BIOME_CHECKED: i32 = 1;
pub const RESULT_BIOME_VIABLE: i32 = 2;
pub const RESULT_APPROXIMATE: i32 = 4;
pub const RESULT_FIXED: i32 = 8;
pub const CAMP_BIOME_MASK: i32 = 0xff;
pub const CAMP_SPECIAL_LOOT: i32 = 1 << 8;
/// Most cells (regions, chunks) one scan may cover.
pub const MAX_SCAN_CELLS: i64 = 4_194_304;

/// One found start or marker (`SAX_RESULT_STRIDE` row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub kind: i32,
    pub x: i32,
    pub y: Option<i32>,
    pub z: i32,
    pub flags: i32,
    pub detail: i32,
}

/// `SAX_UNKNOWN_Y`.
const UNKNOWN_Y: i32 = i32::MIN;

/// Rows a scan writes at first; scans without a cap grow it as needed.
const FIRST_ROWS: usize = 256;

/// Runs a scan into a buffer of `cap` rows (more on truncation where `cap` is
/// unbounded) and turns its rows into hits. A scan returns the rows written,
/// or one more than its capacity when it stopped there.
fn collect(cap: usize, scan: impl Fn(&mut [i32]) -> i32) -> Result<Vec<Hit>, AtlasError> {
    // a scan needs room for one row to report its errors
    let mut rows = cap.clamp(1, FIRST_ROWS);
    loop {
        let mut buf = vec![0i32; rows * 6];
        let rc = scan(&mut buf);
        if rc < 0 {
            return Err(AtlasError::from_code(rc));
        }
        let truncated = rc as usize > rows;
        if truncated && rows < cap {
            rows = rows.saturating_mul(4).min(cap);
            continue;
        }
        let n = (rc as usize).min(rows).min(cap);
        return Ok(buf[..n * 6]
            .chunks_exact(6)
            .map(|r| Hit {
                kind: r[0],
                x: r[1],
                y: (r[2] != UNKNOWN_Y).then_some(r[2]),
                z: r[3],
                flags: r[4],
                detail: r[5],
            })
            .collect());
    }
}

impl Atlas {
    /// `sax_scan_structures`: starts of `structure` inside the block box
    /// `[min_x, min_z, max_x, max_z]`, biome-checked when asked.
    pub fn scan_structures(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
    ) -> Result<Vec<Hit>, AtlasError> {
        self.scan_structures_capped(structure, b, check_biome, usize::MAX)
    }

    /// [`Atlas::scan_structures`] with the interface's `capacity`: the scan
    /// stops at `cap` results, the first ones in scan order.
    pub fn scan_structures_capped(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        collect(cap, |buf| {
            self.context.scan_structures(structure, b, check_biome, buf)
        })
    }

    /// `sax_scan_strongholds`: strongholds in the box, relocated by biome
    /// when asked (exact), else their ring estimates.
    pub fn scan_strongholds(&self, b: [i32; 4], check_biome: bool) -> Result<Vec<Hit>, AtlasError> {
        collect(usize::MAX, |buf| {
            self.context.scan_strongholds(b, check_biome, buf)
        })
    }

    /// `sax_spawn`.
    pub fn spawn(&self, detailed: bool) -> (i32, i32) {
        self.context
            .spawn(detailed)
            .map_or((0, 0), |xyz| (xyz[0], xyz[2]))
    }

    /// `sax_scan_slime_chunks`: slime chunks (their north-west block) in
    /// the box.
    pub fn scan_slime_chunks(&self, b: [i32; 4]) -> Result<Vec<Hit>, AtlasError> {
        self.scan_slime_chunks_capped(b, usize::MAX)
    }

    /// [`Atlas::scan_slime_chunks`], stopping at `cap` results.
    pub fn scan_slime_chunks_capped(
        &self,
        b: [i32; 4],
        cap: usize,
    ) -> Result<Vec<Hit>, AtlasError> {
        collect(cap, |buf| self.context.scan_slime_chunks(b, buf))
    }
}

// --------------------------------------------------- complete scans --

impl Atlas {
    /// The blocks one candidate cell of a scan covers (a structure's region,
    /// an ore vein cell, a chunk), if the kind scans by cells.
    fn scan_cell(&self, structure: i32) -> Option<i32> {
        if [
            marker::ORE_VEIN,
            marker::ORE_VEIN_COPPER,
            marker::ORE_VEIN_IRON,
        ]
        .contains(&structure)
        {
            return Some(128);
        }
        if structure == marker::SLIME_CHUNK {
            return Some(16);
        }
        let engine = if structure == marker::END_SHIP {
            kind::END_CITY
        } else {
            structure
        };
        if !(1..kind::COUNT).contains(&engine) || structure == kind::END_ISLAND {
            return None;
        }
        config(engine, self.mc).map(|c| c.region_size as i32 * 16)
    }
}

/// Candidate cells a complete scan checks at most, for kinds placed per
/// chunk or ore-vein cell (cheap: a random draw per cell) and for the rest
/// (a biome check per cell). Large enough for every kind up to the radius
/// the map shows it at (`meridian_core::seedatlas::visible_at_zoom`): End
/// gateways to 14336 blocks, monuments and ancient cities to 65536.
const COMPLETE_CELLS_CHEAP: i64 = MAX_SCAN_CELLS;
const COMPLETE_CELLS: i64 = 1 << 17;

impl Atlas {
    /// Whether `scan_complete` scans the box for `structure` at all: its
    /// candidate cells are within the budget.
    pub fn scan_fits(&self, structure: i32, b: [i32; 4]) -> bool {
        let Some(cell) = self.scan_cell(structure) else {
            return true;
        };
        let w = (b[2] as i64 - b[0] as i64) / cell as i64 + 1;
        let h = (b[3] as i64 - b[1] as i64) / cell as i64 + 1;
        let limit = if cell <= 128 {
            COMPLETE_CELLS_CHEAP
        } else {
            COMPLETE_CELLS
        };
        w * h <= limit
    }

    /// Every `structure` in the box, or `None` when the box is too large to
    /// scan whole for it (see [`Atlas::scan_fits`]) or holds more than
    /// `cap`: the map then leaves the kind out at that zoom rather than
    /// showing some of them (markers are never thinned or merged).
    pub fn scan_complete(
        &self,
        structure: i32,
        b: [i32; 4],
        check_biome: bool,
        cap: usize,
    ) -> Result<Option<Vec<Hit>>, AtlasError> {
        if b[0] > b[2] || b[1] > b[3] {
            return Err(AtlasError::Argument);
        }
        if !self.scan_fits(structure, b) {
            return Ok(None);
        }
        let all = if structure == marker::SLIME_CHUNK {
            self.scan_slime_chunks_capped(b, cap + 1)?
        } else {
            self.scan_structures_capped(structure, b, check_biome, cap + 1)?
        };
        Ok((all.len() <= cap).then_some(all))
    }
}

// ------------------------------------------------- terrain (ABI 6, 1.18+) --

impl Atlas {
    /// `sax_context_has_terrain`.
    pub fn has_terrain(&self) -> bool {
        self.context.has_terrain()
    }

    /// An output buffer for `w` x `h` samples.
    fn samples(w: i32, h: i32) -> Result<Vec<f32>, AtlasError> {
        if w <= 0 || h <= 0 {
            return Err(AtlasError::Argument);
        }
        let count = w as i64 * h as i64;
        if count > MAX_AREA_SAMPLES {
            return Err(AtlasError::RangeTooLarge);
        }
        Ok(vec![0f32; count as usize])
    }

    /// `sax_height_area_sampled`: terrain heights of the Overworld and the
    /// End (0 is void there).
    #[allow(clippy::too_many_arguments)]
    pub fn height_area(
        &self,
        dimension: Dimension,
        ox: i32,
        oz: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<f32>, AtlasError> {
        let mut out = Self::samples(w, h)?;
        let rc = self
            .context
            .height_area(dimension.id(), ox, oz, step, w, h, &mut out);
        if rc < 0 {
            return Err(AtlasError::from_code(rc));
        }
        Ok(out)
    }

    /// `sax_nether_floor_area_sampled`.
    #[allow(clippy::too_many_arguments)]
    pub fn nether_floor_area(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
    ) -> Result<Vec<f32>, AtlasError> {
        self.nether_floor(ox, oz, y, step, w, h, None)
    }

    /// `sax_nether_floor_area_carved_sampled`: two mask words a sample, bit
    /// y marks carved air.
    #[allow(clippy::too_many_arguments)]
    pub fn nether_floor_area_carved(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: &[u64],
    ) -> Result<Vec<f32>, AtlasError> {
        self.nether_floor(ox, oz, y, step, w, h, Some(carved))
    }

    #[allow(clippy::too_many_arguments)]
    fn nether_floor(
        &self,
        ox: i32,
        oz: i32,
        y: i32,
        step: i32,
        w: i32,
        h: i32,
        carved: Option<&[u64]>,
    ) -> Result<Vec<f32>, AtlasError> {
        let mut out = Self::samples(w, h)?;
        let rc = self
            .context
            .nether_floor_area(ox, oz, y, step, w, h, carved, &mut out);
        if rc < 0 {
            return Err(AtlasError::from_code(rc));
        }
        Ok(out)
    }
}

//! The random number generators of Minecraft as the engine models them:
//! Java's linear congruential generator, Xoroshiro128++ and the layer seed
//! pipeline. All arithmetic wraps, as the original's unsigned C does.

/// Java's `java.util.Random` state (48 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JavaRandom(pub u64);

const MASK48: u64 = (1 << 48) - 1;
const MULT: u64 = 0x5_deec_e66d;

impl JavaRandom {
    pub fn new(seed: u64) -> Self {
        Self((seed ^ MULT) & MASK48)
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.0 = (seed ^ MULT) & MASK48;
    }

    pub fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(MULT).wrapping_add(0xb)) & MASK48;
        ((self.0 as i64) >> (48 - bits)) as i32
    }

    pub fn next_int(&mut self, n: i32) -> i32 {
        let m = n.wrapping_sub(1);
        if m & n == 0 {
            let x = (n as u64).wrapping_mul(self.next(31) as u64);
            return ((x as i64) >> 31) as i32;
        }
        loop {
            let bits = self.next(31);
            let val = bits % n;
            if ((bits as u32)
                .wrapping_sub(val as u32)
                .wrapping_add(m as u32) as i32)
                >= 0
            {
                return val;
            }
        }
    }

    pub fn next_long(&mut self) -> u64 {
        let hi = (self.next(32) as i64 as u64) << 32;
        hi.wrapping_add(self.next(32) as i64 as u64)
    }

    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1 << 24) as f32
    }

    pub fn next_double(&mut self) -> f64 {
        let mut x = self.next(26) as i64 as u64;
        x <<= 27;
        x = x.wrapping_add(self.next(27) as i64 as u64);
        (x as i64) as f64 / (1u64 << 53) as f64
    }

    /// Jumps ahead as if `next` were called `n` times.
    pub fn skip(&mut self, n: u64) {
        let (mut m, mut a) = (1u64, 0u64);
        let (mut im, mut ia) = (MULT, 0xbu64);
        let mut k = n;
        while k != 0 {
            if k & 1 != 0 {
                m = m.wrapping_mul(im);
                a = im.wrapping_mul(a).wrapping_add(ia);
            }
            ia = im.wrapping_add(1).wrapping_mul(ia);
            im = im.wrapping_mul(im);
            k >>= 1;
        }
        self.0 = self.0.wrapping_mul(m).wrapping_add(a) & MASK48;
    }
}

/// Xoroshiro128++ as Minecraft seeds it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Xoroshiro {
    pub lo: u64,
    pub hi: u64,
}

impl Xoroshiro {
    pub fn new(value: u64) -> Self {
        const XL: u64 = 0x9e37_79b9_7f4a_7c15;
        const XH: u64 = 0x6a09_e667_f3bc_c909;
        const A: u64 = 0xbf58_476d_1ce4_e5b9;
        const B: u64 = 0x94d0_49bb_1331_11eb;
        let mut l = value ^ XH;
        let mut h = l.wrapping_add(XL);
        l = (l ^ (l >> 30)).wrapping_mul(A);
        h = (h ^ (h >> 30)).wrapping_mul(A);
        l = (l ^ (l >> 27)).wrapping_mul(B);
        h = (h ^ (h >> 27)).wrapping_mul(B);
        Self {
            lo: l ^ (l >> 31),
            hi: h ^ (h >> 31),
        }
    }

    pub fn next_long(&mut self) -> u64 {
        let (l, mut h) = (self.lo, self.hi);
        let n = l.wrapping_add(h).rotate_left(17).wrapping_add(l);
        h ^= l;
        self.lo = l.rotate_left(49) ^ h ^ (h << 21);
        self.hi = h.rotate_left(28);
        n
    }

    pub fn next_int(&mut self, n: u32) -> i32 {
        let mut r = (self.next_long() & 0xFFFF_FFFF).wrapping_mul(n as u64);
        if (r as u32) < n {
            while (r as u32) < (!n).wrapping_add(1) % n {
                r = (self.next_long() & 0xFFFF_FFFF).wrapping_mul(n as u64);
            }
        }
        (r >> 32) as i32
    }

    pub fn next_double(&mut self) -> f64 {
        (self.next_long() >> (64 - 53)) as f64 * 1.110_223_024_625_156_5e-16
    }

    pub fn next_float(&mut self) -> f32 {
        (self.next_long() >> (64 - 24)) as f32 * 5.960_464_5e-8_f32
    }

    pub fn skip(&mut self, count: usize) {
        for _ in 0..count {
            self.next_long();
        }
    }

    /// Java's `nextLong` on this generator (two 32-bit draws).
    pub fn next_long_j(&mut self) -> u64 {
        let a = (self.next_long() >> 32) as i32;
        let b = (self.next_long() >> 32) as i32;
        ((a as i64 as u64) << 32).wrapping_add(b as i64 as u64)
    }

    /// Java's `nextInt(n)` on this generator.
    pub fn next_int_j(&mut self, n: u32) -> i32 {
        let m = n.wrapping_sub(1) as i32;
        if (m & n as i32) == 0 {
            let x = (n as u64).wrapping_mul(self.next_long() >> 33);
            return ((x as i64) >> 31) as i32;
        }
        loop {
            let bits = (self.next_long() >> 33) as i32;
            let val = bits % n as i32;
            if ((bits as u32)
                .wrapping_sub(val as u32)
                .wrapping_add(m as u32) as i32)
                >= 0
            {
                return val;
            }
        }
    }
}

pub fn mc_step_seed(s: u64, salt: u64) -> u64 {
    s.wrapping_mul(
        s.wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407),
    )
    .wrapping_add(salt)
}

pub fn mc_first_int(s: u64, m: i32) -> i32 {
    let r = (((s as i64) >> 24) % m as i64) as i32;
    if r < 0 { r + m } else { r }
}

pub fn mc_first_is_zero(s: u64, m: i32) -> bool {
    (((s as i64) >> 24) % m as i64) as i32 == 0
}

pub fn chunk_seed(ss: u64, x: i32, z: i32) -> u64 {
    let mut cs = ss.wrapping_add(x as i64 as u64);
    cs = mc_step_seed(cs, z as i64 as u64);
    cs = mc_step_seed(cs, x as i64 as u64);
    mc_step_seed(cs, z as i64 as u64)
}

pub fn layer_salt(salt: u64) -> u64 {
    let ls = mc_step_seed(salt, salt);
    let ls = mc_step_seed(ls, salt);
    mc_step_seed(ls, salt)
}

pub fn start_salt(ws: u64, ls: u64) -> u64 {
    let st = mc_step_seed(ws, ls);
    let st = mc_step_seed(st, ls);
    mc_step_seed(st, ls)
}

pub fn start_seed(ws: u64, ls: u64) -> u64 {
    mc_step_seed(start_salt(ws, ls), 0)
}

pub fn floordiv(a: i32, b: i32) -> i32 {
    let q = a.wrapping_div(b);
    let r = a.wrapping_rem(b);
    q - (((a ^ b) < 0 && r != 0) as i32)
}

pub fn lerp(part: f64, from: f64, to: f64) -> f64 {
    from + part * (to - from)
}

pub fn lerp2(dx: f64, dy: f64, v00: f64, v10: f64, v01: f64, v11: f64) -> f64 {
    lerp(dy, lerp(dx, v00, v10), lerp(dx, v01, v11))
}

#[allow(clippy::too_many_arguments)]
pub fn lerp3(
    dx: f64,
    dy: f64,
    dz: f64,
    v000: f64,
    v100: f64,
    v010: f64,
    v110: f64,
    v001: f64,
    v101: f64,
    v011: f64,
    v111: f64,
) -> f64 {
    let a = lerp2(dx, dy, v000, v100, v010, v110);
    let b = lerp2(dx, dy, v001, v101, v011, v111);
    lerp(dz, a, b)
}

pub fn clamped_lerp(part: f64, from: f64, to: f64) -> f64 {
    if part <= 0.0 {
        from
    } else if part >= 1.0 {
        to
    } else {
        lerp(part, from, to)
    }
}

/// The modular inverse of `x` mod `m` (co-prime, below 2^63).
pub fn mul_inv(mut x: u64, mut m: u64) -> u64 {
    if (m as i64) <= 1 {
        return 0;
    }
    let n = m;
    let (mut a, mut b) = (0u64, 1u64);
    while (x as i64) > 1 {
        if m == 0 {
            return 0;
        }
        let q = x / m;
        let t = m;
        m = x % m;
        x = t;
        let t = a;
        a = b.wrapping_sub(q.wrapping_mul(a));
        b = t;
    }
    if (b as i64) < 0 {
        b = b.wrapping_add(n);
    }
    b
}

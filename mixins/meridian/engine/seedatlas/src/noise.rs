//! Perlin, simplex, octave and double-Perlin noise as Minecraft computes
//! them, ported from the engine's `noise.c`.

use crate::rng::{JavaRandom, Xoroshiro, lerp};

#[derive(Clone, Debug)]
pub struct Perlin {
    pub d: [u8; 257],
    pub h2: u8,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub amplitude: f64,
    pub lacunarity: f64,
    pub d2: f64,
    pub t2: f64,
}

#[inline(always)]
fn indexed_lerp(idx: u8, a: f64, b: f64, c: f64) -> f64 {
    match idx & 0xf {
        0 => a + b,
        1 => -a + b,
        2 => a - b,
        3 => -a - b,
        4 => a + c,
        5 => -a + c,
        6 => a - c,
        7 => -a - c,
        8 => b + c,
        9 => -b + c,
        10 => b - c,
        11 => -b - c,
        12 => a + b,
        13 => -b + c,
        14 => -a + b,
        _ => -b - c,
    }
}

#[inline(always)]
fn fade(d: f64) -> f64 {
    d * d * d * (d * (d * 6.0 - 15.0) + 10.0)
}

impl Perlin {
    fn finish(a: f64, b: f64, c: f64, d: [u8; 257]) -> Self {
        let i2 = b.floor();
        let d2 = b - i2;
        Self {
            d,
            h2: (i2 as i32) as u8,
            a,
            b,
            c,
            amplitude: 1.0,
            lacunarity: 1.0,
            d2,
            t2: fade(d2),
        }
    }

    pub fn new_java(seed: &mut JavaRandom) -> Self {
        let a = seed.next_double() * 256.0;
        let b = seed.next_double() * 256.0;
        let c = seed.next_double() * 256.0;
        let mut d = [0u8; 257];
        for (i, v) in d.iter_mut().enumerate().take(256) {
            *v = i as u8;
        }
        for i in 0..256 {
            let j = seed.next_int(256 - i as i32) as usize + i;
            d.swap(i, j);
        }
        d[256] = d[0];
        Self::finish(a, b, c, d)
    }

    pub fn new_x(xr: &mut Xoroshiro) -> Self {
        let a = xr.next_double() * 256.0;
        let b = xr.next_double() * 256.0;
        let c = xr.next_double() * 256.0;
        let mut d = [0u8; 257];
        for (i, v) in d.iter_mut().enumerate().take(256) {
            *v = i as u8;
        }
        for i in 0..256 {
            let j = xr.next_int(256 - i as u32) as usize + i;
            d.swap(i, j);
        }
        d[256] = d[0];
        Self::finish(a, b, c, d)
    }

    pub fn sample(&self, mut d1: f64, mut d2: f64, mut d3: f64, yamp: f64, ymin: f64) -> f64 {
        let (h2, t2);
        if d2 == 0.0 {
            d2 = self.d2;
            h2 = self.h2;
            t2 = self.t2;
        } else {
            d2 += self.b;
            let i2 = d2.floor();
            d2 -= i2;
            h2 = (i2 as i32) as u8;
            t2 = fade(d2);
        }
        d1 += self.a;
        d3 += self.c;
        let i1 = d1.floor();
        let i3 = d3.floor();
        d1 -= i1;
        d3 -= i3;
        let h1 = (i1 as i32) as u8;
        let h3 = (i3 as i32) as u8;
        let t1 = fade(d1);
        let t3 = fade(d3);
        if yamp != 0.0 {
            let yclamp = if ymin < d2 { ymin } else { d2 };
            d2 -= (yclamp / yamp).floor() * yamp;
        }
        let idx = &self.d;
        let at = |i: u8| idx[i as usize];
        let at1 = |i: u8| idx[i as usize + 1];
        let (v1a, v1b) = (at(h1).wrapping_add(h2), at1(h1).wrapping_add(h2));
        let (v2a, v2b) = (at(v1a).wrapping_add(h3), at1(v1a).wrapping_add(h3));
        let (v3a, v3b) = (at(v1b).wrapping_add(h3), at1(v1b).wrapping_add(h3));
        let (v4a, v4b) = (at(v2a), at1(v2a));
        let (v5a, v5b) = (at(v2b), at1(v2b));
        let (v6a, v6b) = (at(v3a), at1(v3a));
        let (v7a, v7b) = (at(v3b), at1(v3b));
        let mut l1 = indexed_lerp(v4a, d1, d2, d3);
        let mut l5 = indexed_lerp(v4b, d1, d2, d3 - 1.0);
        let l2 = indexed_lerp(v6a, d1 - 1.0, d2, d3);
        let l6 = indexed_lerp(v6b, d1 - 1.0, d2, d3 - 1.0);
        let mut l3 = indexed_lerp(v5a, d1, d2 - 1.0, d3);
        let mut l7 = indexed_lerp(v5b, d1, d2 - 1.0, d3 - 1.0);
        let l4 = indexed_lerp(v7a, d1 - 1.0, d2 - 1.0, d3);
        let l8 = indexed_lerp(v7b, d1 - 1.0, d2 - 1.0, d3 - 1.0);
        l1 = lerp(t1, l1, l2);
        l3 = lerp(t1, l3, l4);
        l5 = lerp(t1, l5, l6);
        l7 = lerp(t1, l7, l8);
        l1 = lerp(t2, l1, l3);
        l5 = lerp(t2, l5, l7);
        lerp(t3, l1, l5)
    }

    pub fn sample_simplex_2d(&self, x: f64, y: f64) -> f64 {
        let skew = 0.5 * (3f64.sqrt() - 1.0);
        let unskew = (3.0 - 3f64.sqrt()) / 6.0;
        let hf = (x + y) * skew;
        let hx = (x + hf).floor() as i32;
        let hz = (y + hf).floor() as i32;
        let mhxz = (hx + hz) as f64 * unskew;
        let x0 = x - (hx as f64 - mhxz);
        let y0 = y - (hz as f64 - mhxz);
        let offx = (x0 > y0) as i32;
        let offz = 1 - offx;
        let x1 = x0 - offx as f64 + unskew;
        let y1 = y0 - offz as f64 + unskew;
        let x2 = x0 - 1.0 + 2.0 * unskew;
        let y2 = y0 - 1.0 + 2.0 * unskew;
        let d = |i: i32| self.d[(i & 0xff) as usize] as i32;
        let gi0 = d(d(hz) + hx);
        let gi1 = d(d(hz + offz) + hx + offx);
        let gi2 = d(d(hz + 1) + hx + 1);
        let mut t = 0.0;
        t += simplex_grad(gi0 % 12, x0, y0, 0.0, 0.5);
        t += simplex_grad(gi1 % 12, x1, y1, 0.0, 0.5);
        t += simplex_grad(gi2 % 12, x2, y2, 0.0, 0.5);
        70.0 * t
    }
}

fn simplex_grad(idx: i32, x: f64, y: f64, z: f64, d: f64) -> f64 {
    let mut con = d - x * x - y * y - z * z;
    if con < 0.0 {
        return 0.0;
    }
    con *= con;
    con * con * indexed_lerp(idx as u8, x, y, z)
}

#[derive(Clone, Debug, Default)]
pub struct Octave {
    pub octaves: Vec<Perlin>,
}

const MD5_OCTAVE_N: [[u64; 2]; 13] = [
    [0xb198de63a8012672, 0x7b84cad43ef7b5a8],
    [0x0fd787bfbc403ec3, 0x74a4a31ca21b48b8],
    [0x36d326eed40efeb2, 0x5be9ce18223c636a],
    [0x082fe255f8be6631, 0x4e96119e22dedc81],
    [0x0ef68ec68504005e, 0x48b6bf93a2789640],
    [0xf11268128982754f, 0x257a1d670430b0aa],
    [0xe51c98ce7d1de664, 0x5f9478a733040c45],
    [0x6d7b49e7e429850a, 0x2e3063c622a24777],
    [0xbd90d5377ba1b762, 0xc07317d419a7548d],
    [0x53d39c6752dac858, 0xbcd1c5a80ab65b3e],
    [0xb4a24d7a84e7677b, 0x023ff9668e89b5c4],
    [0xdffa22b534c5f608, 0xb9b67517d3665ca9],
    [0xd50708086cef4d7c, 0x6e1651ecc7f43309],
];

impl Octave {
    /// Java-random octaves `omin..omin+len` (`octaveInit`).
    pub fn new_java(seed: &mut JavaRandom, omin: i32, len: i32) -> Self {
        let end = omin + len - 1;
        let mut persist = 1.0 / ((1i64 << len) as f64 - 1.0);
        let mut lacuna = 2f64.powi(end);
        assert!(len >= 1 && end <= 0, "unsupported octave range");
        let mut octaves = Vec::with_capacity(len as usize);
        if end == 0 {
            let mut p = Perlin::new_java(seed);
            p.amplitude = persist;
            p.lacunarity = lacuna;
            octaves.push(p);
            persist *= 2.0;
            lacuna *= 0.5;
        } else {
            seed.skip((-end * 262) as u64);
        }
        while (octaves.len() as i32) < len {
            let mut p = Perlin::new_java(seed);
            p.amplitude = persist;
            p.lacunarity = lacuna;
            octaves.push(p);
            persist *= 2.0;
            lacuna *= 0.5;
        }
        Self { octaves }
    }

    /// Xoroshiro octaves (`xOctaveInit`); at most `nmax` when positive.
    pub fn new_x(xr: &mut Xoroshiro, amplitudes: &[f64], omin: i32, nmax: i32) -> Self {
        const LACUNA_INI: [f64; 13] = [
            1.0,
            0.5,
            0.25,
            1. / 8.,
            1. / 16.,
            1. / 32.,
            1. / 64.,
            1. / 128.,
            1. / 256.,
            1. / 512.,
            1. / 1024.,
            1. / 2048.,
            1. / 4096.,
        ];
        const PERSIST_INI: [f64; 10] = [
            0.0,
            1.0,
            2. / 3.,
            4. / 7.,
            8. / 15.,
            16. / 31.,
            32. / 63.,
            64. / 127.,
            128. / 255.,
            256. / 511.,
        ];
        let len = amplitudes.len();
        let mut lacuna = LACUNA_INI[(-omin) as usize];
        let mut persist = PERSIST_INI[len];
        let xlo = xr.next_long();
        let xhi = xr.next_long();
        let mut octaves = Vec::new();
        let mut i = 0;
        while i < len && octaves.len() as i32 != nmax {
            if amplitudes[i] != 0.0 {
                let m = MD5_OCTAVE_N[(12 + omin + i as i32) as usize];
                let mut pxr = Xoroshiro {
                    lo: xlo ^ m[0],
                    hi: xhi ^ m[1],
                };
                let mut p = Perlin::new_x(&mut pxr);
                p.amplitude = amplitudes[i] * persist;
                p.lacunarity = lacuna;
                octaves.push(p);
            }
            i += 1;
            lacuna *= 2.0;
            persist *= 0.5;
        }
        Self { octaves }
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let mut v = 0.0;
        for p in &self.octaves {
            let lf = p.lacunarity;
            v += p.amplitude * p.sample(x * lf, y * lf, z * lf, 0.0, 0.0);
        }
        v
    }

    pub fn sample_amp(&self, x: f64, y: f64, z: f64, yamp: f64, ymin: f64, ydefault: bool) -> f64 {
        let mut v = 0.0;
        for p in &self.octaves {
            let lf = p.lacunarity;
            let ay = if ydefault { -p.b } else { y * lf };
            v += p.amplitude * p.sample(x * lf, ay, z * lf, yamp * lf, ymin * lf);
        }
        v
    }
}

#[derive(Clone, Debug, Default)]
pub struct DoublePerlin {
    pub amplitude: f64,
    pub a: Octave,
    pub b: Octave,
}

impl DoublePerlin {
    /// Java-random double Perlin noise (`doublePerlinInit`).
    pub fn new_java(seed: &mut JavaRandom, omin: i32, len: i32) -> Self {
        let amplitude = (10.0 / 6.0) * len as f64 / (len + 1) as f64;
        let a = Octave::new_java(seed, omin, len);
        let b = Octave::new_java(seed, omin, len);
        Self { amplitude, a, b }
    }

    /// Xoroshiro double Perlin noise (`xDoublePerlinInit`); `nmax` octaves
    /// in all when positive.
    pub fn new_x(xr: &mut Xoroshiro, amplitudes: &[f64], omin: i32, nmax: i32) -> Self {
        let (na, nb) = if nmax > 0 {
            let na = (nmax + 1) >> 1;
            (na, nmax - na)
        } else {
            (-1, -1)
        };
        let a = Octave::new_x(xr, amplitudes, omin, na);
        let b = Octave::new_x(xr, amplitudes, omin, nb);
        let mut len = amplitudes.len();
        let mut i = amplitudes.len();
        while i > 0 && amplitudes[i - 1] == 0.0 {
            len -= 1;
            i -= 1;
        }
        let mut i = 0;
        while amplitudes[i] == 0.0 {
            len -= 1;
            i += 1;
        }
        const AMP_INI: [f64; 10] = [
            0.0,
            5. / 6.,
            10. / 9.,
            15. / 12.,
            20. / 15.,
            25. / 18.,
            30. / 21.,
            35. / 24.,
            40. / 27.,
            45. / 30.,
        ];
        Self {
            amplitude: AMP_INI[len],
            a,
            b,
        }
    }

    pub fn sample(&self, x: f64, y: f64, z: f64) -> f64 {
        let f = 337.0 / 331.0;
        (self.a.sample(x, y, z) + self.b.sample(x * f, y * f, z * f)) * self.amplitude
    }
}

//! Minecraft versions as the engine numbers them (`enum MCVersion` in
//! `biomes.h`), so they compare as in the C code (`mc <= MC_1_17`).
#![allow(non_upper_case_globals)]

pub const MC_UNDEF: i32 = 0;
pub const MC_B1_7: i32 = 1;
pub const MC_B1_8: i32 = 2;
pub const MC_1_0_0: i32 = 3;
pub const MC_1_0: i32 = 3;
pub const MC_1_1_0: i32 = 4;
pub const MC_1_1: i32 = 4;
pub const MC_1_2_5: i32 = 5;
pub const MC_1_2: i32 = 5;
pub const MC_1_3_2: i32 = 6;
pub const MC_1_3: i32 = 6;
pub const MC_1_4_7: i32 = 7;
pub const MC_1_4: i32 = 7;
pub const MC_1_5_2: i32 = 8;
pub const MC_1_5: i32 = 8;
pub const MC_1_6_4: i32 = 9;
pub const MC_1_6: i32 = 9;
pub const MC_1_7_10: i32 = 10;
pub const MC_1_7: i32 = 10;
pub const MC_1_8_9: i32 = 11;
pub const MC_1_8: i32 = 11;
pub const MC_1_9_4: i32 = 12;
pub const MC_1_9: i32 = 12;
pub const MC_1_10_2: i32 = 13;
pub const MC_1_10: i32 = 13;
pub const MC_1_11_2: i32 = 14;
pub const MC_1_11: i32 = 14;
pub const MC_1_12_2: i32 = 15;
pub const MC_1_12: i32 = 15;
pub const MC_1_13_2: i32 = 16;
pub const MC_1_13: i32 = 16;
pub const MC_1_14_4: i32 = 17;
pub const MC_1_14: i32 = 17;
pub const MC_1_15_2: i32 = 18;
pub const MC_1_15: i32 = 18;
pub const MC_1_16_1: i32 = 19;
pub const MC_1_16_5: i32 = 20;
pub const MC_1_16: i32 = 20;
pub const MC_1_17_1: i32 = 21;
pub const MC_1_17: i32 = 21;
pub const MC_1_18_2: i32 = 22;
pub const MC_1_18: i32 = 22;
pub const MC_1_19_2: i32 = 23;
pub const MC_1_19_4: i32 = 24;
pub const MC_1_19: i32 = 24;
pub const MC_1_20_6: i32 = 25;
pub const MC_1_20: i32 = 25;
pub const MC_1_21_1: i32 = 26;
pub const MC_1_21_2: i32 = 27;
pub const MC_1_21_3: i32 = 28;
pub const MC_1_21_4: i32 = 29;
pub const MC_1_21_5: i32 = 30;
pub const MC_1_21_6: i32 = 31;
pub const MC_1_21_7: i32 = 32;
pub const MC_1_21_8: i32 = 33;
pub const MC_1_21_9: i32 = 34;
pub const MC_1_21_10: i32 = 35;
pub const MC_1_21_11: i32 = 36;
pub const MC_1_21: i32 = 36;
pub const MC_26_1: i32 = 37;
pub const MC_26_1_1: i32 = 38;
pub const MC_26_1_2: i32 = 39;
pub const MC_26_2: i32 = 40;
pub const MC_26_3: i32 = 41;
pub const MC_NEWEST: i32 = MC_26_3;

/// `mc2str`: the version's name as the Seed Atlas app shows it ("?" for
/// numbers the engine does not know).
pub fn mc2str(mc: i32) -> &'static str {
    crate::ffi::mc_name(mc).unwrap_or("?")
}

/// `str2mc`: a version by name (`1.12`, `1.12.2`, `26.3-pre-2`...), or
/// `MC_UNDEF`.
pub fn str2mc(s: &str) -> i32 {
    crate::ffi::mc_from_name(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        assert_eq!(str2mc("1.12"), MC_1_12_2);
        assert_eq!(str2mc("1.12.2"), MC_1_12);
        assert_eq!(str2mc("26.3"), MC_NEWEST);
        assert_eq!(str2mc("1.16.1"), MC_1_16_1);
        assert_eq!(mc2str(MC_1_16), "1.16");
        assert_eq!(str2mc("0.9"), MC_UNDEF);
        const { assert!(MC_1_17 < MC_1_18 && MC_B1_8 < MC_1_0) };
    }

    /// The constants here are the engine's numbers: every name the engine
    /// gives a number leads back to it.
    #[test]
    fn constants_match_the_engine() {
        for mc in MC_B1_7..=MC_NEWEST {
            assert_ne!(mc2str(mc), "?", "{mc}");
            assert_eq!(str2mc(mc2str(mc)), mc, "{}", mc2str(mc));
        }
        assert_eq!(mc2str(MC_26_3), "26.3");
        assert_eq!(mc2str(MC_1_18), "1.18");
        assert_eq!(mc2str(MC_1_21_4), "1.21.4");
        assert_eq!(mc2str(MC_NEWEST + 1), "?");
    }
}

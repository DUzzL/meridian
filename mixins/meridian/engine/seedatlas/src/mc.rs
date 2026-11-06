//! Minecraft versions as the engine numbers them (`enum MCVersion` in
//! `biomes.h`), so the port compares them as the C code does (`mc <= MC_1_17`).
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

/// `mc2str`: the version's name as the Seed Atlas app shows it.
pub fn mc2str(mc: i32) -> &'static str {
    match mc {
        MC_B1_7 => "Beta 1.7",
        MC_B1_8 => "Beta 1.8",
        MC_1_0_0 => "1.0",
        MC_1_1_0 => "1.1",
        MC_1_2_5 => "1.2",
        MC_1_3_2 => "1.3",
        MC_1_4_7 => "1.4",
        MC_1_5_2 => "1.5",
        MC_1_6_4 => "1.6",
        MC_1_7_10 => "1.7",
        MC_1_8_9 => "1.8",
        MC_1_9_4 => "1.9",
        MC_1_10_2 => "1.10",
        MC_1_11_2 => "1.11",
        MC_1_12_2 => "1.12",
        MC_1_13_2 => "1.13",
        MC_1_14_4 => "1.14",
        MC_1_15_2 => "1.15",
        MC_1_16_1 => "1.16.1",
        MC_1_16_5 => "1.16",
        MC_1_17_1 => "1.17",
        MC_1_18_2 => "1.18",
        MC_1_19_2 => "1.19.2",
        MC_1_19_4 => "1.19",
        MC_1_20_6 => "1.20",
        MC_1_21_1 => "1.21.1",
        MC_1_21_2 => "1.21.2",
        MC_1_21_3 => "1.21.3",
        MC_1_21_4 => "1.21.4",
        MC_1_21_5 => "1.21.5",
        MC_1_21_6 => "1.21.6",
        MC_1_21_7 => "1.21.7",
        MC_1_21_8 => "1.21.8",
        MC_1_21_9 => "1.21.9",
        MC_1_21_10 => "1.21.10",
        MC_1_21_11 => "1.21",
        MC_26_1 => "26.1",
        MC_26_1_1 => "26.1.1",
        MC_26_1_2 => "26.1.2",
        MC_26_2 => "26.2",
        MC_26_3 => "26.3",
        _ => "?",
    }
}

/// `str2mc`: a version by name (`1.12`, `1.12.2`, `26.3-pre-2`...), or
/// `MC_UNDEF`.
pub fn str2mc(s: &str) -> i32 {
    match s {
        "26.3 Pre-Release 2" | "26.3-pre-2" => MC_26_3,
        "1.21 WD" => MC_1_21_4,
        "Beta 1.8" => MC_B1_8,
        "Beta 1.7" => MC_B1_7,
        "1.0.0" | "1.0" => MC_1_0_0,
        "1.1.0" | "1.1" => MC_1_1_0,
        "1.2.5" | "1.2" => MC_1_2_5,
        "1.3.2" | "1.3" => MC_1_3_2,
        "1.4.7" | "1.4" => MC_1_4_7,
        "1.5.2" | "1.5" => MC_1_5_2,
        "1.6.4" | "1.6" => MC_1_6_4,
        "1.7.10" | "1.7" => MC_1_7_10,
        "1.8.9" | "1.8" => MC_1_8_9,
        "1.9.4" | "1.9" => MC_1_9_4,
        "1.10.2" | "1.10" => MC_1_10_2,
        "1.11.2" | "1.11" => MC_1_11_2,
        "1.12.2" | "1.12" => MC_1_12_2,
        "1.13.2" | "1.13" => MC_1_13_2,
        "1.14.4" | "1.14" => MC_1_14_4,
        "1.15.2" | "1.15" => MC_1_15_2,
        "1.16.1" => MC_1_16_1,
        "1.16.5" | "1.16" => MC_1_16_5,
        "1.17.1" | "1.17" => MC_1_17_1,
        "1.18.2" | "1.18" => MC_1_18_2,
        "1.19.2" => MC_1_19_2,
        "1.19.4" | "1.19" => MC_1_19_4,
        "1.20.6" | "1.20" => MC_1_20_6,
        "1.21.1" => MC_1_21_1,
        "1.21.2" => MC_1_21_2,
        "1.21.3" => MC_1_21_3,
        "1.21.4" => MC_1_21_4,
        "1.21.5" => MC_1_21_5,
        "1.21.6" => MC_1_21_6,
        "1.21.7" => MC_1_21_7,
        "1.21.8" => MC_1_21_8,
        "1.21.9" => MC_1_21_9,
        "1.21.10" => MC_1_21_10,
        "1.21.11" | "1.21" => MC_1_21_11,
        "26.1" => MC_26_1,
        "26.1.1" => MC_26_1_1,
        "26.1.2" => MC_26_1_2,
        "26.2" => MC_26_2,
        "26.3" => MC_26_3,
        _ => MC_UNDEF,
    }
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
}

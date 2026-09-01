use crate::stats::{RunDensity, categorize_measure_density};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::ops::Deref;

#[derive(Default)]
struct U64MixHasher(u64);

impl U64MixHasher {
    #[inline(always)]
    fn mix(mut value: u64) -> u64 {
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }
}

impl Hasher for U64MixHasher {
    #[inline(always)]
    fn finish(&self) -> u64 {
        self.0
    }

    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        let mut value = 0u64;
        for (shift, &byte) in bytes.iter().take(8).enumerate() {
            value |= u64::from(byte) << (shift * 8);
        }
        self.0 = Self::mix(value);
    }

    #[inline(always)]
    fn write_u64(&mut self, value: u64) {
        self.0 = Self::mix(value);
    }
}

type BpmCountsMap = HashMap<u64, [usize; 4], BuildHasherDefault<U64MixHasher>>;

/// Sorted difficulty table for efficient bound queries.
type DifficultyMeasures = [(i32, i32); 13];
type DifficultyTable = [(i32, DifficultyMeasures); 43];

const MIN_BPM_KEY: i32 = 80;
const MAX_BPM_KEY: i32 = 500;
const BPM_KEY_STEP: i32 = 10;
const HASH_AGGREGATION_MIN_SEGMENTS: usize = 32;
// Caps speculative profile reservation at one 4 KiB page of 16-byte inputs.
const MAX_PROFILE_RESERVE: usize = 256;
const MAX_MATRIX_MEASURES: usize = 512;
const MEASURE_LOOKUP_LEN: usize = MAX_MATRIX_MEASURES + 1;
const NO_RANGE_END: u8 = u8::MAX;

/// Static difficulty table for matrix rating interpolation.
const DIFFICULTY_TABLE: DifficultyTable = [
    (
        80,
        [
            (8, 7),
            (12, 7),
            (16, 8),
            (24, 8),
            (32, 9),
            (48, 9),
            (64, 9),
            (96, 10),
            (128, 10),
            (192, 10),
            (256, 10),
            (384, 11),
            (512, 11),
        ],
    ),
    (
        90,
        [
            (8, 7),
            (12, 8),
            (16, 8),
            (24, 9),
            (32, 9),
            (48, 9),
            (64, 10),
            (96, 10),
            (128, 11),
            (192, 11),
            (256, 11),
            (384, 12),
            (512, 12),
        ],
    ),
    (
        100,
        [
            (8, 8),
            (12, 8),
            (16, 9),
            (24, 9),
            (32, 10),
            (48, 10),
            (64, 10),
            (96, 11),
            (128, 11),
            (192, 11),
            (256, 11),
            (384, 12),
            (512, 12),
        ],
    ),
    (
        110,
        [
            (8, 8),
            (12, 9),
            (16, 9),
            (24, 10),
            (32, 10),
            (48, 10),
            (64, 11),
            (96, 11),
            (128, 12),
            (192, 12),
            (256, 12),
            (384, 13),
            (512, 13),
        ],
    ),
    (
        120,
        [
            (8, 9),
            (12, 9),
            (16, 10),
            (24, 10),
            (32, 11),
            (48, 11),
            (64, 12),
            (96, 12),
            (128, 12),
            (192, 13),
            (256, 13),
            (384, 13),
            (512, 13),
        ],
    ),
    (
        130,
        [
            (8, 9),
            (12, 10),
            (16, 10),
            (24, 11),
            (32, 11),
            (48, 12),
            (64, 12),
            (96, 13),
            (128, 13),
            (192, 13),
            (256, 14),
            (384, 14),
            (512, 14),
        ],
    ),
    (
        140,
        [
            (8, 10),
            (12, 10),
            (16, 11),
            (24, 11),
            (32, 12),
            (48, 12),
            (64, 13),
            (96, 13),
            (128, 13),
            (192, 14),
            (256, 14),
            (384, 14),
            (512, 15),
        ],
    ),
    (
        150,
        [
            (8, 10),
            (12, 11),
            (16, 11),
            (24, 12),
            (32, 12),
            (48, 13),
            (64, 13),
            (96, 14),
            (128, 14),
            (192, 15),
            (256, 15),
            (384, 15),
            (512, 16),
        ],
    ),
    (
        160,
        [
            (8, 11),
            (12, 11),
            (16, 12),
            (24, 12),
            (32, 12),
            (48, 13),
            (64, 14),
            (96, 14),
            (128, 15),
            (192, 15),
            (256, 16),
            (384, 16),
            (512, 16),
        ],
    ),
    (
        170,
        [
            (8, 11),
            (12, 12),
            (16, 12),
            (24, 13),
            (32, 13),
            (48, 14),
            (64, 14),
            (96, 15),
            (128, 15),
            (192, 16),
            (256, 16),
            (384, 17),
            (512, 17),
        ],
    ),
    (
        180,
        [
            (8, 12),
            (12, 12),
            (16, 13),
            (24, 13),
            (32, 13),
            (48, 14),
            (64, 15),
            (96, 15),
            (128, 16),
            (192, 16),
            (256, 17),
            (384, 17),
            (512, 18),
        ],
    ),
    (
        190,
        [
            (8, 12),
            (12, 13),
            (16, 13),
            (24, 14),
            (32, 14),
            (48, 15),
            (64, 15),
            (96, 16),
            (128, 17),
            (192, 17),
            (256, 18),
            (384, 18),
            (512, 19),
        ],
    ),
    (
        200,
        [
            (8, 13),
            (12, 13),
            (16, 14),
            (24, 14),
            (32, 15),
            (48, 15),
            (64, 16),
            (96, 17),
            (128, 17),
            (192, 18),
            (256, 19),
            (384, 19),
            (512, 20),
        ],
    ),
    (
        210,
        [
            (8, 13),
            (12, 14),
            (16, 14),
            (24, 15),
            (32, 15),
            (48, 16),
            (64, 17),
            (96, 18),
            (128, 18),
            (192, 19),
            (256, 20),
            (384, 20),
            (512, 21),
        ],
    ),
    (
        220,
        [
            (8, 14),
            (12, 14),
            (16, 15),
            (24, 16),
            (32, 16),
            (48, 17),
            (64, 18),
            (96, 19),
            (128, 19),
            (192, 20),
            (256, 21),
            (384, 22),
            (512, 22),
        ],
    ),
    (
        230,
        [
            (8, 14),
            (12, 15),
            (16, 16),
            (24, 16),
            (32, 17),
            (48, 18),
            (64, 19),
            (96, 20),
            (128, 20),
            (192, 21),
            (256, 22),
            (384, 22),
            (512, 23),
        ],
    ),
    (
        240,
        [
            (8, 15),
            (12, 16),
            (16, 16),
            (24, 17),
            (32, 18),
            (48, 19),
            (64, 20),
            (96, 21),
            (128, 22),
            (192, 23),
            (256, 23),
            (384, 24),
            (512, 24),
        ],
    ),
    (
        250,
        [
            (8, 16),
            (12, 17),
            (16, 18),
            (24, 18),
            (32, 19),
            (48, 20),
            (64, 21),
            (96, 22),
            (128, 23),
            (192, 24),
            (256, 24),
            (384, 25),
            (512, 25),
        ],
    ),
    (
        260,
        [
            (8, 17),
            (12, 18),
            (16, 19),
            (24, 19),
            (32, 21),
            (48, 22),
            (64, 23),
            (96, 23),
            (128, 24),
            (192, 25),
            (256, 25),
            (384, 26),
            (512, 26),
        ],
    ),
    (
        270,
        [
            (8, 18),
            (12, 19),
            (16, 20),
            (24, 21),
            (32, 22),
            (48, 23),
            (64, 24),
            (96, 25),
            (128, 25),
            (192, 26),
            (256, 26),
            (384, 27),
            (512, 27),
        ],
    ),
    (
        280,
        [
            (8, 19),
            (12, 20),
            (16, 21),
            (24, 22),
            (32, 23),
            (48, 24),
            (64, 25),
            (96, 26),
            (128, 26),
            (192, 27),
            (256, 27),
            (384, 28),
            (512, 28),
        ],
    ),
    (
        290,
        [
            (8, 20),
            (12, 21),
            (16, 22),
            (24, 23),
            (32, 24),
            (48, 25),
            (64, 26),
            (96, 27),
            (128, 27),
            (192, 28),
            (256, 28),
            (384, 29),
            (512, 29),
        ],
    ),
    (
        300,
        [
            (8, 21),
            (12, 22),
            (16, 23),
            (24, 24),
            (32, 24),
            (48, 25),
            (64, 26),
            (96, 27),
            (128, 28),
            (192, 29),
            (256, 30),
            (384, 30),
            (512, 30),
        ],
    ),
    (
        310,
        [
            (8, 22),
            (12, 23),
            (16, 24),
            (24, 24),
            (32, 25),
            (48, 26),
            (64, 27),
            (96, 28),
            (128, 29),
            (192, 29),
            (256, 30),
            (384, 31),
            (512, 31),
        ],
    ),
    (
        320,
        [
            (8, 22),
            (12, 23),
            (16, 24),
            (24, 25),
            (32, 26),
            (48, 27),
            (64, 28),
            (96, 29),
            (128, 30),
            (192, 30),
            (256, 31),
            (384, 32),
            (512, 32),
        ],
    ),
    (
        330,
        [
            (8, 23),
            (12, 24),
            (16, 25),
            (24, 26),
            (32, 26),
            (48, 28),
            (64, 29),
            (96, 30),
            (128, 31),
            (192, 31),
            (256, 32),
            (384, 32),
            (512, 33),
        ],
    ),
    (
        340,
        [
            (8, 24),
            (12, 25),
            (16, 26),
            (24, 27),
            (32, 27),
            (48, 29),
            (64, 30),
            (96, 31),
            (128, 31),
            (192, 32),
            (256, 32),
            (384, 33),
            (512, 34),
        ],
    ),
    (
        350,
        [
            (8, 25),
            (12, 26),
            (16, 27),
            (24, 28),
            (32, 28),
            (48, 30),
            (64, 30),
            (96, 31),
            (128, 32),
            (192, 33),
            (256, 33),
            (384, 34),
            (512, 35),
        ],
    ),
    (
        360,
        [
            (8, 26),
            (12, 27),
            (16, 27),
            (24, 28),
            (32, 29),
            (48, 30),
            (64, 31),
            (96, 32),
            (128, 33),
            (192, 34),
            (256, 34),
            (384, 35),
            (512, 36),
        ],
    ),
    (
        370,
        [
            (8, 27),
            (12, 28),
            (16, 28),
            (24, 29),
            (32, 30),
            (48, 32),
            (64, 32),
            (96, 33),
            (128, 34),
            (192, 34),
            (256, 35),
            (384, 36),
            (512, 37),
        ],
    ),
    (
        380,
        [
            (8, 28),
            (12, 29),
            (16, 29),
            (24, 30),
            (32, 31),
            (48, 33),
            (64, 34),
            (96, 34),
            (128, 35),
            (192, 36),
            (256, 36),
            (384, 37),
            (512, 38),
        ],
    ),
    (
        390,
        [
            (8, 29),
            (12, 30),
            (16, 31),
            (24, 32),
            (32, 33),
            (48, 34),
            (64, 35),
            (96, 35),
            (128, 36),
            (192, 37),
            (256, 37),
            (384, 38),
            (512, 39),
        ],
    ),
    (
        400,
        [
            (8, 30),
            (12, 31),
            (16, 32),
            (24, 33),
            (32, 34),
            (48, 35),
            (64, 36),
            (96, 37),
            (128, 37),
            (192, 38),
            (256, 39),
            (384, 39),
            (512, 40),
        ],
    ),
    (
        410,
        [
            (8, 31),
            (12, 32),
            (16, 33),
            (24, 34),
            (32, 35),
            (48, 36),
            (64, 37),
            (96, 38),
            (128, 38),
            (192, 39),
            (256, 40),
            (384, 40),
            (512, 41),
        ],
    ),
    (
        420,
        [
            (8, 32),
            (12, 33),
            (16, 34),
            (24, 35),
            (32, 36),
            (48, 37),
            (64, 38),
            (96, 39),
            (128, 39),
            (192, 40),
            (256, 41),
            (384, 42),
            (512, 42),
        ],
    ),
    (
        430,
        [
            (8, 33),
            (12, 34),
            (16, 35),
            (24, 36),
            (32, 37),
            (48, 38),
            (64, 39),
            (96, 39),
            (128, 40),
            (192, 41),
            (256, 42),
            (384, 43),
            (512, 43),
        ],
    ),
    (
        440,
        [
            (8, 34),
            (12, 35),
            (16, 36),
            (24, 37),
            (32, 38),
            (48, 39),
            (64, 40),
            (96, 40),
            (128, 41),
            (192, 42),
            (256, 43),
            (384, 44),
            (512, 44),
        ],
    ),
    (
        450,
        [
            (8, 35),
            (12, 36),
            (16, 37),
            (24, 38),
            (32, 39),
            (48, 40),
            (64, 40),
            (96, 41),
            (128, 42),
            (192, 43),
            (256, 44),
            (384, 45),
            (512, 45),
        ],
    ),
    (
        460,
        [
            (8, 36),
            (12, 37),
            (16, 38),
            (24, 39),
            (32, 40),
            (48, 41),
            (64, 41),
            (96, 42),
            (128, 43),
            (192, 44),
            (256, 45),
            (384, 46),
            (512, 46),
        ],
    ),
    (
        470,
        [
            (8, 37),
            (12, 38),
            (16, 39),
            (24, 40),
            (32, 41),
            (48, 42),
            (64, 42),
            (96, 43),
            (128, 44),
            (192, 45),
            (256, 46),
            (384, 47),
            (512, 47),
        ],
    ),
    (
        480,
        [
            (8, 38),
            (12, 39),
            (16, 40),
            (24, 41),
            (32, 42),
            (48, 43),
            (64, 43),
            (96, 44),
            (128, 45),
            (192, 46),
            (256, 47),
            (384, 48),
            (512, 48),
        ],
    ),
    (
        490,
        [
            (8, 39),
            (12, 40),
            (16, 41),
            (24, 42),
            (32, 43),
            (48, 44),
            (64, 44),
            (96, 45),
            (128, 46),
            (192, 47),
            (256, 48),
            (384, 49),
            (512, 49),
        ],
    ),
    (
        500,
        [
            (8, 40),
            (12, 41),
            (16, 42),
            (24, 43),
            (32, 44),
            (48, 45),
            (64, 45),
            (96, 46),
            (128, 47),
            (192, 48),
            (256, 49),
            (384, 50),
            (512, 50),
        ],
    ),
];

const fn build_measure_index() -> [[u8; MEASURE_LOOKUP_LEN]; DIFFICULTY_TABLE.len()] {
    let mut lookup = [[0u8; MEASURE_LOOKUP_LEN]; DIFFICULTY_TABLE.len()];
    let mut row = 0usize;
    while row < DIFFICULTY_TABLE.len() {
        let measures = &DIFFICULTY_TABLE[row].1;
        let mut measure = 0usize;
        let mut index = 0usize;
        while measure < MEASURE_LOOKUP_LEN {
            while index + 1 < measures.len() && measures[index + 1].0 <= measure as i32 {
                index += 1;
            }
            lookup[row][measure] = index as u8;
            measure += 1;
        }
        row += 1;
    }
    lookup
}

const fn build_difficulty_ranges() -> [[(u8, u8); 13]; DIFFICULTY_TABLE.len()] {
    let mut ranges = [[(0u8, NO_RANGE_END); 13]; DIFFICULTY_TABLE.len()];
    let mut row = 0usize;
    while row < DIFFICULTY_TABLE.len() {
        let measures = &DIFFICULTY_TABLE[row].1;
        let mut index = 0usize;
        while index < measures.len() {
            let difficulty = measures[index].1;
            let mut start = index;
            while start > 0 && measures[start - 1].1 == difficulty {
                start -= 1;
            }
            let mut end = index + 1;
            while end < measures.len() && measures[end].1 <= difficulty {
                end += 1;
            }
            ranges[row][index] = (
                start as u8,
                if end < measures.len() {
                    end as u8
                } else {
                    NO_RANGE_END
                },
            );
            index += 1;
        }
        row += 1;
    }
    ranges
}

// Compile-time, immutable 23 KiB lookup: no startup or per-rating allocation.
static MEASURE_INDEX: [[u8; MEASURE_LOOKUP_LEN]; DIFFICULTY_TABLE.len()] = build_measure_index();
static DIFFICULTY_RANGES: [[(u8, u8); 13]; DIFFICULTY_TABLE.len()] = build_difficulty_ranges();

/// Computes downward extrapolation for low measures.
#[inline(always)]
fn extrapolate_downward(measures: f64, min_measure_key: f64, min_difficulty: f64) -> f64 {
    let adjustment = (min_measure_key / measures).ln();
    (min_difficulty - adjustment).max(0.0)
}

/// Computes logarithmic interpolation within a range.
#[inline(always)]
fn interpolate_log(
    measures: f64,
    range_start_m: f64,
    range_end_m: f64,
    base_difficulty: f64,
) -> f64 {
    if measures <= range_start_m {
        return base_difficulty;
    }
    let log_progress =
        (measures.ln() - range_start_m.ln()) / (range_end_m.ln() - range_start_m.ln());
    base_difficulty + log_progress
}

/// Computes scaling for plateau regions.
#[inline(always)]
fn scale_plateau(measures: f64, plateau_start_m: f64, base_difficulty: f64) -> f64 {
    if measures <= plateau_start_m {
        return base_difficulty;
    }
    let scaling_factor = (measures / plateau_start_m).ln();
    base_difficulty + scaling_factor
}

/// Calculates difficulty for a given BPM row, handling extrapolation and plateaus.
fn calculate_difficulty_for_bpm(measures: f64, row: usize) -> f64 {
    if measures <= 0.0 {
        return 0.0;
    }

    let bpm_data = &DIFFICULTY_TABLE[row].1;
    let min_measure_key = f64::from(bpm_data[0].0);
    if measures < min_measure_key {
        return extrapolate_downward(measures, min_measure_key, f64::from(bpm_data[0].1));
    }
    if measures.is_nan() {
        return nan_difficulty(measures, bpm_data);
    }

    let measure_idx = (measures as usize).min(MAX_MATRIX_MEASURES);
    let base_idx = usize::from(MEASURE_INDEX[row][measure_idx]);
    let base_difficulty = bpm_data[base_idx].1;
    let (range_start_idx, range_end_idx) = DIFFICULTY_RANGES[row][base_idx];
    let range_start = f64::from(bpm_data[usize::from(range_start_idx)].0);

    if range_end_idx == NO_RANGE_END {
        scale_plateau(measures, range_start, f64::from(base_difficulty))
    } else {
        let range_end = f64::from(bpm_data[usize::from(range_end_idx)].0);
        interpolate_log(measures, range_start, range_end, f64::from(base_difficulty))
    }
}

#[cold]
fn nan_difficulty(measures: f64, bpm_data: &[(i32, i32)]) -> f64 {
    let range_end = f64::from(bpm_data[0].0);
    interpolate_log(measures, 0.0, range_end, 0.0)
}

/// Finds bounding BPMs for interpolation without collecting all keys.
#[inline(always)]
fn find_bounding_bpms(bpm: f64, table: &DifficultyTable) -> (i32, i32) {
    debug_assert_eq!(table[0].0, MIN_BPM_KEY);
    debug_assert_eq!(table[table.len() - 1].0, MAX_BPM_KEY);

    if bpm > f64::from(MAX_BPM_KEY) {
        return (MAX_BPM_KEY - BPM_KEY_STEP, MAX_BPM_KEY);
    }

    if bpm < f64::from(MIN_BPM_KEY) {
        return (MIN_BPM_KEY, MIN_BPM_KEY + BPM_KEY_STEP);
    }

    let bpm_i = bpm as i32;
    if bpm_i < MIN_BPM_KEY {
        return (0, MIN_BPM_KEY);
    }

    let offset = bpm_i - MIN_BPM_KEY;
    let bpm1 = MIN_BPM_KEY + (offset / BPM_KEY_STEP) * BPM_KEY_STEP;
    let bpm2 = if offset % BPM_KEY_STEP == 0 {
        bpm1
    } else {
        bpm1 + BPM_KEY_STEP
    };
    (bpm1, bpm2)
}

#[inline(always)]
fn bpm_row_index(bpm: i32) -> Option<usize> {
    ((MIN_BPM_KEY..=MAX_BPM_KEY).contains(&bpm) && (bpm - MIN_BPM_KEY) % BPM_KEY_STEP == 0)
        .then_some(((bpm - MIN_BPM_KEY) / BPM_KEY_STEP) as usize)
}

/// Interpolates difficulty between two BPM rows.
pub fn get_difficulty(bpm: f64, measures: f64) -> f64 {
    let (bpm1, bpm2) = find_bounding_bpms(bpm, &DIFFICULTY_TABLE);

    let diff_at_bpm1 =
        bpm_row_index(bpm1).map_or(0.0, |row| calculate_difficulty_for_bpm(measures, row));

    if bpm1 == bpm2 {
        return diff_at_bpm1;
    }

    let diff_at_bpm2 =
        bpm_row_index(bpm2).map_or(0.0, |row| calculate_difficulty_for_bpm(measures, row));

    let bpm_range = f64::from(bpm2 - bpm1);
    if bpm_range == 0.0 {
        return diff_at_bpm1;
    }

    let bpm_progress = (bpm - f64::from(bpm1)) / bpm_range;
    (diff_at_bpm2 - diff_at_bpm1).mul_add(bpm_progress, diff_at_bpm1)
}

/// Computes effective BPM multiplier based on run density.
#[inline(always)]
const fn get_density_multiplier(category: RunDensity) -> f64 {
    match category {
        RunDensity::Run16 => 1.0,
        RunDensity::Run20 => 1.25,
        RunDensity::Run24 => 1.5,
        RunDensity::Run32 => 2.0,
        RunDensity::Break => 0.0,
    }
}

#[inline(always)]
const fn density_code(category: RunDensity) -> u8 {
    match category {
        RunDensity::Run16 => 0,
        RunDensity::Run20 => 1,
        RunDensity::Run24 => 2,
        RunDensity::Run32 => 3,
        RunDensity::Break => u8::MAX,
    }
}

#[inline(always)]
const fn density_from_code(code: u8) -> RunDensity {
    match code {
        0 => RunDensity::Run16,
        1 => RunDensity::Run20,
        2 => RunDensity::Run24,
        3 => RunDensity::Run32,
        _ => RunDensity::Break,
    }
}

/// Minimal input needed to reevaluate one Matrix rating candidate at another music rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatrixRatingInput {
    pub effective_bpm: f64,
    pub measures: usize,
}

const EMPTY_MATRIX_INPUT: MatrixRatingInput = MatrixRatingInput {
    effective_bpm: 0.0,
    measures: 0,
};

/// Immutable inputs needed to reevaluate a chart's Matrix rating.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatrixProfile(Box<[MatrixRatingInput]>);

impl MatrixProfile {
    /// Returns the rating inputs as a contiguous slice.
    #[must_use]
    pub fn as_slice(&self) -> &[MatrixRatingInput] {
        &self.0
    }

    /// Reevaluates this profile after scaling its effective BPMs by `music_rate`.
    #[must_use]
    pub fn rating_at_rate(&self, music_rate: f64) -> f64 {
        matrix_rating_at_valid_rate(&self.0, valid_music_rate(music_rate))
    }
}

impl AsRef<[MatrixRatingInput]> for MatrixProfile {
    fn as_ref(&self) -> &[MatrixRatingInput] {
        self.as_slice()
    }
}

impl Deref for MatrixProfile {
    type Target = [MatrixRatingInput];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

fn sort_matrix_inputs(inputs: &mut [MatrixRatingInput]) {
    inputs.sort_unstable_by(|left, right| {
        left.effective_bpm
            .total_cmp(&right.effective_bpm)
            .then(left.measures.cmp(&right.measures))
    });
}

/// Finds the maximum difficulty rating from stream sections.
pub fn compute_matrix_rating(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> f64 {
    let mut best = 0.0f64;
    for_each_matrix_input(measure_densities, bpm_map, |input| {
        best = best.max(get_difficulty(input.effective_bpm, input.measures as f64));
    });
    best
}

/// Builds the compact inputs needed to reevaluate a chart's Matrix rating at any music rate.
pub fn compute_matrix_profile(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
) -> MatrixProfile {
    if measure_densities.is_empty() || bpm_map.is_empty() {
        return MatrixProfile::default();
    }
    if bpm_map.len() == 1 {
        let mut inputs = [EMPTY_MATRIX_INPUT; 4];
        let mut len = 0usize;
        fixed_matrix_inputs(measure_densities, bpm_map[0].1, &mut |input| {
            inputs[len] = input;
            len += 1;
        });
        return box_bounded_profile(inputs, len);
    }
    if bpm_map.len() < HASH_AGGREGATION_MIN_SEGMENTS {
        let mut inputs = [EMPTY_MATRIX_INPUT; 4 * (HASH_AGGREGATION_MIN_SEGMENTS - 1)];
        let mut len = 0usize;
        small_matrix_inputs(measure_densities, bpm_map, &mut |input| {
            inputs[len] = input;
            len += 1;
        });
        return box_bounded_profile(inputs, len);
    }

    let mut profile = Vec::with_capacity(matrix_profile_capacity(measure_densities, bpm_map));
    hashed_matrix_inputs(measure_densities, bpm_map, &mut |input| profile.push(input));
    box_matrix_profile(profile)
}

fn matrix_profile_capacity(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> usize {
    measure_densities
        .len()
        .min(bpm_map.len().saturating_mul(4))
        .min(MAX_PROFILE_RESERVE)
}

fn box_bounded_profile<const N: usize>(
    mut inputs: [MatrixRatingInput; N],
    len: usize,
) -> MatrixProfile {
    let inputs = &mut inputs[..len];
    sort_matrix_inputs(inputs);
    let len = dedup_matrix_inputs(inputs);
    MatrixProfile(Box::from(&inputs[..len]))
}

fn box_matrix_profile(mut inputs: Vec<MatrixRatingInput>) -> MatrixProfile {
    sort_matrix_inputs(&mut inputs);
    inputs.dedup();
    MatrixProfile(inputs.into_boxed_slice())
}

fn dedup_matrix_inputs(inputs: &mut [MatrixRatingInput]) -> usize {
    if inputs.is_empty() {
        return 0;
    }
    let mut unique = 1usize;
    for index in 1..inputs.len() {
        if inputs[index] != inputs[unique - 1] {
            inputs[unique] = inputs[index];
            unique += 1;
        }
    }
    unique
}

/// Reevaluates a compact Matrix profile after scaling its effective BPMs by `music_rate`.
pub fn matrix_rating_at_rate(profile: &[MatrixRatingInput], music_rate: f64) -> f64 {
    matrix_rating_at_valid_rate(profile, valid_music_rate(music_rate))
}

#[inline(always)]
fn valid_music_rate(music_rate: f64) -> f64 {
    if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    }
}

#[inline(always)]
fn matrix_rating_at_valid_rate(profile: &[MatrixRatingInput], rate: f64) -> f64 {
    profile.iter().fold(0.0f64, |best, input| {
        best.max(get_difficulty(
            input.effective_bpm * rate,
            input.measures as f64,
        ))
    })
}

fn for_each_matrix_input(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    mut emit: impl FnMut(MatrixRatingInput),
) {
    if measure_densities.is_empty() || bpm_map.is_empty() {
        return;
    }
    if bpm_map.len() == 1 {
        fixed_matrix_inputs(measure_densities, bpm_map[0].1, &mut emit);
    } else if bpm_map.len() < HASH_AGGREGATION_MIN_SEGMENTS {
        small_matrix_inputs(measure_densities, bpm_map, &mut emit);
    } else {
        hashed_matrix_inputs(measure_densities, bpm_map, &mut emit);
    }
}

fn hashed_matrix_inputs(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    emit: &mut impl FnMut(MatrixRatingInput),
) {
    let mut bpm_counts =
        BpmCountsMap::with_capacity_and_hasher(bpm_map.len(), BuildHasherDefault::default());

    let (mut bpm_idx, mut next_beat) = (0usize, bpm_map.get(1).map_or(f64::INFINITY, |m| m.0));
    for (idx, &density) in measure_densities.iter().enumerate() {
        let beat = idx as f64 * 4.0;
        while beat >= next_beat {
            bpm_idx += 1;
            next_beat = bpm_map.get(bpm_idx + 1).map_or(f64::INFINITY, |m| m.0);
        }

        let code = density_code(categorize_measure_density(density));
        let bpm = bpm_map[bpm_idx].1;
        if code != u8::MAX && bpm > 0.0 {
            bpm_counts.entry(bpm.to_bits()).or_default()[code as usize] += 1;
        }
    }

    for (bpm_bits, counts) in bpm_counts {
        emit_matrix_counts(bpm_bits, counts, emit);
    }
}

fn small_matrix_inputs(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    emit: &mut impl FnMut(MatrixRatingInput),
) {
    debug_assert!(bpm_map.len() < HASH_AGGREGATION_MIN_SEGMENTS);
    let mut bpm_bits = [0u64; HASH_AGGREGATION_MIN_SEGMENTS];
    let mut bpm_counts = [[0usize; 4]; HASH_AGGREGATION_MIN_SEGMENTS];
    let mut segment_ids = [usize::MAX; HASH_AGGREGATION_MIN_SEGMENTS];
    let mut unique_bpms = 0usize;

    for (segment_idx, &(_, bpm)) in bpm_map.iter().enumerate() {
        if bpm <= 0.0 {
            continue;
        }

        let bits = bpm.to_bits();
        let id = bpm_bits[..unique_bpms]
            .iter()
            .position(|&seen| seen == bits)
            .unwrap_or_else(|| {
                bpm_bits[unique_bpms] = bits;
                unique_bpms += 1;
                unique_bpms - 1
            });
        segment_ids[segment_idx] = id;
    }

    if unique_bpms == 0 {
        return;
    }

    let (mut segment_idx, mut next_beat) = (0usize, bpm_map.get(1).map_or(f64::INFINITY, |m| m.0));
    for (idx, &density) in measure_densities.iter().enumerate() {
        let beat = idx as f64 * 4.0;
        while beat >= next_beat {
            segment_idx += 1;
            next_beat = bpm_map.get(segment_idx + 1).map_or(f64::INFINITY, |m| m.0);
        }

        let code = density_code(categorize_measure_density(density));
        let bpm_id = segment_ids[segment_idx];
        if code != u8::MAX && bpm_id != usize::MAX {
            bpm_counts[bpm_id][code as usize] += 1;
        }
    }

    for index in 0..unique_bpms {
        emit_matrix_counts(bpm_bits[index], bpm_counts[index], emit);
    }
}

fn fixed_matrix_inputs(
    measure_densities: &[usize],
    bpm: f64,
    emit: &mut impl FnMut(MatrixRatingInput),
) {
    if bpm <= 0.0 {
        return;
    }

    let mut counts = [0usize; 4];
    for &density in measure_densities {
        let code = density_code(categorize_measure_density(density));
        if code != u8::MAX {
            counts[code as usize] += 1;
        }
    }
    emit_matrix_counts(bpm.to_bits(), counts, emit);
}

fn emit_matrix_counts(bpm_bits: u64, counts: [usize; 4], emit: &mut impl FnMut(MatrixRatingInput)) {
    let bpm = f64::from_bits(bpm_bits);
    for (code, count) in counts.into_iter().enumerate() {
        if count == 0 {
            continue;
        }
        let multiplier = get_density_multiplier(density_from_code(code as u8));
        let effective_bpm = bpm * multiplier;
        if effective_bpm > 0.0 {
            emit(MatrixRatingInput {
                effective_bpm,
                measures: count,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bpm::for_each_measure_bpm;

    #[test]
    fn difficulty_table_is_monotonic() {
        for &(bpm, ref row) in &DIFFICULTY_TABLE {
            assert!(
                row.windows(2)
                    .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 <= pair[1].1),
                "difficulty row invariant changed at {bpm} BPM"
            );
            assert!(
                row.last().is_some_and(|&(measures, _)| {
                    usize::try_from(measures).is_ok_and(|value| value <= MAX_MATRIX_MEASURES)
                }),
                "difficulty lookup is too short at {bpm} BPM"
            );
        }
    }

    fn matrix_rating_generic(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> f64 {
        let mut keys: Vec<(u8, u64)> = Vec::with_capacity(measure_densities.len());
        for_each_measure_bpm(measure_densities.len(), bpm_map, 4.0, |idx, bpm| {
            let code = density_code(categorize_measure_density(measure_densities[idx]));
            if code != u8::MAX && bpm > 0.0 {
                keys.push((code, bpm.to_bits()));
            }
        });

        keys.sort_unstable();
        let mut best = 0.0f64;
        let mut i = 0usize;
        while i < keys.len() {
            let (code, bpm_bits) = keys[i];
            let mut count = 1usize;
            i += 1;
            while i < keys.len() && keys[i] == (code, bpm_bits) {
                count += 1;
                i += 1;
            }

            let bpm = f64::from_bits(bpm_bits);
            let multiplier = get_density_multiplier(density_from_code(code));
            let effective_bpm = bpm * multiplier;
            if effective_bpm > 0.0 {
                best = best.max(get_difficulty(effective_bpm, count as f64));
            }
        }
        best
    }

    #[test]
    fn profile_uses_exact_size_boxed_storage() {
        assert!(MatrixProfile::default().is_empty());
        let profile = compute_matrix_profile(&[16, 16, 16, 16], &[(0.0, 180.0)]);
        assert_eq!(profile.len(), 1);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            std::mem::size_of::<MatrixProfile>(),
            std::mem::size_of::<Box<[MatrixRatingInput]>>()
        );
        #[cfg(target_pointer_width = "64")]
        assert!(
            std::mem::size_of::<MatrixProfile>() < std::mem::size_of::<Vec<MatrixRatingInput>>()
        );
    }

    #[test]
    fn fixed_bpm_matrix_matches_generic_path() {
        let densities = [0, 16, 17, 20, 23, 24, 31, 32, 48, 0, 12, 16];
        let bpm_map = [(0.0, 180.0)];
        assert_eq!(
            compute_matrix_rating(&densities, &bpm_map),
            matrix_rating_generic(&densities, &bpm_map)
        );
    }

    #[test]
    fn variable_bpm_matrix_matches_generic_path() {
        let densities = [0, 16, 20, 24, 32, 16, 20, 24, 32, 16, 0, 20, 24, 32, 48, 16];
        let bpm_map = [
            (0.0, 180.0),
            (8.0, 220.0),
            (16.0, 180.0),
            (32.0, -10.0),
            (48.0, 240.0),
        ];
        assert_eq!(
            compute_matrix_rating(&densities, &bpm_map),
            matrix_rating_generic(&densities, &bpm_map)
        );
    }

    #[test]
    fn few_variable_bpms_match_generic_path_at_stack_boundary() {
        let densities: Vec<_> = (0..256)
            .map(|idx| [0, 16, 20, 24, 32, 48][idx % 6])
            .collect();

        for segment_count in 2..HASH_AGGREGATION_MIN_SEGMENTS {
            let bpm_map: Vec<_> = (0..segment_count)
                .map(|idx| {
                    let bpm = if idx % 11 == 0 {
                        -10.0
                    } else {
                        90.0 + (idx % 7) as f64 * 15.0
                    };
                    (idx as f64 * 8.0, bpm)
                })
                .collect();

            assert_eq!(
                compute_matrix_rating(&densities, &bpm_map),
                matrix_rating_generic(&densities, &bpm_map),
                "small-map result changed for {segment_count} segments"
            );
        }
    }

    #[test]
    fn many_unique_bpms_match_generic_path() {
        let densities: Vec<_> = (0..512).map(|idx| [0, 16, 20, 24, 32][idx % 5]).collect();
        let bpm_map: Vec<_> = (0..256)
            .map(|idx| {
                let bpm = if idx % 17 == 0 {
                    -10.0
                } else {
                    60.0 + idx as f64 * 0.125
                };
                (idx as f64 * 8.0, bpm)
            })
            .collect();

        assert_eq!(
            compute_matrix_rating(&densities, &bpm_map),
            matrix_rating_generic(&densities, &bpm_map)
        );
    }

    #[test]
    fn matrix_profile_matches_direct_rating_at_multiple_rates() {
        let densities = [0, 16, 20, 24, 32, 16, 20, 24, 32, 16, 0, 20, 24, 32, 48, 16];
        let bpm_map = [
            (0.0, 180.0),
            (8.0, 220.0),
            (16.0, 180.0),
            (32.0, -10.0),
            (48.0, 240.0),
        ];
        let profile = compute_matrix_profile(&densities, &bpm_map);

        assert_eq!(
            matrix_rating_at_rate(&profile, 1.0),
            compute_matrix_rating(&densities, &bpm_map)
        );
        assert_eq!(
            profile.rating_at_rate(1.0),
            compute_matrix_rating(&densities, &bpm_map)
        );
        for rate in [0.8, 1.25, 1.5] {
            let scaled_bpms: Vec<_> = bpm_map
                .iter()
                .map(|&(beat, bpm)| (beat, bpm * rate))
                .collect();
            assert_eq!(
                matrix_rating_at_rate(&profile, rate),
                compute_matrix_rating(&densities, &scaled_bpms),
                "profile result changed at {rate:.2}x"
            );
            assert_eq!(
                profile.rating_at_rate(rate),
                compute_matrix_rating(&densities, &scaled_bpms),
                "profile method result changed at {rate:.2}x"
            );
        }
    }

    #[test]
    fn generated_profiles_match_fixed_small_and_hashed_rating_paths() {
        let densities: Vec<_> = (0..512)
            .map(|index| [0, 16, 20, 24, 32, 48][index % 6])
            .collect();
        for segment_count in [1usize, 2, 31, 32, 96] {
            let segment_beats = densities.len() as f64 * 4.0 / segment_count as f64;
            let bpm_map: Vec<_> = (0..segment_count)
                .map(|index| {
                    (
                        index as f64 * segment_beats,
                        80.0 + (index % 17) as f64 * 10.0,
                    )
                })
                .collect();
            let profile = compute_matrix_profile(&densities, &bpm_map);
            for rate in [0.5, 0.75, 1.0, 1.25, 1.5, 2.0] {
                let scaled_bpms: Vec<_> = bpm_map
                    .iter()
                    .map(|&(beat, bpm)| (beat, bpm * rate))
                    .collect();
                assert_eq!(
                    matrix_rating_at_rate(&profile, rate).to_bits(),
                    compute_matrix_rating(&densities, &scaled_bpms).to_bits(),
                    "profile result changed for {segment_count} segments at {rate:.2}x"
                );
            }
        }
    }

    #[test]
    fn invalid_matrix_profile_rate_falls_back_to_one() {
        let profile = compute_matrix_profile(&[16, 16, 16, 16], &[(0.0, 180.0)]);
        let base = matrix_rating_at_rate(&profile, 1.0);

        assert_eq!(matrix_rating_at_rate(&profile, 0.0), base);
        assert_eq!(matrix_rating_at_rate(&profile, f64::NAN), base);
        assert_eq!(profile.rating_at_rate(0.0), base);
        assert_eq!(profile.rating_at_rate(f64::NAN), base);
    }
}

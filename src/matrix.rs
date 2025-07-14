use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use crate::bpm::get_current_bpm;
use crate::stats::{categorize_measure_density, RunDensity};

/// Sorted difficulty table for efficient bound queries.
type DifficultyTable = BTreeMap<i32, BTreeMap<i32, i32>>;

/// Lazily initialized difficulty table.
static DIFFICULTY_TABLE: LazyLock<DifficultyTable> = LazyLock::new(|| {
    // Static data array for compile-time inclusion.
    const DATA: [(i32, [(i32, i32); 13]); 43] = [
        (80, [(8, 7), (12, 7), (16, 8), (24, 8), (32, 9), (48, 9), (64, 9), (96, 10), (128, 10), (192, 10), (256, 10), (384, 11), (512, 11)]),
        (90, [(8, 7), (12, 8), (16, 8), (24, 9), (32, 9), (48, 9), (64, 10), (96, 10), (128, 11), (192, 11), (256, 11), (384, 12), (512, 12)]),
        (100, [(8, 8), (12, 8), (16, 9), (24, 9), (32, 10), (48, 10), (64, 10), (96, 11), (128, 11), (192, 11), (256, 11), (384, 12), (512, 12)]),
        (110, [(8, 8), (12, 9), (16, 9), (24, 10), (32, 10), (48, 10), (64, 11), (96, 11), (128, 12), (192, 12), (256, 12), (384, 13), (512, 13)]),
        (120, [(8, 9), (12, 9), (16, 10), (24, 10), (32, 11), (48, 11), (64, 12), (96, 12), (128, 12), (192, 13), (256, 13), (384, 13), (512, 13)]),
        (130, [(8, 9), (12, 10), (16, 10), (24, 11), (32, 11), (48, 12), (64, 12), (96, 13), (128, 13), (192, 13), (256, 14), (384, 14), (512, 14)]),
        (140, [(8, 10), (12, 10), (16, 11), (24, 11), (32, 12), (48, 12), (64, 13), (96, 13), (128, 13), (192, 14), (256, 14), (384, 14), (512, 15)]),
        (150, [(8, 10), (12, 11), (16, 11), (24, 12), (32, 12), (48, 13), (64, 13), (96, 14), (128, 14), (192, 15), (256, 15), (384, 15), (512, 16)]),
        (160, [(8, 11), (12, 11), (16, 12), (24, 12), (32, 12), (48, 13), (64, 14), (96, 14), (128, 15), (192, 15), (256, 16), (384, 16), (512, 16)]),
        (170, [(8, 11), (12, 12), (16, 12), (24, 13), (32, 13), (48, 14), (64, 14), (96, 15), (128, 15), (192, 16), (256, 16), (384, 17), (512, 17)]),
        (180, [(8, 12), (12, 12), (16, 13), (24, 13), (32, 13), (48, 14), (64, 15), (96, 15), (128, 16), (192, 16), (256, 17), (384, 17), (512, 18)]),
        (190, [(8, 12), (12, 13), (16, 13), (24, 14), (32, 14), (48, 15), (64, 15), (96, 16), (128, 17), (192, 17), (256, 18), (384, 18), (512, 19)]),
        (200, [(8, 13), (12, 13), (16, 14), (24, 14), (32, 15), (48, 15), (64, 16), (96, 17), (128, 17), (192, 18), (256, 19), (384, 19), (512, 20)]),
        (210, [(8, 13), (12, 14), (16, 14), (24, 15), (32, 15), (48, 16), (64, 17), (96, 18), (128, 18), (192, 19), (256, 20), (384, 20), (512, 21)]),
        (220, [(8, 14), (12, 14), (16, 15), (24, 16), (32, 16), (48, 17), (64, 18), (96, 19), (128, 19), (192, 20), (256, 21), (384, 22), (512, 22)]),
        (230, [(8, 14), (12, 15), (16, 16), (24, 16), (32, 17), (48, 18), (64, 19), (96, 20), (128, 20), (192, 21), (256, 22), (384, 22), (512, 23)]),
        (240, [(8, 15), (12, 16), (16, 16), (24, 17), (32, 18), (48, 19), (64, 20), (96, 21), (128, 22), (192, 23), (256, 23), (384, 24), (512, 24)]),
        (250, [(8, 16), (12, 17), (16, 18), (24, 18), (32, 19), (48, 20), (64, 21), (96, 22), (128, 23), (192, 24), (256, 24), (384, 25), (512, 25)]),
        (260, [(8, 17), (12, 18), (16, 19), (24, 19), (32, 21), (48, 22), (64, 23), (96, 23), (128, 24), (192, 25), (256, 25), (384, 26), (512, 26)]),
        (270, [(8, 18), (12, 19), (16, 20), (24, 21), (32, 22), (48, 23), (64, 24), (96, 25), (128, 25), (192, 26), (256, 26), (384, 27), (512, 27)]),
        (280, [(8, 19), (12, 20), (16, 21), (24, 22), (32, 23), (48, 24), (64, 25), (96, 26), (128, 26), (192, 27), (256, 27), (384, 28), (512, 28)]),
        (290, [(8, 20), (12, 21), (16, 22), (24, 23), (32, 24), (48, 25), (64, 26), (96, 27), (128, 27), (192, 28), (256, 28), (384, 29), (512, 29)]),
        (300, [(8, 21), (12, 22), (16, 23), (24, 24), (32, 24), (48, 25), (64, 26), (96, 27), (128, 28), (192, 29), (256, 30), (384, 30), (512, 30)]),
        (310, [(8, 22), (12, 23), (16, 24), (24, 24), (32, 25), (48, 26), (64, 27), (96, 28), (128, 29), (192, 29), (256, 30), (384, 31), (512, 31)]),
        (320, [(8, 22), (12, 23), (16, 24), (24, 25), (32, 26), (48, 27), (64, 28), (96, 29), (128, 30), (192, 30), (256, 31), (384, 32), (512, 32)]),
        (330, [(8, 23), (12, 24), (16, 25), (24, 26), (32, 26), (48, 28), (64, 29), (96, 30), (128, 31), (192, 31), (256, 32), (384, 32), (512, 33)]),
        (340, [(8, 24), (12, 25), (16, 26), (24, 27), (32, 27), (48, 29), (64, 30), (96, 31), (128, 31), (192, 32), (256, 32), (384, 33), (512, 34)]),
        (350, [(8, 25), (12, 26), (16, 27), (24, 28), (32, 28), (48, 30), (64, 30), (96, 31), (128, 32), (192, 33), (256, 33), (384, 34), (512, 35)]),
        (360, [(8, 26), (12, 27), (16, 27), (24, 28), (32, 29), (48, 30), (64, 31), (96, 32), (128, 33), (192, 34), (256, 34), (384, 35), (512, 36)]),
        (370, [(8, 27), (12, 28), (16, 28), (24, 29), (32, 30), (48, 32), (64, 32), (96, 33), (128, 34), (192, 34), (256, 35), (384, 36), (512, 37)]),
        (380, [(8, 28), (12, 29), (16, 29), (24, 30), (32, 31), (48, 33), (64, 34), (96, 34), (128, 35), (192, 36), (256, 36), (384, 37), (512, 38)]),
        (390, [(8, 29), (12, 30), (16, 31), (24, 32), (32, 33), (48, 34), (64, 35), (96, 35), (128, 36), (192, 37), (256, 37), (384, 38), (512, 39)]),
        (400, [(8, 30), (12, 31), (16, 32), (24, 33), (32, 34), (48, 35), (64, 36), (96, 37), (128, 37), (192, 38), (256, 39), (384, 39), (512, 40)]),
        (410, [(8, 31), (12, 32), (16, 33), (24, 34), (32, 35), (48, 36), (64, 37), (96, 38), (128, 38), (192, 39), (256, 40), (384, 40), (512, 41)]),
        (420, [(8, 32), (12, 33), (16, 34), (24, 35), (32, 36), (48, 37), (64, 38), (96, 39), (128, 39), (192, 40), (256, 41), (384, 42), (512, 42)]),
        (430, [(8, 33), (12, 34), (16, 35), (24, 36), (32, 37), (48, 38), (64, 39), (96, 39), (128, 40), (192, 41), (256, 42), (384, 43), (512, 43)]),
        (440, [(8, 34), (12, 35), (16, 36), (24, 37), (32, 38), (48, 39), (64, 40), (96, 40), (128, 41), (192, 42), (256, 43), (384, 44), (512, 44)]),
        (450, [(8, 35), (12, 36), (16, 37), (24, 38), (32, 39), (48, 40), (64, 40), (96, 41), (128, 42), (192, 43), (256, 44), (384, 45), (512, 45)]),
        (460, [(8, 36), (12, 37), (16, 38), (24, 39), (32, 40), (48, 41), (64, 41), (96, 42), (128, 43), (192, 44), (256, 45), (384, 46), (512, 46)]),
        (470, [(8, 37), (12, 38), (16, 39), (24, 40), (32, 41), (48, 42), (64, 42), (96, 43), (128, 44), (192, 45), (256, 46), (384, 47), (512, 47)]),
        (480, [(8, 38), (12, 39), (16, 40), (24, 41), (32, 42), (48, 43), (64, 43), (96, 44), (128, 45), (192, 46), (256, 47), (384, 48), (512, 48)]),
        (490, [(8, 39), (12, 40), (16, 41), (24, 42), (32, 43), (48, 44), (64, 44), (96, 45), (128, 46), (192, 47), (256, 48), (384, 49), (512, 49)]),
        (500, [(8, 40), (12, 41), (16, 42), (24, 43), (32, 44), (48, 45), (64, 45), (96, 46), (128, 47), (192, 48), (256, 49), (384, 50), (512, 50)]),
    ];

    DATA.iter()
        .map(|&(bpm, measures_arr)| {
            (
                bpm,
                measures_arr.iter().cloned().collect::<BTreeMap<_, _>>(),
            )
        })
        .collect()
});

/// Finds the lower bound measure and its difficulty.
#[inline(always)]
fn find_lower_bound(measures: f64, bpm_data: &BTreeMap<i32, i32>) -> (f64, f64) {
    if let Some((&m, &d)) = bpm_data.iter().rev().find(|&(&m, _)| (m as f64) <= measures) {
        (m as f64, d as f64)
    } else {
        (0.0, 0.0)
    }
}

/// Finds the start of a difficulty range.
#[inline(always)]
fn find_range_start(base_difficulty: f64, bpm_data: &BTreeMap<i32, i32>) -> f64 {
    bpm_data
        .iter()
        .find(|&(_, &d)| (d as f64) == base_difficulty)
        .map(|(&m, _)| m as f64)
        .unwrap_or(0.0)
}

/// Finds the start of the next difficulty range.
#[inline(always)]
fn find_range_end(range_start_m: f64, base_difficulty: f64, bpm_data: &BTreeMap<i32, i32>) -> f64 {
    bpm_data
        .iter()
        .find(|&(&m, &d)| (m as f64) > range_start_m && (d as f64) > base_difficulty)
        .map(|(&m, _)| m as f64)
        .unwrap_or(f64::INFINITY)
}

/// Computes downward extrapolation for low measures.
#[inline(always)]
fn extrapolate_downward(measures: f64, min_measure_key: f64, min_difficulty: f64) -> f64 {
    let adjustment = (min_measure_key / measures).ln();
    (min_difficulty - adjustment).max(0.0)
}

/// Computes logarithmic interpolation within a range.
#[inline(always)]
fn interpolate_log(measures: f64, range_start_m: f64, range_end_m: f64, base_difficulty: f64) -> f64 {
    if measures <= range_start_m {
        return base_difficulty;
    }
    let log_progress = (measures.ln() - range_start_m.ln()) / (range_end_m.ln() - range_start_m.ln());
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
fn calculate_difficulty_for_bpm(measures: f64, bpm_data: &BTreeMap<i32, i32>) -> f64 {
    if measures <= 0.0 {
        return 0.0;
    }

    let min_measure_key = *bpm_data.keys().next().unwrap_or(&0) as f64;

    if measures < min_measure_key {
        let min_difficulty = *bpm_data.get(&(min_measure_key as i32)).unwrap_or(&0) as f64;
        return extrapolate_downward(measures, min_measure_key, min_difficulty);
    }

    let (_, base_difficulty) = find_lower_bound(measures, bpm_data);

    let max_diff_in_row = *bpm_data.values().max().unwrap_or(&0) as f64;

    if (base_difficulty - max_diff_in_row).abs() < f64::EPSILON {
        let plateau_start_m = find_range_start(max_diff_in_row, bpm_data);
        scale_plateau(measures, plateau_start_m, base_difficulty)
    } else {
        let range_start_m = find_range_start(base_difficulty, bpm_data);
        let range_end_m = find_range_end(range_start_m, base_difficulty, bpm_data);
        interpolate_log(measures, range_start_m, range_end_m, base_difficulty)
    }
}

/// Finds bounding BPMs for interpolation without collecting all keys.
#[inline(always)]
fn find_bounding_bpms(bpm: f64, table: &DifficultyTable) -> (i32, i32) {
    if let Some((&max_bpm, _)) = table.iter().next_back() {
        if bpm > max_bpm as f64 {
            if let Some((&prev, _)) = table.range(..max_bpm).next_back() {
                return (prev, max_bpm);
            }
        }
    }

    if let Some((&min_bpm, _)) = table.iter().next() {
        if bpm < min_bpm as f64 {
            if let Some((&next, _)) = table.range(min_bpm + 1..).next() {
                return (min_bpm, next);
            }
        }
    }

    let bpm_i = bpm as i32;
    let bpm1 = table
        .range(..=bpm_i)
        .next_back()
        .map(|(&b, _)| b)
        .unwrap_or(0);
    let bpm2 = table
        .range(bpm_i..)
        .next()
        .map(|(&b, _)| b)
        .unwrap_or(bpm1);
    (bpm1, bpm2)
}

/// Interpolates difficulty between two BPM rows.
pub fn get_difficulty(bpm: f64, measures: f64) -> f64 {
    let (bpm1, bpm2) = find_bounding_bpms(bpm, &DIFFICULTY_TABLE);

    let diff_at_bpm1 = calculate_difficulty_for_bpm(measures, DIFFICULTY_TABLE.get(&bpm1).unwrap_or(&BTreeMap::new()));

    if bpm1 == bpm2 {
        return diff_at_bpm1;
    }

    let diff_at_bpm2 = calculate_difficulty_for_bpm(measures, DIFFICULTY_TABLE.get(&bpm2).unwrap_or(&BTreeMap::new()));

    let bpm_range = (bpm2 - bpm1) as f64;
    if bpm_range == 0.0 {
        return diff_at_bpm1;
    }

    let bpm_progress = (bpm - bpm1 as f64) / bpm_range;
    diff_at_bpm1 + (diff_at_bpm2 - diff_at_bpm1) * bpm_progress
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

/// Finds the maximum difficulty rating from stream sections.
pub fn compute_matrix_rating(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> f64 {
    if measure_densities.is_empty() || bpm_map.is_empty() {
        return 0.0;
    }

    let mut stream_counts: HashMap<(RunDensity, u64), usize> = HashMap::new();

    for (i, &density) in measure_densities.iter().enumerate() {
        let category = categorize_measure_density(density);
        if category == RunDensity::Break {
            continue;
        }

        let beat = i as f64 * 4.0;
        let bpm = get_current_bpm(beat, bpm_map);
        if bpm <= 0.0 {
            continue;
        }

        *stream_counts.entry((category, bpm.to_bits())).or_insert(0) += 1;
    }

    stream_counts
        .into_iter()
        .filter_map(|((category, bpm_bits), count)| {
            let bpm = f64::from_bits(bpm_bits);
            let multiplier = get_density_multiplier(category);
            let effective_bpm = bpm * multiplier;
            if effective_bpm > 0.0 {
                Some(get_difficulty(effective_bpm, count as f64))
            } else {
                None
            }
        })
        .fold(0.0, f64::max)
}

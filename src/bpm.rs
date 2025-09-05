fn normalize_entry(beat_bpm: &str) -> String {
    if let Some((beat_str, bpm_str)) = beat_bpm.split_once('=') {
        let beat_str = beat_str.trim_matches(|c: char| c.is_control());
        let bpm_str = bpm_str.trim_matches(|c: char| c.is_control());
        if let (Ok(beat_val), Ok(bpm_val)) = (beat_str.parse::<f64>(), bpm_str.parse::<f64>()) {
            let beat_rounded = (beat_val * 1000.0).round() / 1000.0;
            let bpm_rounded = (bpm_val * 1000.0).round() / 1000.0;
            return format!("{:.3}={:.3}", beat_rounded, bpm_rounded);
        }
    }
    beat_bpm.to_string()
}

pub fn normalize_float_digits(param: &str) -> String {
    param
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_entry)
        .collect::<Vec<_>>()
        .join(",")
}

pub fn parse_bpm_map(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpms_vec: Vec<(f64, f64)> = normalized_bpms
        .split(',')
        .filter_map(|chunk| {
            chunk.trim().split_once('=').and_then(|(left, right)| {
                let beat = left.trim().parse::<f64>().ok()?;
                let bpm = right.trim().parse::<f64>().ok()?;
                Some((beat, bpm))
            })
        })
        .collect();

    bpms_vec.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    bpms_vec
}

/// Returns the BPM in effect at a given beat
pub fn get_current_bpm(beat: f64, bpm_map: &[(f64, f64)]) -> f64 {
    if bpm_map.is_empty() {
        return 0.0;
    }

    // `partition_point` returns the index of the first element for which the predicate is false.
    // It's equivalent to a binary search for the insertion point to maintain order.
    let pos = bpm_map.partition_point(|&(b, _)| b <= beat);

    if pos == 0 {
        // If the beat is before the very first BPM change, the effective BPM is that first change.
        bpm_map[0].1
    } else {
        // `pos` is the index of the first BPM change *after* the given beat.
        // The correct BPM is the one at the previous index.
        bpm_map[pos - 1].1
    }
}

pub fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (i32, i32) {
    if bpm_map.is_empty() {
        return (0, 0);
    }
    let (min_bpm, max_bpm) = bpm_map.iter().map(|&(_, bpm)| bpm).fold(
        (f64::MAX, f64::MIN),
        |(min, max), bpm| (min.min(bpm), max.max(bpm)),
    );
    (min_bpm.round() as i32, max_bpm.round() as i32)
}

pub fn compute_total_chart_length(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> i32 {
    let total_length_seconds: f64 = measure_densities
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let measure_start_beat = i as f64 * 4.0;
            let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
            if curr_bpm > 0.0 {
                (4.0 / curr_bpm) * 60.0
            } else {
                0.0
            }
        })
        .sum();
    total_length_seconds.floor() as i32
}

pub fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    measure_densities
        .iter()
        .enumerate()
        .map(|(i, &density)| {
            let measure_start_beat = i as f64 * 4.0;
            let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
            if curr_bpm > 0.0 {
                // NPS = density / (4 * 60 / BPM) = density * BPM / 240
                density as f64 * curr_bpm / 240.0
            } else {
                0.0
            }
        })
        .collect()
}

/// Computes median of a pre-sorted slice of f64.
fn median_of_sorted(sorted: &[f64]) -> f64 {
    let len = sorted.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    } else {
        sorted[len / 2]
    }
}

/// A small helper to compute median of a slice of f64.
fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    let mut sorted = arr.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    median_of_sorted(&sorted)
}

pub fn get_nps_stats(measure_nps_vec: &[f64]) -> (f64, f64) {
    let max_nps = measure_nps_vec
        .iter()
        .fold(f64::MIN, |a, &b| a.max(b))
        .max(0.0);
    let median_nps = median(measure_nps_vec);
    (max_nps, median_nps)
}

pub fn compute_bpm_stats(bpm_values: &[f64]) -> (f64, f64) {
    if bpm_values.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted = bpm_values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = median_of_sorted(&sorted);
    let average = sorted.iter().sum::<f64>() / sorted.len() as f64;
    (median, average)
}

pub fn compute_tier_bpm(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    beats_per_measure: f64,
) -> f64 {
    use crate::stats::categorize_measure_density;
    use crate::stats::RunDensity;

    let max_bpm = bpm_map
        .iter()
        .map(|&(_, bpm)| bpm)
        .fold(f64::NEG_INFINITY, f64::max);

    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();
    let mut max_e: f64 = 0.0;

    let mut i = 0;
    while i < cats.len() {
        let cat = cats[i];
        if cat == RunDensity::Break {
            i += 1;
            continue;
        }

        let mut j = i;
        while j < cats.len() && cats[j] == cat {
            j += 1;
        }
        let seq_len = j - i;

        if seq_len >= 4 {
            for k in i..j {
                let beat = k as f64 * beats_per_measure;
                let bpm_k = get_current_bpm(beat, bpm_map);
                let d_k = measure_densities[k] as f64;
                let e_k = (d_k * bpm_k) / 16.0;
                max_e = max_e.max(e_k);
            }
        }
        i = j;
    }

    if max_e > 0.0 {
        max_e
    } else {
        max_bpm
    }
}

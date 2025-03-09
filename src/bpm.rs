use std::fmt::Write as FmtWrite;

pub fn normalize_float_digits(param: &str) -> String {
    let mut output = String::with_capacity(param.len());
    let mut first = true;
    for beat_bpm in param.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !first {
            output.push(',');
        } else {
            first = false;
        }

        let mut eq_split = beat_bpm.split('=');
        let beat_str = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());
        let bpm_str  = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());

        if let (Ok(beat_val), Ok(bpm_val)) = (beat_str.parse::<f64>(), bpm_str.parse::<f64>()) {
            let beat_rounded = (beat_val * 1000.0).round() / 1000.0;
            let bpm_rounded  = (bpm_val * 1000.0).round() / 1000.0;
            let _ = write!(&mut output, "{:.3}={:.3}", beat_rounded, bpm_rounded);
        } else {
            output.push_str(beat_bpm);
        }
    }
    output
}

pub fn parse_bpm_map(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpms_vec = Vec::new();
    for chunk in normalized_bpms.split(',') {
        let chunk = chunk.trim();
        if let Some(eq_pos) = chunk.find('=') {
            let left = &chunk[..eq_pos].trim();
            let right = &chunk[eq_pos + 1..].trim();
            if let (Ok(beat), Ok(bpm)) = (left.parse::<f64>(), right.parse::<f64>()) {
                bpms_vec.push((beat, bpm));
            }
        }
    }
    bpms_vec.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    bpms_vec
}

/// Returns the BPM in effect at a given beat
pub fn get_current_bpm(beat: f64, bpm_map: &[(f64, f64)]) -> f64 {
    let mut curr_bpm = if !bpm_map.is_empty() { bpm_map[0].1 } else { 0.0 };
    for &(b_beat, b_bpm) in bpm_map {
        if beat >= b_beat {
            curr_bpm = b_bpm;
        } else {
            break;
        }
    }
    curr_bpm
}

pub fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (i32, i32) {
    if bpm_map.is_empty() {
        return (0, 0);
    }
    let mut min_bpm = f64::MAX;
    let mut max_bpm = f64::MIN;
    for &(_, bpm) in bpm_map {
        if bpm < min_bpm {
            min_bpm = bpm;
        }
        if bpm > max_bpm {
            max_bpm = bpm;
        }
    }
    (
        min_bpm.round() as i32,
        max_bpm.round() as i32,
    )
}

pub fn compute_total_chart_length(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> i32 {
    let mut total_length_seconds = 0.0;
    for (i, _) in measure_densities.iter().enumerate() {
        let measure_start_beat = i as f64 * 4.0;
        let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
        if curr_bpm <= 0.0 {
            continue;
        }
        let measure_length_s = (4.0 / curr_bpm) * 60.0;
        total_length_seconds += measure_length_s;
    }
    total_length_seconds.floor() as i32
}

pub fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    let mut measure_nps_vec = Vec::with_capacity(measure_densities.len());
    for (i, &density) in measure_densities.iter().enumerate() {
        let measure_start_beat = i as f64 * 4.0;
        let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
        if curr_bpm <= 0.0 {
            measure_nps_vec.push(0.0);
            continue;
        }
        // NPS = #notes / (measure_length_in_seconds)
        // measure_length_in_seconds = 4 beats / (BPM/60) = 4 * 60 / BPM
        // so notes per second = density / (4 * 60 / BPM) = density * BPM / (4 * 60).
        let measure_nps = density as f64 * (curr_bpm / 4.0) / 60.0;
        measure_nps_vec.push(measure_nps);
    }
    measure_nps_vec
}

/// A small helper to compute median of a slice of f64.
fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    let mut sorted = arr.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    } else {
        sorted[len / 2]
    }
}

pub fn get_nps_stats(measure_nps_vec: &[f64]) -> (f64, f64) {
    let max_nps = if measure_nps_vec.is_empty() {
        0.0
    } else {
        measure_nps_vec.iter().fold(f64::MIN, |a, &b| a.max(b))
    };
    let median_nps = median(measure_nps_vec);
    (max_nps, median_nps)
}

pub fn compute_bpm_stats(bpm_values: &[f64]) -> (f64, f64) {
    if bpm_values.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted = bpm_values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    let median = if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    };
    let average = sorted.iter().sum::<f64>() / sorted.len() as f64;
    (median, average)
}

pub fn compute_tier_bpm(measure_densities: &[usize], bpm_map: &[(f64, f64)], beats_per_measure: f64) -> f64 {
    use crate::stats::categorize_measure_density;
    use crate::stats::RunDensity;

    // Calculate the maximum BPM from bpm_map
    let max_bpm = bpm_map.iter().map(|&(_, bpm)| bpm).fold(f64::NEG_INFINITY, f64::max);

    let cats: Vec<RunDensity> = measure_densities.iter().map(|&d| categorize_measure_density(d)).collect();
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
                let bpm_k = get_bpm_at_beat(bpm_map, beat);
                let d_k = measure_densities[k] as f64;
                let e_k = (d_k * bpm_k) / 16.0;
                max_e = max_e.max(e_k);
            }
        }
        i = j;
    }

    // Return max_bpm if no qualifying sequences are found
    if max_e > 0.0 {
        max_e
    } else {
        max_bpm
    }
}

pub fn get_bpm_at_beat(bpm_map: &[(f64, f64)], beat: f64) -> f64 {
    let mut last_bpm = 0.0; // Default to 0 if no BPM is set before the beat
    for &(b, bpm) in bpm_map {
        if b > beat {
            break;
        }
        last_bpm = bpm;
    }
    last_bpm
}
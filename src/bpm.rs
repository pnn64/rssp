fn normalize_decimal(s: &str) -> Option<String> {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    let value: f64 = cleaned.trim().parse().ok()?;

    let mult = 1000.0;
    let temp = value * mult + 0.5;
    let rounded = (temp - temp.rem_euclid(1.0)) / mult;

    Some(format!("{:.3}", rounded))
}

fn normalize_entry(beat_bpm: &str) -> String {
    let trimmed = beat_bpm.trim();
    if let Some((beat_str, bpm_str)) = trimmed.split_once('=') {
        if let (Some(beat), Some(bpm)) = (normalize_decimal(beat_str), normalize_decimal(bpm_str)) {
            return format!("{}={}", beat, bpm);
        }
    }
    trimmed.to_string()
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

fn normalized_3dp_to_thousandths(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (sign, body) = s.strip_prefix('-').map_or((1i64, s), |rest| (-1i64, rest));
    let (int_part, frac_part) = body.split_once('.').unwrap_or((body, "0"));

    if !int_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    let int_value: i64 = int_part.parse().ok()?;
    let mut frac = frac_part.to_string();
    if frac.len() > 3 {
        frac.truncate(3);
    } else {
        while frac.len() < 3 {
            frac.push('0');
        }
    }
    let frac_value: i64 = frac.parse().ok()?;

    Some(sign * (int_value * 1000 + frac_value))
}

#[derive(Clone)]
struct NormalizedTimingEntry {
    beat_thousandths: i64,
    beat_str: String,
    value_thousandths: i64,
    value_str: String,
    index: usize,
}

fn parse_and_normalize_timing_entry(entry: &str, index: usize) -> Option<NormalizedTimingEntry> {
    let trimmed = entry.trim();
    let (beat_raw, value_raw) = trimmed.split_once('=')?;
    let beat_str = normalize_decimal(beat_raw)?;
    let value_str = normalize_decimal(value_raw)?;
    Some(NormalizedTimingEntry {
        beat_thousandths: normalized_3dp_to_thousandths(&beat_str)?,
        beat_str,
        value_thousandths: normalized_3dp_to_thousandths(&value_str)?,
        value_str,
        index,
    })
}

pub fn normalize_and_tidy_bpms(param: &str) -> String {
    let mut entries: Vec<NormalizedTimingEntry> = param
        .split(',')
        .enumerate()
        .filter_map(|(i, entry)| parse_and_normalize_timing_entry(entry, i))
        .collect();

    if entries.is_empty() {
        return "0.000=60.000".to_string();
    }

    entries.sort_by(|a, b| a
        .beat_thousandths
        .cmp(&b.beat_thousandths)
        .then_with(|| a.index.cmp(&b.index)));

    let mut last_per_beat: Vec<NormalizedTimingEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(last) = last_per_beat.last_mut() {
            if last.beat_thousandths == entry.beat_thousandths {
                *last = entry;
                continue;
            }
        }
        last_per_beat.push(entry);
    }

    if let Some(first) = last_per_beat.first_mut() {
        if first.beat_thousandths != 0 {
            first.beat_thousandths = 0;
            first.beat_str = "0.000".to_string();
        }
    }

    let mut tidied: Vec<NormalizedTimingEntry> = Vec::with_capacity(last_per_beat.len());
    let mut last_value: Option<i64> = None;
    for entry in last_per_beat {
        if last_value == Some(entry.value_thousandths) {
            continue;
        }
        last_value = Some(entry.value_thousandths);
        tidied.push(entry);
    }

    tidied
        .into_iter()
        .map(|e| format!("{}={}", e.beat_str, e.value_str))
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

/// Alias for parsing generic beat=value timing maps (Stops, Delays, Warps).
pub fn parse_timing_map(normalized: &str) -> Vec<(f64, f64)> {
    parse_bpm_map(normalized)
}

/// Returns the BPM in effect at a given beat.
/// This is used for actual timing calculations and is NOT filtered.
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

/// Threshold for determining if a BPM is a "gimmick" (warp/visual effect) vs playable.
/// Matches Simply Love's logic roughly (SL uses 0.12s/measure which is ~2000 BPM).
/// We use 10,000 here to be conservative but catch the millions.
const GIMMICK_BPM_THRESHOLD: f64 = 10000.0;

/// Determines if a BPM is considered "playable" for stats/display purposes.
/// Filters out stops (<= 0) and visual gimmick warps (>= 10000).
fn is_display_bpm(bpm: f64) -> bool {
    bpm > 0.0 && bpm < GIMMICK_BPM_THRESHOLD
}

/// Computes the min/max BPM range for display purposes.
///
/// Applies a heuristic to ignore "gimmick" BPMs (e.g., <= 0 or >= 10,000) which are
/// often used for visual effects or stops, unless no valid BPMs remain.
pub fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (i32, i32) {
    if bpm_map.is_empty() {
        return (0, 0);
    }

    let (mut min_bpm, mut max_bpm, count) = bpm_map.iter()
        .map(|&(_, bpm)| bpm)
        .filter(|&bpm| is_display_bpm(bpm))
        .fold((f64::MAX, f64::MIN, 0), |(min, max, count), bpm| {
            (min.min(bpm), max.max(bpm), count + 1)
        });

    if count == 0 {
        // Fallback: if all BPMs were filtered out (e.g., gimmicks only), include everything.
        let (fmin, fmax) = bpm_map.iter().map(|&(_, bpm)| bpm).fold(
            (f64::MAX, f64::MIN),
            |(min, max), bpm| (min.min(bpm), max.max(bpm)),
        );
        min_bpm = fmin;
        max_bpm = fmax;
    }

    (min_bpm.round() as i32, max_bpm.round() as i32)
}

/// Calculates the accurate cumulative time to reach a target beat, accounting for
/// BPM changes, Stops, Delays, and Warps.
///
/// Logic mimics StepMania/ITGmania's `GetElapsedTimeFromBeat`:
/// - Beats advance time based on current BPM.
/// - Warps skip beats instantly (time doesn't advance).
/// - Stops/Delays add time instantly (beats don't advance).
pub fn get_elapsed_time(
    target_beat: f64,
    bpm_map: &[(f64, f64)],
    stop_map: &[(f64, f64)],
    delay_map: &[(f64, f64)],
    warp_map: &[(f64, f64)],
) -> f64 {
    // Event priority: 0=BPM, 1=Stop/Delay, 2=Warp
    let mut events = Vec::with_capacity(bpm_map.len() + stop_map.len() + delay_map.len() + warp_map.len());
    for &(b, v) in bpm_map { events.push((b, 0, v)); }
    for &(b, v) in stop_map { events.push((b, 1, v)); }
    for &(b, v) in delay_map { events.push((b, 1, v)); }
    for &(b, v) in warp_map { events.push((b, 2, v)); }

    // Sort by beat, then priority
    events.sort_by(|a, b| {
        a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
           .then_with(|| a.1.cmp(&b.1))
    });

    let mut current_time = 0.0;
    let mut current_beat = 0.0;
    let mut current_bpm = if !bpm_map.is_empty() && bpm_map[0].0 <= 0.0 { bpm_map[0].1 } else { 120.0 };
    let mut warp_end_beat = 0.0;

    for (event_beat, priority, value) in events {
        // Optimization: if we are past target and not currently warping, we can stop.
        if event_beat > target_beat && warp_end_beat <= target_beat {
            break;
        }

        // Advance time to the event beat
        if event_beat > current_beat {
            // We only accumulate time for beats that are NOT inside a warp.
            let effective_start = current_beat.max(warp_end_beat);
            if event_beat > effective_start {
                let valid_dist = event_beat - effective_start;
                if current_bpm > 0.0 {
                    current_time += valid_dist * (60.0 / current_bpm);
                }
            }
            current_beat = event_beat;
        }

        match priority {
            0 => current_bpm = value,
            1 => current_time += value, // Stop/Delay adds time
            2 => {
                // Warp skips beats instantly.
                let end = event_beat + value;
                if end > warp_end_beat { warp_end_beat = end; }
            }
            _ => {}
        }
    }

    // Final advance to target beat
    let effective_start = current_beat.max(warp_end_beat);
    if target_beat > effective_start {
        let valid_dist = target_beat - effective_start;
        if current_bpm > 0.0 {
            current_time += valid_dist * (60.0 / current_bpm);
        }
    }

    current_time
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_tidy_bpms_dedups_redundant_and_duplicate_entries() {
        let raw = "0.000=180.000,0.000=180.000,128.000=89.800,128.000=89.800,130.000=90.600,130.000=90.600,131.500=88.500,131.500=88.500,132.000=180.000,132.000=180.000,260.000=85.300,260.000=85.300,262.000=95.000,262.000=95.000,263.500=95.000,263.500=95.000,264.000=180.000,264.000=180.000";
        let expected = "0.000=180.000,128.000=89.800,130.000=90.600,131.500=88.500,132.000=180.000,260.000=85.300,262.000=95.000,264.000=180.000";
        assert_eq!(normalize_and_tidy_bpms(raw), expected);
    }
}

/// Computes the beat of the last playable object in the chart from minimized note data.
///
/// The minimized format produced by `minimize_chart_and_count_with_lanes` is:
///   - fixed-width note rows (per-chart lane count) followed by '\n'
///   - ",\n" as a measure separator
/// Measures are assumed to be 4 beats long, matching StepMania's default behavior.
fn compute_last_beat(minimized_note_data: &[u8], lanes: usize) -> f64 {
    let mut rows_per_measure: Vec<usize> = Vec::new();
    let mut current_rows: usize = 0;

    let mut last_measure_idx: Option<usize> = None;
    let mut last_row_in_measure: usize = 0;

    let lanes = lanes.max(1);

    for line in minimized_note_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line[0] == b',' {
            rows_per_measure.push(current_rows);
            current_rows = 0;
            continue;
        }

        if line.len() >= lanes {
            let has_object = line[..lanes]
                .iter()
                .any(|&b| matches!(b, b'1' | b'2' | b'3' | b'4'));
            if has_object {
                last_measure_idx = Some(rows_per_measure.len());
                last_row_in_measure = current_rows;
            }
            current_rows += 1;
        }
    }

    // Push the final measure's row count.
    rows_per_measure.push(current_rows);

    let Some(measure_idx) = last_measure_idx else {
        return 0.0;
    };

    let total_rows_in_measure = rows_per_measure
        .get(measure_idx)
        .copied()
        .unwrap_or(0)
        .max(1) as f64;
    let row_index = last_row_in_measure as f64;

    let beats_into_measure = 4.0 * (row_index / total_rows_in_measure);
    (measure_idx as f64) * 4.0 + beats_into_measure
}

pub fn compute_total_chart_length(
    minimized_note_data: &[u8],
    lanes: usize,
    bpm_map: &[(f64, f64)],
    stop_map: &[(f64, f64)],
    delay_map: &[(f64, f64)],
    warp_map: &[(f64, f64)],
) -> i32 {
    let target_beat = compute_last_beat(minimized_note_data, lanes);
    if target_beat <= 0.0 || bpm_map.is_empty() {
        return 0;
    }

    get_elapsed_time(target_beat, bpm_map, stop_map, delay_map, warp_map).floor() as i32
}

/// Computes the number of mines that are actually judgable, i.e. not inside
/// warp ranges or #FAKES ranges. Uses the minimized chart data format
/// produced by `minimize_chart_and_count`.
pub fn compute_mines_nonfake(
    minimized_note_data: &[u8],
    lanes: usize,
    warp_map: &[(f64, f64)],
    fake_map: &[(f64, f64)],
) -> u32 {
    #[derive(Clone, Copy)]
    struct RowInfo {
        measure_idx: usize,
        row_in_measure: usize,
        is_mine: bool,
    }

    let mut rows: Vec<RowInfo> = Vec::new();
    let mut rows_per_measure: Vec<usize> = Vec::new();
    let mut current_rows: usize = 0;
    let mut measure_idx: usize = 0;
    let mut row_in_measure: usize = 0;

    let lanes = lanes.max(1);

    for line in minimized_note_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line[0] == b',' {
            rows_per_measure.push(current_rows);
            measure_idx += 1;
            current_rows = 0;
            row_in_measure = 0;
            continue;
        }
        if line.len() < lanes {
            continue;
        }
        let is_mine = line[..lanes]
            .iter()
            .any(|&b| b == b'M' || b == b'm');

        rows.push(RowInfo {
            measure_idx,
            row_in_measure,
            is_mine,
        });
        current_rows += 1;
        row_in_measure += 1;
    }
    rows_per_measure.push(current_rows);

    if rows.is_empty() {
        return 0;
    }

    let mut beats: Vec<f64> = Vec::with_capacity(rows.len());
    for info in &rows {
        let total_rows = rows_per_measure
            .get(info.measure_idx)
            .copied()
            .unwrap_or(0)
            .max(1) as f64;
        let row_index = info.row_in_measure as f64;
        let beats_into_measure = 4.0 * (row_index / total_rows);
        let beat = (info.measure_idx as f64) * 4.0 + beats_into_measure;
        beats.push(beat);
    }

    #[inline]
    fn is_active_at_beat(beat: f64, segments: &[(f64, f64)]) -> bool {
        if segments.is_empty() {
            return false;
        }
        let idx = segments.partition_point(|(seg_beat, _)| *seg_beat <= beat);
        if idx == 0 {
            return false;
        }
        let (start, len) = segments[idx - 1];
        if !len.is_finite() || len <= 0.0 {
            return false;
        }
        beat >= start && beat < start + len
    }

    let mut count: u32 = 0;
    for (info, beat) in rows.iter().zip(beats.iter()) {
        if !info.is_mine {
            continue;
        }
        let b = *beat;
        if !is_active_at_beat(b, warp_map) && !is_active_at_beat(b, fake_map) {
            count = count.saturating_add(1);
        }
    }

    count
}

pub fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    measure_densities
        .iter()
        .enumerate()
        .map(|(i, &density)| {
            let measure_start_beat = i as f64 * 4.0;
            let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
            
            // For NPS calculation, if the BPM is a gimmick (too high),
            // it implies the measure passes instantly (warp), so effective NPS
            // for a human reading it is treated as 0/unplayable, matching Simply Love.
            if !is_display_bpm(curr_bpm) {
                0.0
            } else {
                // NPS = density / (4 * 60 / BPM) = density * BPM / 240
                density as f64 * curr_bpm / 240.0
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

/// Computes median and average BPM, filtering out gimmick values unless unavoidable.
pub fn compute_bpm_stats(bpm_values: &[f64]) -> (f64, f64) {
    if bpm_values.is_empty() {
        return (0.0, 0.0);
    }

    // Filter out gimmick BPMs for stats
    let mut sorted: Vec<f64> = bpm_values.iter().copied().filter(|&b| is_display_bpm(b)).collect();

    // Fallback if everything was filtered
    if sorted.is_empty() {
        sorted = bpm_values.to_vec();
    }

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

    // Filter max BPM search
    let max_bpm = bpm_map
        .iter()
        .map(|&(_, bpm)| bpm)
        .filter(|&bpm| is_display_bpm(bpm))
        .fold(f64::NEG_INFINITY, f64::max);
    
    // If we filtered everything out (e.g. all gimmicks), just fallback to 0 or whatever is there
    let max_bpm = if max_bpm.is_finite() { max_bpm } else { 
        bpm_map.iter().map(|&(_, bpm)| bpm).fold(f64::NEG_INFINITY, f64::max)
    };

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
                
                // Only count stream density for playable BPMs.
                // If it's a gimmick warp, the stream doesn't physically exist for the player.
                if is_display_bpm(bpm_k) {
                    let d_k = measure_densities[k] as f64;
                    let e_k = (d_k * bpm_k) / 16.0;
                    max_e = max_e.max(e_k);
                }
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

use std::cmp::Ordering;

use crate::bpm::{chart_map_mode, clean_map_mode, is_display_bpm};
use crate::math::lrint_f32;
use crate::parse::{
    decode_unescape_trim, extract_sections, normalize_chart_desc_ref, parse_offset_seconds,
    parse_version,
};
use crate::timing::{
    BeatTimeCursorF32, ROWS_PER_BEAT, TimingData, compute_timing_segments, fixed_timing_parts,
    steps_timing_allowed, timing_data_from_segments, timing_format_from_ext,
};

const NPS_MEDIAN_SCAN_MIN: usize = 64;

#[derive(Debug, Clone)]
pub struct ChartNpsInfo {
    pub step_type: String,
    pub difficulty: String,
    pub peak_nps: f64,
}

/// Computes peak NPS for every supported chart in a simfile.
///
/// # Errors
///
/// Returns an error when the extension is unsupported or the simfile cannot
/// be parsed.
// This single pass keeps parsing state and scratch reuse explicit.
#[allow(clippy::too_many_lines)]
pub fn compute_chart_peak_nps(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartNpsInfo>, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let timing_format = timing_format_from_ext(extension);
    let ssc_version = parse_version(parsed_data.version, timing_format);
    let allow_steps_timing = steps_timing_allowed(ssc_version, timing_format);
    let song_offset = parse_offset_seconds(parsed_data.offset);

    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"")).unwrap_or("");
    let cleaned_global_bpms = clean_map_mode::<true>(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_stops = clean_map_mode::<true>(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_delays = clean_map_mode::<true>(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_warps = clean_map_mode::<true>(global_warps_raw);
    let global_speeds_raw = parsed_data
        .speeds
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_speeds = clean_map_mode::<true>(global_speeds_raw);
    let global_scrolls_raw = parsed_data
        .scrolls
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_scrolls = clean_map_mode::<true>(global_scrolls_raw);
    let global_fakes_raw = parsed_data
        .fakes
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_fakes = clean_map_mode::<true>(global_fakes_raw);

    let entries = parsed_data.notes_list;
    let density_capacity = entries
        .iter()
        .filter_map(|entry| {
            crate::supported_stepstype_lanes_bytes(entry.fields[0])
                .map(|lanes| crate::stats::density_capacity(entry.note_data.len(), lanes))
        })
        .max()
        .unwrap_or(0);
    let mut results = Vec::with_capacity(entries.len());
    let mut global_timing = None;
    let mut density_scratch = crate::stats::DensityScratch::with_capacity(density_capacity);

    for entry in entries {
        if entry.field_count < 5 {
            continue;
        }
        let fields = entry.fields;
        let chart_data = entry.note_data;

        let Some(lanes) = crate::supported_stepstype_lanes_bytes(fields[0]) else {
            continue;
        };
        let step_type = decode_unescape_trim(fields[0]).into_owned();
        let description_raw = decode_unescape_trim(fields[1]);
        let description =
            normalize_chart_desc_ref(description_raw.as_ref(), timing_format, ssc_version);
        let difficulty_raw = decode_unescape_trim(fields[2]);
        let meter_raw = decode_unescape_trim(fields[3]);
        let difficulty = crate::resolve_difficulty_label(
            difficulty_raw.as_ref(),
            description,
            meter_raw.as_ref(),
            extension,
        );

        let timing_src = crate::timing::resolve_chart_timing(
            allow_steps_timing,
            song_offset,
            entry.chart_offset.as_deref(),
            entry.chart_bpms.as_deref(),
            entry.chart_stops.as_deref(),
            entry.chart_delays.as_deref(),
            entry.chart_warps.as_deref(),
            entry.chart_speeds.as_deref(),
            entry.chart_scrolls.as_deref(),
            entry.chart_fakes.as_deref(),
            entry.chart_time_signatures.as_deref(),
            entry.chart_labels.as_deref(),
            entry.chart_tickcounts.as_deref(),
            entry.chart_combos.as_deref(),
            cleaned_global_bpms.as_ref(),
            cleaned_global_stops.as_ref(),
            cleaned_global_delays.as_ref(),
            cleaned_global_warps.as_ref(),
            cleaned_global_speeds.as_ref(),
            cleaned_global_scrolls.as_ref(),
            cleaned_global_fakes.as_ref(),
        );
        let chart_offset = timing_src.chart_offset_seconds;
        let chart_bpms = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_bpms.as_deref())
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_stops.as_deref())
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_delays.as_deref())
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_warps.as_deref())
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_speeds.as_deref())
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_scrolls.as_deref())
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            chart_map_mode::<true>(entry.chart_fakes.as_deref())
        } else {
            None
        };
        let chart_timing;
        let timing = if timing_src.chart_has_own_timing {
            let timing_segments = compute_timing_segments(
                chart_bpms.as_deref(),
                timing_src.global_bpms,
                chart_stops.as_deref(),
                timing_src.global_stops,
                chart_delays.as_deref(),
                timing_src.global_delays,
                chart_warps.as_deref(),
                timing_src.global_warps,
                chart_speeds.as_deref(),
                timing_src.global_speeds,
                chart_scrolls.as_deref(),
                timing_src.global_scrolls,
                chart_fakes.as_deref(),
                timing_src.global_fakes,
                timing_format,
                true,
            );
            chart_timing = timing_data_from_segments(chart_offset, 0.0, &timing_segments);
            &chart_timing
        } else {
            global_timing.get_or_insert_with(|| {
                let timing_segments = compute_timing_segments(
                    None,
                    &cleaned_global_bpms,
                    None,
                    &cleaned_global_stops,
                    None,
                    &cleaned_global_delays,
                    None,
                    &cleaned_global_warps,
                    None,
                    &cleaned_global_speeds,
                    None,
                    &cleaned_global_scrolls,
                    None,
                    &cleaned_global_fakes,
                    timing_format,
                    true,
                );
                timing_data_from_segments(song_offset, 0.0, &timing_segments)
            })
        };

        let densities =
            crate::stats::measure_densities_with_scratch(chart_data, lanes, &mut density_scratch);
        let max_nps = compute_peak_nps_with_timing(densities, timing);

        results.push(ChartNpsInfo {
            step_type,
            difficulty,
            peak_nps: max_nps,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod batch_tests {
    use super::compute_chart_peak_nps;

    const INHERITED_TIMING_FIXTURE: &[u8] =
        include_bytes!("../../rssp/benches/fixtures/camellia_mix.ssc");

    #[test]
    fn inherited_timing_cache_preserves_peak_nps() {
        let charts =
            compute_chart_peak_nps(INHERITED_TIMING_FIXTURE, "ssc").expect("fixture should parse");
        let expected = [
            ("Beginner", 10.938_223_250_845_647),
            ("Easy", 11.669_515_669_515_67),
            ("Medium", 14.583_648_582_491_433),
            ("Hard", 14.586_894_586_894_587),
            ("Challenge", 14.586_894_586_894_587),
        ];

        assert_eq!(charts.len(), expected.len());
        for (chart, (difficulty, peak_nps)) in charts.iter().zip(expected) {
            assert_eq!(chart.step_type, "dance-single");
            assert_eq!(chart.difficulty, difficulty);
            assert!((chart.peak_nps - peak_nps).abs() < 1.0e-12);
        }
    }
}

#[must_use]
pub fn compute_measure_nps_vec(densities: &[usize], bpms: &[(f64, f64)]) -> Vec<f64> {
    let mut out = Vec::with_capacity(densities.len());
    if bpms.is_empty() {
        out.resize(densities.len(), 0.0);
        return out;
    }
    crate::bpm::for_each_measure_bpm(densities.len(), bpms, 4.0, |i, bpm| {
        let density = densities[i];
        out.push(if density == 0 || !is_display_bpm(bpm) {
            0.0
        } else {
            density as f64 * bpm / 240.0
        });
    });
    out
}

#[must_use]
pub fn compute_measure_nps_vec_with_timing(densities: &[usize], timing: &TimingData) -> Vec<f64> {
    if let Some(parts) = fixed_timing_parts(timing) {
        return compute_measure_nps_vec_fixed(densities, parts);
    }

    let mut out = Vec::with_capacity(densities.len());
    let mut cursor = BeatTimeCursorF32::new(timing);
    let mut start = cursor.time_for_beat(0.0);

    for (i, &d) in densities.iter().enumerate() {
        let end = cursor.time_for_beat((i as f64 + 1.0) * 4.0);
        let dur = end - start;
        out.push(nps_for_measure(d, dur));
        start = end;
    }

    out
}

fn compute_measure_nps_vec_fixed(
    densities: &[usize],
    parts: crate::timing::FixedTimingParts,
) -> Vec<f64> {
    let mut out = Vec::with_capacity(densities.len());
    let mut start = fixed_measure_time(parts, 0);

    for (i, &d) in densities.iter().enumerate() {
        let end = fixed_measure_time(parts, i + 1);
        let dur = end - start;
        out.push(nps_for_measure(d, dur));
        start = end;
    }

    out
}

/// Computes only the peak NPS without allocating the per-measure NPS vector.
#[must_use]
pub fn compute_peak_nps_with_timing(densities: &[usize], timing: &TimingData) -> f64 {
    let mut peak = 0.0f64;

    if let Some(parts) = fixed_timing_parts(timing) {
        let mut start = fixed_measure_time(parts, 0);
        for (i, &density) in densities.iter().enumerate() {
            let end = fixed_measure_time(parts, i + 1);
            peak = peak.max(nps_for_measure(density, end - start));
            start = end;
        }
        return peak;
    }

    let mut cursor = BeatTimeCursorF32::new(timing);
    let mut start = cursor.time_for_beat(0.0);
    for (i, &density) in densities.iter().enumerate() {
        let end = cursor.time_for_beat((i as f64 + 1.0) * 4.0);
        peak = peak.max(nps_for_measure(density, end - start));
        start = end;
    }
    peak
}

#[inline(always)]
fn nps_for_measure(density: usize, duration: f64) -> f64 {
    if density == 0 || duration <= 0.12 {
        0.0
    } else {
        density as f64 / duration
    }
}

#[inline(always)]
fn fixed_measure_time(parts: crate::timing::FixedTimingParts, measure: usize) -> f64 {
    let (start, bps, global_offset) = parts;
    let beat = measure as f64 * 4.0;
    let row = lrint_f32(beat as f32 * ROWS_PER_BEAT as f32);
    f64::from(start + (row as f32 / ROWS_PER_BEAT as f32) / bps) - global_offset
}

fn median_in_place(arr: &mut [f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    let mid = arr.len() / 2;
    arr.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    if arr.len() % 2 == 1 {
        arr[mid]
    } else {
        f64::midpoint(arr[..mid].iter().fold(f64::MIN, |a, &b| a.max(b)), arr[mid])
    }
}

fn scan_nps(nps: &[f64]) -> (f64, Option<f64>) {
    if nps.is_empty() {
        return (0.0, Some(0.0));
    }
    if nps.len() < NPS_MEDIAN_SCAN_MIN {
        return (nps.iter().fold(f64::MIN, |a, &b| a.max(b)).max(0.0), None);
    }

    let (mut max, mut zeros) = (f64::MIN, 0usize);
    let (first, mut all_same, mut all_finite) = (nps[0], true, true);
    for &value in nps {
        max = max.max(value);
        zeros += usize::from(value == 0.0);
        all_same &= value == first;
        all_finite &= value.is_finite();
    }
    let median = if all_same {
        Some(first)
    } else if all_finite && zeros > nps.len() / 2 {
        Some(0.0)
    } else {
        None
    };
    (max.max(0.0), median)
}

#[must_use]
pub fn get_nps_stats(nps: &[f64]) -> (f64, f64) {
    get_nps_stats_with_scratch(nps, &mut Vec::new())
}

/// Computes NPS statistics using caller-owned median-selection storage.
///
/// One or two measures need no heap storage; larger inputs reuse `scratch`.
#[must_use]
pub fn get_nps_stats_with_scratch(nps: &[f64], scratch: &mut Vec<f64>) -> (f64, f64) {
    scratch.clear();
    let (max, median) = scan_nps(nps);
    let median = median.unwrap_or_else(|| match nps {
        [value] => *value,
        &[first, second] => median_in_place(&mut [first, second]),
        _ => {
            scratch.extend_from_slice(nps);
            median_in_place(scratch)
        }
    });
    (max, median)
}

/// Computes NPS statistics by using `nps` as median-selection storage.
///
/// The values may be reordered. Use this when the owned input is no longer
/// needed after the call to avoid allocating and copying a second buffer.
#[must_use]
pub fn get_nps_stats_in_place(nps: &mut [f64]) -> (f64, f64) {
    let (max, median) = scan_nps(nps);
    (max, median.unwrap_or_else(|| median_in_place(nps)))
}

#[must_use]
pub fn measure_equally_spaced(data: &[u8], lanes: usize) -> Vec<bool> {
    match lanes {
        5 => equally_spaced_impl::<5>(data),
        8 => equally_spaced_impl::<8>(data),
        10 => equally_spaced_impl::<10>(data),
        _ => equally_spaced_impl::<4>(data),
    }
}

#[inline(always)]
const fn is_note(ch: u8) -> bool {
    matches!(ch, b'1' | b'2' | b'4')
}

fn equally_spaced_impl<const L: usize>(data: &[u8]) -> Vec<bool> {
    let measure_count = crate::stats::count_byte(data, b',') + 1;
    let mut results = Vec::with_capacity(measure_count);
    crate::stats::for_each_minimized_measure::<L, _>(data, |_, rows, _| {
        results.push(rows.iter().all(|row| row.iter().copied().any(is_note)));
    });
    results
}

#[cfg(test)]
mod tests {
    use super::{
        NPS_MEDIAN_SCAN_MIN, compute_measure_nps_vec_with_timing, get_nps_stats,
        get_nps_stats_with_scratch,
    };
    use crate::timing::{TimingFormat, compute_timing_segments, timing_data_from_segments};
    #[test]
    fn nps_stats_empty() {
        assert_eq!(get_nps_stats(&[]), (0.0, 0.0));
    }

    #[test]
    fn reusable_nps_stats_match_allocating_api_and_retain_capacity() {
        let cases = [
            vec![],
            vec![1.0],
            vec![3.0, 1.0, 2.0, 4.0],
            vec![0.0; NPS_MEDIAN_SCAN_MIN + 1],
            (0..NPS_MEDIAN_SCAN_MIN + 3)
                .map(|i| ((i * 37) % 23) as f64 / 3.0)
                .collect(),
        ];
        let mut scratch = Vec::new();

        for values in cases {
            assert_eq!(
                get_nps_stats_with_scratch(&values, &mut scratch),
                get_nps_stats(&values)
            );
        }

        let capacity = scratch.capacity();
        let short = [4.0, 1.0, 3.0, 2.0];
        assert_eq!(
            get_nps_stats_with_scratch(&short, &mut scratch),
            get_nps_stats(&short)
        );
        assert_eq!(scratch.capacity(), capacity);
    }
    #[test]
    fn nps_stats_even_median() {
        assert_eq!(get_nps_stats(&[8.0, 2.0, 4.0, 16.0]), (16.0, 6.0));
    }

    #[test]
    fn nps_medians_across_sizes() {
        let mut scratch = Vec::new();
        for (len, median) in [
            (256, 127.5),
            (1, 0.0),
            (2, 0.5),
            (3, 1.0),
            (32, 15.5),
            (63, 31.0),
            (64, 31.5),
            (65, 32.0),
        ] {
            let mut values: Vec<_> = (0..len).rev().map(f64::from).collect();
            values.rotate_left(len as usize / 3);
            let original = values.clone();
            let expected = (f64::from(len - 1), median);
            assert_eq!(get_nps_stats(&values), expected);
            assert_eq!(get_nps_stats_with_scratch(&values, &mut scratch), expected);
            assert_eq!(values, original);
        }
    }

    #[test]
    fn nps_preserves_special_values() {
        let mut scratch = vec![99.0; 128];
        for (values, expected) in [
            (&[-0.0][..], (0.0_f64, -0.0_f64)),
            (&[f64::INFINITY][..], (f64::INFINITY, f64::INFINITY)),
            (&[f64::NEG_INFINITY][..], (0.0, f64::NEG_INFINITY)),
            (&[-9.0, -1.0, -5.0][..], (0.0, -5.0)),
            (&[8.0, 2.0, 8.0, 2.0, 8.0][..], (8.0, 8.0)),
            (&[][..], (0.0, 0.0)),
        ] {
            let actual = get_nps_stats_with_scratch(values, &mut scratch);
            assert_eq!(actual.0.to_bits(), expected.0.to_bits());
            assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        }
        let (max, median) = get_nps_stats_with_scratch(&[f64::NAN], &mut scratch);
        assert_eq!(max, 0.0);
        assert!(median.is_nan());
    }

    #[test]
    fn nps_pairs_match_in_place() {
        let values = [
            f64::NEG_INFINITY,
            f64::MIN,
            -1.0,
            -0.0,
            0.0,
            1.0,
            f64::MAX,
            f64::INFINITY,
            f64::NAN,
        ];
        let mut scratch = Vec::new();
        for first in values {
            for second in values {
                let mut pair = [first, second];
                let expected = super::get_nps_stats_in_place(&mut pair);
                let actual = get_nps_stats_with_scratch(&[first, second], &mut scratch);
                assert_eq!(actual.0.to_bits(), expected.0.to_bits());
                if expected.1.is_nan() {
                    assert!(actual.1.is_nan());
                } else {
                    assert_eq!(actual.1.to_bits(), expected.1.to_bits());
                }
            }
        }
    }

    #[test]
    fn nps_stats_constant_median() {
        assert_eq!(get_nps_stats(&[7.5; NPS_MEDIAN_SCAN_MIN]), (7.5, 7.5));
    }

    #[test]
    fn nps_stats_zero_majority_median() {
        let mut nps = [0.0; NPS_MEDIAN_SCAN_MIN + 1];
        nps[0] = 12.0;
        nps[1] = 18.0;
        assert_eq!(get_nps_stats(&nps), (18.0, 0.0));
    }

    #[test]
    fn nps_stats_even_zero_half_median() {
        assert_eq!(get_nps_stats(&[0.0, 0.0, 10.0, 20.0]), (20.0, 5.0));
    }

    #[test]
    fn nps_fixed_timing_values() {
        let segments = compute_timing_segments(
            None,
            "0.000=120.000",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            None,
            "",
            TimingFormat::Ssc,
            true,
        );
        let timing = timing_data_from_segments(0.0, 0.0, &segments);

        assert_eq!(
            compute_measure_nps_vec_with_timing(&[16, 0, 32], &timing),
            vec![8.0, 0.0, 16.0]
        );
    }
}

use std::cmp::Ordering;

use crate::ChartNpsInfo;
use crate::bpm::{clean_timing_map, is_display_bpm};
use crate::parse::{
    decode_bytes, extract_sections, normalize_chart_desc, parse_offset_seconds, parse_version,
    unescape_trim,
};
use crate::timing::{
    TimingData, compute_timing_segments, get_time_for_beat_f32,
    steps_timing_allowed, timing_data_from_segments, timing_format_from_ext,
};

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
    let cleaned_global_bpms = clean_timing_map(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_stops = clean_timing_map(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_delays = clean_timing_map(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_warps = clean_timing_map(global_warps_raw);
    let global_speeds_raw = parsed_data
        .speeds
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_speeds = clean_timing_map(global_speeds_raw);
    let global_scrolls_raw = parsed_data
        .scrolls
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_scrolls = clean_timing_map(global_scrolls_raw);
    let global_fakes_raw = parsed_data
        .fakes
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_fakes = clean_timing_map(global_fakes_raw);

    let mut results = Vec::new();

    for entry in parsed_data.notes_list {
        if entry.field_count < 5 {
            continue;
        }
        let fields = entry.fields;
        let chart_data = entry.note_data;

        let Some(lanes) = crate::analysis::supported_stepstype_lanes_bytes(fields[0]) else {
            continue;
        };
        let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
        let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
        let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
        let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
        let meter_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
        let difficulty =
            crate::resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        let measure_densities = crate::stats::measure_densities(chart_data, lanes);

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
            cleaned_global_bpms.as_str(),
            cleaned_global_stops.as_str(),
            cleaned_global_delays.as_str(),
            cleaned_global_warps.as_str(),
            cleaned_global_speeds.as_str(),
            cleaned_global_scrolls.as_str(),
            cleaned_global_fakes.as_str(),
        );
        let chart_offset = timing_src.chart_offset_seconds;
        let chart_bpms = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_bpms.as_deref())
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_stops.as_deref())
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_delays.as_deref())
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_warps.as_deref())
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_speeds.as_deref())
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_scrolls.as_deref())
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            crate::chart_timing_tag_raw(entry.chart_fakes.as_deref())
        } else {
            None
        };
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
        let timing = timing_data_from_segments(chart_offset, 0.0, &timing_segments);

        let measure_nps_vec = compute_measure_nps_vec_with_timing(&measure_densities, &timing);
        let (max_nps, _median_nps) = get_nps_stats(&measure_nps_vec);

        results.push(ChartNpsInfo {
            step_type,
            difficulty,
            peak_nps: max_nps,
        });
    }

    Ok(results)
}

#[must_use] 
pub fn compute_measure_nps_vec(densities: &[usize], bpms: &[(f64, f64)]) -> Vec<f64> {
    compute_nps_iter(densities, |i| {
        let beat = i as f64 * 4.0;
        let idx = bpms.partition_point(|&(b, _)| b <= beat).saturating_sub(1);
        bpms.get(idx).map_or(0.0, |&(_, b)| b)
    })
}

#[must_use] 
pub fn compute_measure_nps_vec_with_timing(densities: &[usize], timing: &TimingData) -> Vec<f64> {
    densities
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            let beat = i as f64 * 4.0;
            let (start, end) = (
                get_time_for_beat_f32(timing, beat),
                get_time_for_beat_f32(timing, beat + 4.0),
            );
            let dur = end - start;
            if d == 0 || dur <= 0.12 {
                0.0
            } else {
                d as f64 / dur
            }
        })
        .collect()
}

fn compute_nps_iter<F: Fn(usize) -> f64>(densities: &[usize], get_bpm: F) -> Vec<f64> {
    densities
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            let bpm = get_bpm(i);
            if d == 0 || !is_display_bpm(bpm) {
                0.0
            } else {
                d as f64 * bpm / 240.0
            }
        })
        .collect()
}

fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    let mut v = arr.to_vec();
    let mid = v.len() / 2;
    v.select_nth_unstable_by(mid, |a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    if v.len() % 2 == 1 {
        v[mid]
    } else {
        f64::midpoint(v[..mid].iter().fold(f64::MIN, |a, &b| a.max(b)), v[mid])
    }
}

#[must_use] 
pub fn get_nps_stats(nps: &[f64]) -> (f64, f64) {
    (
        nps.iter().fold(f64::MIN, |a, &b| a.max(b)).max(0.0),
        median(nps),
    )
}

#[must_use] 
pub fn measure_equally_spaced(data: &[u8], lanes: usize) -> Vec<bool> {
    let lanes = if lanes == 8 { 8 } else { 4 };
    let minimized = crate::stats::minimize_chart_for_hash(data, lanes);
    if lanes == 8 {
        equally_spaced_impl::<8>(&minimized)
    } else {
        equally_spaced_impl::<4>(&minimized)
    }
}

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

#[inline(always)]
const fn is_note(ch: u8) -> bool {
    matches!(ch, b'1' | b'2' | b'4')
}

#[inline(always)]
fn has_step<const L: usize>(line: &[u8]) -> bool {
    line.iter().take(L).any(|&b| is_note(b))
}

fn equally_spaced_impl<const L: usize>(data: &[u8]) -> Vec<bool> {
    let mut results = Vec::new();
    let (mut rows, mut notes) = (0usize, 0usize);
    let mut saw_term = false;

    for raw in data.split(|&b| b == b'\n') {
        let line = trim_cr(raw);
        if line.is_empty() {
            continue;
        }

        match line[0] {
            b',' => {
                results.push(notes == rows);
                rows = 0;
                notes = 0;
            }
            b';' => {
                results.push(notes == rows);
                saw_term = true;
                break;
            }
            _ if line.len() >= L => {
                rows += 1;
                if has_step::<L>(line) {
                    notes += 1;
                }
            }
            _ => {}
        }
    }

    if !saw_term {
        results.push(notes == rows);
    }

    results
}

use std::cmp::Ordering;

use crate::ChartNpsInfo;
use crate::bpm::{clean_timing_map, is_display_bpm};
use crate::parse::{
    decode_bytes, extract_sections, normalize_chart_desc, parse_offset_seconds, parse_version,
    split_notes_fields, unescape_trim,
};
use crate::timing::{TimingData, TimingFormat, steps_timing_allowed};

pub fn compute_chart_peak_nps(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartNpsInfo>, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let timing_format = TimingFormat::from_extension(extension);
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
        let (fields, chart_data) = split_notes_fields(&entry.notes);
        if fields.len() < 5 {
            continue;
        }

        let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
        if step_type == "lights-cabinet" {
            continue;
        }
        let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
        let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
        let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
        let meter_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
        let difficulty =
            crate::resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        let lanes = crate::step_type_lanes(&step_type);
        let measure_densities = crate::stats::measure_densities(chart_data, lanes);

        let timing_tags = if allow_steps_timing {
            (
                crate::chart_timing_tag_raw(entry.chart_bpms.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_stops.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_delays.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_warps.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_speeds.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_scrolls.as_deref()),
                crate::chart_timing_tag_raw(entry.chart_fakes.as_deref()),
            )
        } else {
            (None, None, None, None, None, None, None)
        };
        let (
            chart_bpms,
            chart_stops,
            chart_delays,
            chart_warps,
            chart_speeds,
            chart_scrolls,
            chart_fakes,
        ) = timing_tags;

        let chart_offset = if allow_steps_timing && entry.chart_offset.is_some() {
            parse_offset_seconds(entry.chart_offset.as_deref())
        } else {
            song_offset
        };

        let chart_has_own_timing = allow_steps_timing
            && (entry.chart_bpms.is_some()
                || entry.chart_stops.is_some()
                || entry.chart_delays.is_some()
                || entry.chart_warps.is_some()
                || entry.chart_speeds.is_some()
                || entry.chart_scrolls.is_some()
                || entry.chart_fakes.is_some()
                || entry.chart_time_signatures.is_some()
                || entry.chart_labels.is_some()
                || entry.chart_tickcounts.is_some()
                || entry.chart_combos.is_some()
                || entry.chart_offset.is_some());

        let (
            timing_bpms_global,
            timing_stops_global,
            timing_delays_global,
            timing_warps_global,
            timing_speeds_global,
            timing_scrolls_global,
            timing_fakes_global,
        ) = if chart_has_own_timing {
            ("", "", "", "", "", "", "")
        } else {
            (
                cleaned_global_bpms.as_str(),
                cleaned_global_stops.as_str(),
                cleaned_global_delays.as_str(),
                cleaned_global_warps.as_str(),
                cleaned_global_speeds.as_str(),
                cleaned_global_scrolls.as_str(),
                cleaned_global_fakes.as_str(),
            )
        };

        let timing = TimingData::from_chart_data(
            chart_offset,
            0.0,
            chart_bpms.as_deref(),
            timing_bpms_global,
            chart_stops.as_deref(),
            timing_stops_global,
            chart_delays.as_deref(),
            timing_delays_global,
            chart_warps.as_deref(),
            timing_warps_global,
            chart_speeds.as_deref(),
            timing_speeds_global,
            chart_scrolls.as_deref(),
            timing_scrolls_global,
            chart_fakes.as_deref(),
            timing_fakes_global,
            timing_format,
            true,
        );

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

pub fn compute_measure_nps_vec(densities: &[usize], bpms: &[(f64, f64)]) -> Vec<f64> {
    compute_nps_iter(densities, |i| {
        let beat = i as f64 * 4.0;
        let idx = bpms.partition_point(|&(b, _)| b <= beat).saturating_sub(1);
        bpms.get(idx).map_or(0.0, |&(_, b)| b)
    })
}

pub fn compute_measure_nps_vec_with_timing(densities: &[usize], timing: &TimingData) -> Vec<f64> {
    densities
        .iter()
        .enumerate()
        .map(|(i, &d)| {
            let beat = i as f64 * 4.0;
            let (start, end) = (
                timing.get_time_for_beat_f32(beat),
                timing.get_time_for_beat_f32(beat + 4.0),
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
        (v[..mid].iter().fold(f64::MIN, |a, &b| a.max(b)) + v[mid]) / 2.0
    }
}

pub fn get_nps_stats(nps: &[f64]) -> (f64, f64) {
    (
        nps.iter().fold(f64::MIN, |a, &b| a.max(b)).max(0.0),
        median(nps),
    )
}

pub fn measure_equally_spaced(data: &[u8], lanes: usize) -> Vec<bool> {
    if lanes == 8 {
        equally_spaced_impl::<8>(data)
    } else {
        equally_spaced_impl::<4>(data)
    }
}

#[inline(always)]
fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(&[b'\r']).unwrap_or(line)
}

#[inline(always)]
fn is_note(ch: u8) -> bool {
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

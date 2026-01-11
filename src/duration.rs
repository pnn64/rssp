use crate::bpm::clean_timing_map;
use crate::math::round_sig_figs_itg;
use crate::parse::{
    decode_bytes, extract_sections, normalize_chart_desc, parse_offset_seconds, parse_version,
    split_notes_fields, unescape_trim,
};
use crate::timing::{TimingData, TimingFormat, steps_timing_allowed};

#[derive(Debug, Clone)]
pub struct ChartDuration {
    pub step_type: String,
    pub difficulty: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct TimingOffsets {
    pub global_offset_seconds: f64,
    pub group_offset_seconds: f64,
}

impl Default for TimingOffsets {
    fn default() -> Self {
        Self {
            global_offset_seconds: 0.0,
            group_offset_seconds: 0.0,
        }
    }
}

#[inline(always)]
pub(crate) fn chart_duration_seconds(
    last_beat: f64,
    timing: &TimingData,
    offsets: TimingOffsets,
) -> f64 {
    if last_beat <= 0.0 {
        return 0.0;
    }
    round_sig_figs_itg(
        timing.get_time_for_beat_f32(last_beat)
            - offsets.global_offset_seconds
            - offsets.group_offset_seconds,
    )
}

pub fn compute_chart_durations(
    simfile_data: &[u8],
    extension: &str,
    offsets: TimingOffsets,
) -> Result<Vec<ChartDuration>, String> {
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
        let (_, _, _, _, last_beat) = crate::stats::minimize_chart_count_rows(chart_data, lanes);

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

        let timing = TimingData::from_chart_data(
            chart_offset,
            0.0,
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
        let duration_seconds = chart_duration_seconds(last_beat, &timing, offsets);

        results.push(ChartDuration {
            step_type,
            difficulty,
            duration_seconds,
        });
    }

    Ok(results)
}

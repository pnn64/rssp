use crate::bpm::{chart_map_mode, clean_map_mode};
use crate::math::round_sig_figs_itg;
use crate::parse::{
    decode_unescape_trim, extract_sections, normalize_chart_desc_ref, parse_offset_seconds,
    parse_version,
};
use crate::timing::{
    TimingData, TimingFormat, TimingSegments, compute_timing_segments, get_time_for_beat_f32,
    steps_timing_allowed, timing_data_from_segments, timing_format_from_ext,
};

#[derive(Debug, Clone)]
pub struct ChartDuration {
    pub step_type: String,
    pub difficulty: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TimingOffsets {
    pub global_offset_seconds: f64,
    pub group_offset_seconds: f64,
}

#[inline(always)]
pub fn chart_duration_seconds(last_beat: f64, timing: &TimingData, offsets: TimingOffsets) -> f64 {
    if last_beat <= 0.0 {
        return 0.0;
    }
    round_sig_figs_itg(
        get_time_for_beat_f32(timing, last_beat)
            - offsets.global_offset_seconds
            - offsets.group_offset_seconds,
    )
}

#[allow(clippy::too_many_arguments)]
fn compute_duration_timing_segments(
    chart_bpms: Option<&str>,
    global_bpms: &str,
    chart_stops: Option<&str>,
    global_stops: &str,
    chart_delays: Option<&str>,
    global_delays: &str,
    chart_warps: Option<&str>,
    global_warps: &str,
    format: TimingFormat,
) -> TimingSegments {
    compute_timing_segments(
        chart_bpms,
        global_bpms,
        chart_stops,
        global_stops,
        chart_delays,
        global_delays,
        chart_warps,
        global_warps,
        None,
        "",
        None,
        "",
        None,
        "",
        format,
        true,
    )
}

pub fn compute_chart_durations(
    simfile_data: &[u8],
    extension: &str,
    offsets: TimingOffsets,
) -> Result<Vec<ChartDuration>, String> {
    compute_chart_durations_impl::<true>(simfile_data, extension, offsets)
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn chart_durations_owned(
    simfile_data: &[u8],
    extension: &str,
    offsets: TimingOffsets,
) -> Result<Vec<ChartDuration>, String> {
    compute_chart_durations_impl::<false>(simfile_data, extension, offsets)
}

fn compute_chart_durations_impl<const BORROW: bool>(
    simfile_data: &[u8],
    extension: &str,
    offsets: TimingOffsets,
) -> Result<Vec<ChartDuration>, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let timing_format = timing_format_from_ext(extension);
    let ssc_version = parse_version(parsed_data.version, timing_format);
    let allow_steps_timing = steps_timing_allowed(ssc_version, timing_format);
    let song_offset = parse_offset_seconds(parsed_data.offset);

    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"")).unwrap_or("");
    let cleaned_global_bpms = clean_map_mode::<BORROW>(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_stops = clean_map_mode::<BORROW>(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_delays = clean_map_mode::<BORROW>(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let cleaned_global_warps = clean_map_mode::<BORROW>(global_warps_raw);

    let entries = parsed_data.notes_list;
    let mut results = Vec::with_capacity(entries.len());
    let mut global_timing = None;

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

        let last_beat = crate::stats::chart_last_beat(chart_data, lanes);

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
            "",
            "",
            "",
        );
        let chart_offset = timing_src.chart_offset_seconds;
        let chart_bpms = if allow_steps_timing {
            chart_map_mode::<BORROW>(entry.chart_bpms.as_deref())
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_map_mode::<BORROW>(entry.chart_stops.as_deref())
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_map_mode::<BORROW>(entry.chart_delays.as_deref())
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_map_mode::<BORROW>(entry.chart_warps.as_deref())
        } else {
            None
        };
        let chart_timing;
        let timing = if timing_src.chart_has_own_timing {
            let timing_segments = compute_duration_timing_segments(
                chart_bpms.as_deref(),
                timing_src.global_bpms,
                chart_stops.as_deref(),
                timing_src.global_stops,
                chart_delays.as_deref(),
                timing_src.global_delays,
                chart_warps.as_deref(),
                timing_src.global_warps,
                timing_format,
            );
            chart_timing = timing_data_from_segments(chart_offset, 0.0, &timing_segments);
            &chart_timing
        } else {
            global_timing.get_or_insert_with(|| {
                let timing_segments = compute_duration_timing_segments(
                    None,
                    &cleaned_global_bpms,
                    None,
                    &cleaned_global_stops,
                    None,
                    &cleaned_global_delays,
                    None,
                    &cleaned_global_warps,
                    timing_format,
                );
                timing_data_from_segments(song_offset, 0.0, &timing_segments)
            })
        };
        let duration_seconds = chart_duration_seconds(last_beat, timing, offsets);

        results.push(ChartDuration {
            step_type,
            difficulty,
            duration_seconds,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{TimingOffsets, compute_chart_durations, compute_chart_durations_impl};
    use crate::timing::{
        TimingFormat, compute_timing_segments, get_time_for_beat_f32, resolve_chart_timing,
        timing_data_from_segments,
    };

    const INHERITED_TIMING_FIXTURE: &[u8] =
        include_bytes!("../../rssp/benches/fixtures/camellia_mix.ssc");

    #[test]
    fn inherited_timing_cache_preserves_chart_durations() {
        let charts =
            compute_chart_durations(INHERITED_TIMING_FIXTURE, "ssc", TimingOffsets::default())
                .expect("fixture should parse");
        let owned = compute_chart_durations_impl::<false>(
            INHERITED_TIMING_FIXTURE,
            "ssc",
            TimingOffsets::default(),
        )
        .expect("owned timing fixture should parse");
        assert_eq!(charts.len(), owned.len());
        for (actual, expected) in charts.iter().zip(&owned) {
            assert_eq!(actual.step_type, expected.step_type);
            assert_eq!(actual.difficulty, expected.difficulty);
            assert_eq!(actual.duration_seconds, expected.duration_seconds);
        }
        let difficulties: Vec<_> = charts
            .iter()
            .map(|chart| chart.difficulty.as_str())
            .collect();

        assert_eq!(
            difficulties,
            ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
        );
        assert!(charts.iter().all(|chart| chart.step_type == "dance-single"));
        assert!(
            charts
                .iter()
                .all(|chart| chart.duration_seconds == 7_367.31)
        );
    }

    #[test]
    fn visual_timing_maps_do_not_change_elapsed_time() {
        for format in [TimingFormat::Ssc, TimingFormat::Sm] {
            let full_segments = compute_timing_segments(
                Some("0=120,8=180,20=90"),
                "",
                Some("4=0.5,18=0.25"),
                "",
                Some("12=0.125"),
                "",
                Some("24=4"),
                "",
                Some("0=1=0=0,4=2=1=1,8=0.5=0.25=0"),
                "",
                Some("0=1,2=0.5,6=-1"),
                "",
                Some("1=0.5,8=4,16=2"),
                "",
                format,
                true,
            );
            let time_only_segments = compute_timing_segments(
                Some("0=120,8=180,20=90"),
                "",
                Some("4=0.5,18=0.25"),
                "",
                Some("12=0.125"),
                "",
                Some("24=4"),
                "",
                None,
                "",
                None,
                "",
                None,
                "",
                format,
                true,
            );
            let full_timing = timing_data_from_segments(0.25, 0.0, &full_segments);
            let time_only_timing = timing_data_from_segments(0.25, 0.0, &time_only_segments);

            for beat in [0.0, 1.0, 4.0, 8.0, 12.0, 18.0, 24.0, 28.0, 32.0] {
                assert_eq!(
                    get_time_for_beat_f32(&full_timing, beat).to_bits(),
                    get_time_for_beat_f32(&time_only_timing, beat).to_bits(),
                    "elapsed time changed at beat {beat} for {format:?}"
                );
            }
        }
    }

    #[test]
    fn visual_only_chart_timing_still_disables_global_timing() {
        let timing = resolve_chart_timing(
            true,
            0.0,
            None,
            None,
            None,
            None,
            None,
            Some(b"0=2=0=0"),
            None,
            None,
            None,
            None,
            None,
            None,
            "0=120",
            "",
            "",
            "",
            "",
            "",
            "",
        );

        assert!(timing.chart_has_own_timing);
        assert_eq!(timing.global_bpms, "");
    }
}

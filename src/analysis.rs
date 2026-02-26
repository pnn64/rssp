use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;

use crate::duration::{self, TimingOffsets};
use crate::report::{ChartSummary, SimfileSummary};
use crate::stats;
use crate::step_parity;

use crate::bpm::{clean_timing_map, normalize_float_digits, clean_timing_map_cow, compute_tier_bpm, compute_measure_nps_vec_with_timing, get_nps_stats, compute_bpm_range, compute_bpm_stats};
use crate::hash::compute_chart_hash;
use crate::math::{round_dp, round_sig_figs_6};
use crate::matrix::compute_matrix_rating;
use crate::parse::{decode_bytes, ParsedChartEntry, unescape_trim, normalize_chart_desc, unescape_tag, clean_tag, extract_sections, strip_title_tags, parse_offset_seconds, parse_version};
use crate::stats::{RADAR_CATEGORY_COUNT, StreamCounts, compute_stream_counts, generate_breakdown, BreakdownMode, stream_breakdown, StreamBreakdownLevel, minimize_chart_rows_bits, minimize_rows_typed, compute_timing_aware_stats_from_rows_with_row_to_beat, compute_timing_aware_stats_with_row_to_beat, minimize_chart_for_hash};
use crate::tech::parse_tech_notation;
use crate::timing::{
    TimingFormat, compute_timing_segments, get_time_for_beat, steps_timing_allowed,
    timing_data_from_segments, timing_format_from_ext, TimingSegments,
};
use crate::patterns::{
    compile_custom_patterns, compiled_custom_empty, compiled_custom_is_empty,
    detect_custom_patterns_compiled, detect_default_patterns, count_anchors,
    count_facing_steps, PatternVariant, CompiledCustomPatterns, PatternCounts, PATTERN_COUNT,
};

/// Options for controlling simfile analysis.
#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub strip_tags: bool,
    pub mono_threshold: usize,
    pub custom_patterns: Vec<String>,
    pub compute_tech_counts: bool,
    pub compute_pattern_counts: bool,
    pub translate_markers: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            strip_tags: false,
            mono_threshold: 0,
            custom_patterns: Vec::new(),
            compute_tech_counts: true,
            compute_pattern_counts: true,
            translate_markers: false,
        }
    }
}

#[derive(Debug)]
pub struct ChartHashInfo {
    pub step_type: String,
    pub difficulty: String,
    pub hash: String,
}

#[derive(Debug, Clone)]
pub struct ChartNpsInfo {
    pub step_type: String,
    pub difficulty: String,
    pub peak_nps: f64,
}

/// Normalizes common difficulty labels to a canonical form (e.g. Expert -> Challenge).
pub fn normalize_difficulty_label(raw: &str) -> String {
    old_style_difficulty_label(raw).map_or_else(|| raw.trim().to_string(), str::to_string)
}

fn canonical_difficulty_label(raw: &str) -> Option<&'static str> {
    let lowered = raw.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "beginner" => Some("Beginner"),
        "easy" => Some("Easy"),
        "medium" => Some("Medium"),
        "hard" => Some("Hard"),
        "challenge" => Some("Challenge"),
        "edit" => Some("Edit"),
        _ => None,
    }
}

fn old_style_difficulty_label(raw: &str) -> Option<&'static str> {
    let lowered = raw.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "beginner" => Some("Beginner"),
        "easy" | "basic" | "light" => Some("Easy"),
        "medium" | "another" | "trick" | "standard" | "difficult" => Some("Medium"),
        "hard" | "ssr" | "maniac" | "heavy" => Some("Hard"),
        "challenge" | "expert" | "oni" | "smaniac" => Some("Challenge"),
        "edit" => Some("Edit"),
        _ => None,
    }
}

fn parse_meter_for_difficulty(meter_str: &str, extension: &str) -> i32 {
    let trimmed = meter_str.trim();
    if extension.eq_ignore_ascii_case("sm") && trimmed.is_empty() {
        return 1;
    }
    trimmed.parse::<i32>().unwrap_or(0)
}

#[must_use] 
pub fn resolve_difficulty_label(
    raw_difficulty: &str,
    description: &str,
    meter_str: &str,
    extension: &str,
) -> String {
    // Match ITGmania Steps::TidyUpData fallback when difficulty is invalid.
    let mut difficulty = if extension.eq_ignore_ascii_case("sm") {
        old_style_difficulty_label(raw_difficulty)
    } else {
        canonical_difficulty_label(raw_difficulty)
    };

    if extension.eq_ignore_ascii_case("sm") && difficulty == Some("Hard") {
        let desc = description.trim();
        if desc.eq_ignore_ascii_case("smaniac") || desc.eq_ignore_ascii_case("challenge") {
            difficulty = Some("Challenge");
        }
    }

    if difficulty.is_none() {
        difficulty = canonical_difficulty_label(description);
    }

    if let Some(label) = difficulty {
        return label.to_string();
    }

    let meter = parse_meter_for_difficulty(meter_str, extension);
    if meter == 1 {
        "Beginner".to_string()
    } else if meter <= 3 {
        "Easy".to_string()
    } else if meter <= 6 {
        "Medium".to_string()
    } else {
        "Hard".to_string()
    }
}

#[must_use] 
pub fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "dance-double" => 8,
        _ => 4,
    }
}

#[inline(always)]
const fn trim_ascii_ws(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if b.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((&b, rest)) = s.split_last() {
        if b.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

#[inline(always)]
pub(crate) const fn supported_stepstype_lanes_bytes(raw: &[u8]) -> Option<usize> {
    let s = trim_ascii_ws(raw);
    if s.eq_ignore_ascii_case(b"dance-single") || s.eq_ignore_ascii_case(b"dance_single") {
        Some(4)
    } else if s.eq_ignore_ascii_case(b"dance-double") || s.eq_ignore_ascii_case(b"dance_double") {
        Some(8)
    } else {
        None
    }
}

#[must_use] 
pub fn display_metadata(
    title: &str,
    subtitle: &str,
    artist: &str,
    title_translit: &str,
    subtitle_translit: &str,
    artist_translit: &str,
    show_native: bool,
) -> (String, String, String) {
    if show_native {
        return (title.to_string(), subtitle.to_string(), artist.to_string());
    }
    let title_out = if title_translit.is_empty() {
        title
    } else {
        title_translit
    };
    let subtitle_out = if subtitle_translit.is_empty() {
        subtitle
    } else {
        subtitle_translit
    };
    let artist_out = if artist_translit.is_empty() {
        artist
    } else {
        artist_translit
    };
    (
        title_out.to_string(),
        subtitle_out.to_string(),
        artist_out.to_string(),
    )
}

fn chart_timing_tag_pair(tag: Option<&[u8]>) -> (Option<String>, Option<String>) {
    let Some(bytes) = tag else {
        return (None, None);
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return (None, None);
    };
    let raw = clean_timing_map(text);
    let norm = normalize_float_digits(text);
    let raw = if raw.is_empty() { None } else { Some(raw) };
    let norm = if norm.is_empty() { None } else { Some(norm) };
    (raw, norm)
}

pub(crate) fn chart_timing_tag_raw(tag: Option<&[u8]>) -> Option<String> {
    let bytes = tag?;
    let text = std::str::from_utf8(bytes).ok()?;
    let cleaned = clean_timing_map(text);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn chart_display_bpm_tag(tag: Option<&[u8]>) -> Option<String> {
    let bytes = tag?;
    let text = decode_bytes(bytes);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn msd_first_param_bytes(bytes: &[u8]) -> &[u8] {
    let mut bs_run = 0usize;
    for (idx, &b) in bytes.iter().enumerate() {
        if b == b':' && bs_run.is_multiple_of(2) {
            return &bytes[..idx];
        }
        if b == b'\\' {
            bs_run += 1;
        } else {
            bs_run = 0;
        }
    }
    bytes
}

const RADAR_CATEGORY_NOTES: usize = 5;

fn parse_radar_values_bytes(
    raw: Option<&[u8]>,
    split_players: bool,
) -> Option<[f32; RADAR_CATEGORY_COUNT]> {
    let bytes = raw?;
    let text = std::str::from_utf8(bytes).ok()?;
    parse_radar_values_str(text, split_players)
}

fn parse_radar_values_str(raw: &str, split_players: bool) -> Option<[f32; RADAR_CATEGORY_COUNT]> {
    let cleaned = clean_timing_map_cow(raw);
    let cleaned = cleaned.as_ref();
    if cleaned.is_empty() {
        return None;
    }

    let mut out = [0.0f32; RADAR_CATEGORY_COUNT];
    let mut filled = 0usize;
    let mut total = 0usize;

    for part in cleaned.split(',') {
        if part.is_empty() {
            continue;
        }
        let Ok(value) = part.trim().parse::<f32>() else {
            continue;
        };
        if filled < RADAR_CATEGORY_COUNT {
            out[filled] = value;
            filled += 1;
        }
        total += 1;
    }

    let needed = if split_players {
        RADAR_CATEGORY_COUNT * 2
    } else {
        RADAR_CATEGORY_COUNT
    };
    if total < needed {
        return None;
    }
    if out
        .iter()
        .skip(RADAR_CATEGORY_NOTES)
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return None;
    }

    Some(out)
}

/// Detects predefined patterns and counts anchors from note bitmasks.
fn compute_pattern_and_anchor_stats(
    bitmasks: &[u8],
) -> (PatternCounts, (u32, u32, u32, u32)) {
    let detected_patterns = detect_default_patterns(bitmasks);
    let anchors = count_anchors(bitmasks);
    (detected_patterns, anchors)
}

/// Calculates mono (same-foot patterns) and candle stats.
fn compute_mono_and_candle_stats(
    bitmasks: &[u8],
    stats: &stats::ArrowStats,
    detected_patterns: &PatternCounts,
    options: &AnalysisOptions,
) -> (u32, u32, u32, f64, u32, f64) {
    if stats.total_steps <= 1 {
        return (0, 0, 0, 0.0, 0, 0.0);
    }

    let (facing_left, facing_right) = count_facing_steps(bitmasks, options.mono_threshold);
    let mono_total = facing_left + facing_right;
    let mono_percent = if stats.total_steps > 0 {
        (f64::from(mono_total) / f64::from(stats.total_steps)) * 100.0
    } else {
        0.0
    };

    let candle_left = detected_patterns[PatternVariant::CandleLeft as usize];
    let candle_right = detected_patterns[PatternVariant::CandleRight as usize];
    let candle_total = candle_left + candle_right;

    let max_candles = (stats.total_steps.saturating_sub(1)) / 2;
    let candle_percent = if max_candles > 0 {
        (f64::from(candle_total) / f64::from(max_candles)) * 100.0
    } else {
        0.0
    };

    (
        facing_left,
        facing_right,
        mono_total,
        mono_percent,
        candle_total,
        candle_percent,
    )
}

// A private helper struct to bundle metrics derived from density and BPMs.
struct DerivedChartMetrics {
    stream_counts: StreamCounts,
    total_streams: u32,
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
    short_hash: String,
    bpm_neutral_hash: String,
    tier_bpm: f64,
    matrix_rating: f64,
}

// Computes various metrics derived from measure densities and the BPM map.
fn compute_derived_chart_metrics(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    minimized_chart: &[u8],
    bpms_to_use: &str,
) -> DerivedChartMetrics {
    let stream_counts = compute_stream_counts(measure_densities);
    let total_streams = stream_counts.run16_streams
        + stream_counts.run20_streams
        + stream_counts.run24_streams
        + stream_counts.run32_streams;

    let sn_detailed_breakdown = generate_breakdown(measure_densities, BreakdownMode::Detailed);
    let sn_partial_breakdown = generate_breakdown(measure_densities, BreakdownMode::Partial);
    let sn_simple_breakdown = generate_breakdown(measure_densities, BreakdownMode::Simplified);

    let detailed_breakdown = stream_breakdown(measure_densities, StreamBreakdownLevel::Detailed);
    let partial_breakdown = stream_breakdown(measure_densities, StreamBreakdownLevel::Partial);
    let simple_breakdown = stream_breakdown(measure_densities, StreamBreakdownLevel::Simple);

    let short_hash = compute_chart_hash(minimized_chart, bpms_to_use);
    let bpm_neutral_hash = compute_chart_hash(minimized_chart, "0.000=0.000");
    let tier_bpm = round_dp(compute_tier_bpm(measure_densities, bpm_map, 4.0), 2);
    let matrix_rating = round_dp(compute_matrix_rating(measure_densities, bpm_map), 2);

    DerivedChartMetrics {
        stream_counts,
        total_streams,
        sn_detailed_breakdown,
        sn_partial_breakdown,
        sn_simple_breakdown,
        detailed_breakdown,
        partial_breakdown,
        simple_breakdown,
        short_hash,
        bpm_neutral_hash,
        tier_bpm,
        matrix_rating,
    }
}

/// Processes a single chart's data to produce a `ChartSummary`.
fn build_chart_summary(
    entry: &ParsedChartEntry<'_>,
    global_attacks_opt: Option<&[u8]>,
    global_bpms_raw: &str,
    global_stops_raw: &str,
    global_delays_raw: &str,
    global_warps_raw: &str,
    global_speeds_raw: &str,
    global_scrolls_raw: &str,
    global_fakes_raw: &str,
    global_bpms_norm: &str,
    global_timing_segments: &Arc<TimingSegments>,
    global_bpm_map: &[(f64, f64)],
    song_offset: f64,
    extension: &str,
    timing_format: TimingFormat,
    ssc_version: f32,
    allow_steps_timing: bool,
    compiled_custom_patterns: &CompiledCustomPatterns,
    parity_scratch4: &mut step_parity::TimingRowsScratch<4>,
    parity_scratch8: &mut step_parity::TimingRowsScratch<8>,
    options: &AnalysisOptions,
) -> Option<(ChartSummary, i32)> {
    let chart_start_time = Instant::now();

    if entry.field_count < 5 {
        return None;
    }
    let fields = entry.fields;
    let chart_data = entry.note_data;
    let lanes = supported_stepstype_lanes_bytes(fields[0])?;

    let chart_bpms_opt = entry.chart_bpms.as_deref();
    let chart_attacks_opt = entry.chart_attacks.as_deref().or(global_attacks_opt);
    let chart_delays_opt = entry.chart_delays.as_deref();
    let chart_warps_opt = entry.chart_warps.as_deref();
    let chart_stops_opt = entry.chart_stops.as_deref();
    let chart_speeds_opt = entry.chart_speeds.as_deref();
    let chart_scrolls_opt = entry.chart_scrolls.as_deref();
    let chart_fakes_opt = entry.chart_fakes.as_deref();
    let chart_time_signatures_opt = entry.chart_time_signatures.as_deref();
    let chart_labels_opt = entry.chart_labels.as_deref();
    let chart_tickcounts_opt = entry.chart_tickcounts.as_deref();
    let chart_combos_opt = entry.chart_combos.as_deref();
    let chart_display_bpm_opt = entry.chart_display_bpm.as_deref();
    let chart_offset_opt = entry.chart_offset.as_deref();
    let chart_radar_values_opt = entry.chart_radar_values.as_deref();

    let step_type_str = unescape_trim(decode_bytes(fields[0]).as_ref());

    let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
    let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
    let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
    let rating_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
    let difficulty_str =
        resolve_difficulty_label(&difficulty_raw, &description, &rating_raw, extension);
    let rating_str = rating_raw;
    let is_ssc = extension.eq_ignore_ascii_case("ssc");
    let credit_decoded = if is_ssc {
        decode_bytes(fields[4])
    } else {
        Cow::Borrowed("")
    };
    let credit = unescape_tag(credit_decoded.as_ref());
    let tech_notation_str = parse_tech_notation(credit.as_ref(), &description);
    let step_artist_str = if is_ssc {
        credit.into_owned()
    } else {
        description.clone()
    };

    let compute_patterns = lanes == 4 && options.compute_pattern_counts;
    let (mut rows4, mut rows8) = (Vec::new(), Vec::new());
    let (mut minimized_chart, mut stats, measure_densities, row_to_beat, last_beat, bitmasks) =
        if compute_patterns {
            let (chart, stats, densities, rows, row_to_beat, last_beat, bitmasks) =
                minimize_chart_rows_bits(chart_data);
            rows4 = rows;
            (
                chart,
                stats,
                densities,
                row_to_beat,
                last_beat,
                Some(bitmasks),
            )
        } else if lanes == 8 {
            let (chart, stats, densities, rows, row_to_beat, last_beat) =
                minimize_rows_typed::<8>(chart_data);
            rows8 = rows;
            (chart, stats, densities, row_to_beat, last_beat, None)
        } else {
            let (chart, stats, densities, rows, row_to_beat, last_beat) =
                minimize_rows_typed::<4>(chart_data);
            rows4 = rows;
            (chart, stats, densities, row_to_beat, last_beat, None)
        };
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }

    let (chart_bpms, chart_bpms_norm) = chart_timing_tag_pair(chart_bpms_opt);
    let bpms_to_use = chart_bpms_norm
        
        .unwrap_or_else(|| global_bpms_norm.to_string());
    let chart_stops = chart_timing_tag_raw(chart_stops_opt);
    let chart_speeds = chart_timing_tag_raw(chart_speeds_opt);
    let chart_delays = chart_timing_tag_raw(chart_delays_opt);
    let chart_scrolls = chart_timing_tag_raw(chart_scrolls_opt);
    let chart_warps = chart_timing_tag_raw(chart_warps_opt);
    let chart_fakes = chart_timing_tag_raw(chart_fakes_opt);

    let chart_bpms_timing = if allow_steps_timing {
        chart_bpms.as_deref()
    } else {
        None
    };
    let chart_stops_timing = if allow_steps_timing {
        chart_stops.as_deref()
    } else {
        None
    };
    let chart_delays_timing = if allow_steps_timing {
        chart_delays.as_deref()
    } else {
        None
    };
    let chart_warps_timing = if allow_steps_timing {
        chart_warps.as_deref()
    } else {
        None
    };
    let chart_speeds_timing = if allow_steps_timing {
        chart_speeds.as_deref()
    } else {
        None
    };
    let chart_scrolls_timing = if allow_steps_timing {
        chart_scrolls.as_deref()
    } else {
        None
    };
    let chart_fakes_timing = if allow_steps_timing {
        chart_fakes.as_deref()
    } else {
        None
    };
    let chart_time_signatures = chart_time_signatures_opt.and_then(|bytes| {
        let decoded = decode_bytes(bytes);
        let trimmed = decoded.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let chart_labels = chart_labels_opt.and_then(|bytes| {
        let first_param = msd_first_param_bytes(bytes);
        let decoded = decode_bytes(first_param);
        let unescaped = unescape_tag(decoded.as_ref());
        let cleaned = clean_tag(unescaped.as_ref());
        let trimmed = cleaned.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let chart_tickcounts = chart_tickcounts_opt.and_then(|bytes| {
        std::str::from_utf8(bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let chart_combos = chart_combos_opt.and_then(|bytes| {
        std::str::from_utf8(bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let chart_attacks = chart_attacks_opt.and_then(|bytes| {
        std::str::from_utf8(bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let chart_display_bpm = chart_display_bpm_tag(chart_display_bpm_opt);
    let timing_src = crate::timing::resolve_chart_timing(
        allow_steps_timing,
        song_offset,
        chart_offset_opt,
        chart_bpms_opt,
        chart_stops_opt,
        chart_delays_opt,
        chart_warps_opt,
        chart_speeds_opt,
        chart_scrolls_opt,
        chart_fakes_opt,
        chart_time_signatures_opt,
        chart_labels_opt,
        chart_tickcounts_opt,
        chart_combos_opt,
        global_bpms_raw,
        global_stops_raw,
        global_delays_raw,
        global_warps_raw,
        global_speeds_raw,
        global_scrolls_raw,
        global_fakes_raw,
    );
    let chart_offset = timing_src.chart_offset_seconds;
    let cached_radar_values = if extension.eq_ignore_ascii_case("sm") {
        parse_radar_values_bytes(Some(fields[4]), false)
    } else {
        parse_radar_values_bytes(chart_radar_values_opt, true)
    };
    let chart_has_own_timing = timing_src.chart_has_own_timing;
    let (timing_segments, bpm_map): (Arc<TimingSegments>, Cow<'_, [(f64, f64)]>) =
        if chart_has_own_timing {
            let timing_segments = Arc::new(compute_timing_segments(
                chart_bpms_timing,
                timing_src.global_bpms,
                chart_stops_timing,
                timing_src.global_stops,
                chart_delays_timing,
                timing_src.global_delays,
                chart_warps_timing,
                timing_src.global_warps,
                chart_speeds_timing,
                timing_src.global_speeds,
                chart_scrolls_timing,
                timing_src.global_scrolls,
                chart_fakes_timing,
                timing_src.global_fakes,
                timing_format,
                true,
            ));
            let bpm_map = timing_segments
                .bpms
                .iter()
                .map(|(beat, bpm)| (f64::from(*beat), f64::from(*bpm)))
                .collect();
            (timing_segments, Cow::Owned(bpm_map))
        } else {
            (Arc::clone(global_timing_segments), Cow::Borrowed(global_bpm_map))
        };

    let metrics = compute_derived_chart_metrics(
        &measure_densities,
        bpm_map.as_ref(),
        &minimized_chart,
        &bpms_to_use,
    );

    let (detected_patterns, (anchor_left, anchor_down, anchor_up, anchor_right)) =
        bitmasks.as_ref().map_or(
            ([0u32; PATTERN_COUNT], (0, 0, 0, 0)),
            |bm| compute_pattern_and_anchor_stats(bm.as_slice()),
        );

    let (facing_left, facing_right, mono_total, mono_percent_raw, candle_total, candle_percent_raw) =
        bitmasks.as_ref().map_or(
            (0, 0, 0, 0.0, 0, 0.0),
            |bm| compute_mono_and_candle_stats(bm.as_slice(), &stats, &detected_patterns, options),
        );
    let mono_percent = round_dp(mono_percent_raw, 2);
    let candle_percent = round_dp(candle_percent_raw, 2);

    let custom_patterns = if compute_patterns && !compiled_custom_is_empty(compiled_custom_patterns) {
        detect_custom_patterns_compiled(bitmasks.as_ref().unwrap(), compiled_custom_patterns)
    } else {
        Vec::new()
    };

    let timing = timing_data_from_segments(chart_offset, 0.0, &timing_segments);

    let duration_seconds =
        duration::chart_duration_seconds(last_beat, &timing, TimingOffsets::default());
    let chart_length = if last_beat <= 0.0 {
        0
    } else {
        let time_chart_f64 = get_time_for_beat(&timing, last_beat);
        (time_chart_f64 + (song_offset - chart_offset)).floor() as i32
    };

    let measure_nps_vec_raw = compute_measure_nps_vec_with_timing(&measure_densities, &timing);
    let (max_nps_raw, median_nps_raw) = get_nps_stats(&measure_nps_vec_raw);
    let max_nps = round_sig_figs_6(max_nps_raw);
    let median_nps = round_dp(median_nps_raw, 2);
    let measure_nps_vec = measure_nps_vec_raw
        .into_iter()
        .map(round_sig_figs_6)
        .collect();

    let raw_total_steps = stats.total_steps;
    let raw_holding = stats.holding;
    let (tech_counts, mut timing_stats) = match lanes {
        4 => {
            let timing_stats = compute_timing_aware_stats_from_rows_with_row_to_beat::<4>(
                &rows4,
                &timing,
                &row_to_beat,
            );
            let tech_counts = if options.compute_tech_counts {
                step_parity::analyze_timing_rows::<4>(
                    &rows4,
                    &row_to_beat,
                    &timing,
                    parity_scratch4,
                )
            } else {
                step_parity::TechCounts::default()
            };
            (tech_counts, timing_stats)
        }
        8 => {
            let timing_stats = compute_timing_aware_stats_from_rows_with_row_to_beat::<8>(
                &rows8,
                &timing,
                &row_to_beat,
            );
            let tech_counts = if options.compute_tech_counts {
                step_parity::analyze_timing_rows::<8>(
                    &rows8,
                    &row_to_beat,
                    &timing,
                    parity_scratch8,
                )
            } else {
                step_parity::TechCounts::default()
            };
            (tech_counts, timing_stats)
        }
        _ => {
            let tech_counts = if options.compute_tech_counts {
                step_parity::analyze_timing_lanes(&minimized_chart, &timing, lanes)
            } else {
                step_parity::TechCounts::default()
            };
            let timing_stats = compute_timing_aware_stats_with_row_to_beat(
                &minimized_chart,
                lanes,
                &timing,
                &row_to_beat,
            );
            (tech_counts, timing_stats)
        }
    };
    timing_stats.total_steps = raw_total_steps;
    timing_stats.holding = raw_holding;
    let mines_nonfake = timing_stats.mines;
    stats = timing_stats;

    let elapsed_chart = chart_start_time.elapsed();

    Some((
        ChartSummary {
            step_type_str,
            step_artist_str,
            description_str: description,
            difficulty_str,
            rating_str,
            tech_notation_str,
            tier_bpm: metrics.tier_bpm,
            matrix_rating: metrics.matrix_rating,
            stats,
            stream_counts: metrics.stream_counts,
            total_streams: metrics.total_streams,
            mines_nonfake,
            total_measures: measure_densities.len(),
            sn_detailed_breakdown: metrics.sn_detailed_breakdown,
            sn_partial_breakdown: metrics.sn_partial_breakdown,
            sn_simple_breakdown: metrics.sn_simple_breakdown,
            detailed_breakdown: metrics.detailed_breakdown,
            partial_breakdown: metrics.partial_breakdown,
            simple_breakdown: metrics.simple_breakdown,
            max_nps,
            median_nps,
            duration_seconds,
            detected_patterns,
            anchor_left,
            anchor_down,
            anchor_up,
            anchor_right,
            facing_left,
            facing_right,
            mono_total,
            mono_percent,
            candle_total,
            candle_percent,
            tech_counts,
            custom_patterns,
            short_hash: metrics.short_hash,
            bpm_neutral_hash: metrics.bpm_neutral_hash,
            elapsed: elapsed_chart,
            measure_densities,
            measure_nps_vec,
            row_to_beat,
            timing_segments,
            chart_offset_seconds: chart_offset,
            chart_has_own_timing,
            minimized_note_data: minimized_chart,
            chart_attacks,
            chart_stops,
            chart_speeds,
            chart_scrolls,
            chart_bpms,
            chart_delays,
            chart_warps,
            chart_fakes,
            chart_display_bpm,
            chart_time_signatures,
            chart_labels,
            chart_tickcounts,
            chart_combos,
            cached_radar_values,
        },
        chart_length,
    ))
}

pub fn analyze(
    simfile_data: &[u8],
    extension: &str,
    options: &AnalysisOptions,
) -> Result<SimfileSummary, String> {
    let total_start_time = Instant::now();

    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let mut title_str = parsed_data.title.map_or_else(
        || "<invalid-title>".to_string(),
        |b| {
            let decoded = decode_bytes(b);
            let unescaped = unescape_tag(decoded.as_ref());
            clean_tag(unescaped.as_ref()).into_owned()
        },
    );
    if options.strip_tags {
        let stripped = strip_title_tags(&title_str);
        if stripped.as_ref() != title_str.as_str() {
            title_str = stripped.into_owned();
        }
    }
    let trimmed_title = title_str.trim();
    if trimmed_title.len() != title_str.len() {
        title_str = trimmed_title.to_string();
    }

    let mut subtitle_str = parsed_data
        .subtitle
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let trimmed_subtitle = subtitle_str.trim();
    if trimmed_subtitle.len() != subtitle_str.len() {
        subtitle_str = trimmed_subtitle.to_string();
    }
    let mut artist_str = parsed_data
        .artist
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let trimmed_artist = artist_str.trim();
    if trimmed_artist.len() != artist_str.len() {
        artist_str = trimmed_artist.to_string();
    }
    let mut titletranslit_str = parsed_data
        .title_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let mut subtitletranslit_str = parsed_data
        .subtitle_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let mut artisttranslit_str = parsed_data
        .artist_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let banner_path_str = parsed_data
        .banner
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let background_path_str = parsed_data
        .background
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let cdtitle_path_str = parsed_data
        .cdtitle
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let jacket_path_str = parsed_data
        .jacket
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let music_path_str = parsed_data
        .music
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();
    let timing_format = timing_format_from_ext(extension);
    let display_bpm_str = parsed_data
        .display_bpm
        .map(|b| unescape_tag(decode_bytes(b).as_ref()).into_owned())
        .unwrap_or_default();

    if options.translate_markers {
        crate::translate::replace_markers_in_place(&mut title_str);
        crate::translate::replace_markers_in_place(&mut subtitle_str);
        crate::translate::replace_markers_in_place(&mut artist_str);
        crate::translate::replace_markers_in_place(&mut titletranslit_str);
        crate::translate::replace_markers_in_place(&mut subtitletranslit_str);
        crate::translate::replace_markers_in_place(&mut artisttranslit_str);
    }
    if artist_str.is_empty() && artisttranslit_str.trim().is_empty() {
        artist_str = "Unknown artist".to_string();
        artisttranslit_str = "Unknown artist".to_string();
    }
    let offset = parse_offset_seconds(parsed_data.offset);
    let ssc_version = parse_version(parsed_data.version, timing_format);
    let sample_start = parsed_data
        .sample_start
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let sample_length = parsed_data
        .sample_length
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");
    let normalized_global_bpms = normalize_float_digits(global_bpms_raw);
    let cleaned_global_bpms = clean_timing_map(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_stops = normalize_float_digits(global_stops_raw);
    let cleaned_global_stops = clean_timing_map(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_delays = normalize_float_digits(global_delays_raw);
    let cleaned_global_delays = clean_timing_map(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_warps = normalize_float_digits(global_warps_raw);
    let cleaned_global_warps = clean_timing_map(global_warps_raw);
    let global_speeds_raw = parsed_data
        .speeds
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_speeds = normalize_float_digits(global_speeds_raw);
    let cleaned_global_speeds = clean_timing_map(global_speeds_raw);
    let global_scrolls_raw = parsed_data
        .scrolls
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_scrolls = normalize_float_digits(global_scrolls_raw);
    let cleaned_global_scrolls = clean_timing_map(global_scrolls_raw);
    let global_fakes_raw = parsed_data
        .fakes
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_fakes = normalize_float_digits(global_fakes_raw);
    let cleaned_global_fakes = clean_timing_map(global_fakes_raw);
    let normalized_global_time_signatures = parsed_data
        .time_signatures
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_labels = parsed_data
        .labels
        .map(|b| {
            let first_param = msd_first_param_bytes(b);
            let decoded = decode_bytes(first_param);
            let unescaped = unescape_tag(decoded.as_ref());
            clean_tag(unescaped.as_ref()).into_owned()
        })
        .unwrap_or_default();
    let normalized_global_tickcounts = parsed_data
        .tickcounts
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_combos = parsed_data
        .combos
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();

    let allow_steps_timing = steps_timing_allowed(ssc_version, timing_format);
    let compiled_custom_patterns =
        if options.compute_pattern_counts && !options.custom_patterns.is_empty() {
            compile_custom_patterns(&options.custom_patterns)
        } else {
            compiled_custom_empty()
        };
    let global_timing_segments = Arc::new(compute_timing_segments(
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
    ));
    let global_bpm_map: Vec<(f64, f64)> = global_timing_segments
        .bpms
        .iter()
        .map(|(beat, bpm)| (f64::from(*beat), f64::from(*bpm)))
        .collect();
    let (min_bpm_i32, max_bpm_i32) = compute_bpm_range(&global_bpm_map);
    let bpm_values: Vec<f64> = global_bpm_map.iter().map(|&(_, bpm)| bpm).collect();
    let (median_bpm_raw, average_bpm_raw) = compute_bpm_stats(&bpm_values);
    let median_bpm = round_dp(median_bpm_raw, 2);
    let average_bpm = round_dp(average_bpm_raw, 2);

    let cleaned_global_bpms_str = cleaned_global_bpms.as_str();
    let cleaned_global_stops_str = cleaned_global_stops.as_str();
    let cleaned_global_delays_str = cleaned_global_delays.as_str();
    let cleaned_global_warps_str = cleaned_global_warps.as_str();
    let cleaned_global_speeds_str = cleaned_global_speeds.as_str();
    let cleaned_global_scrolls_str = cleaned_global_scrolls.as_str();
    let cleaned_global_fakes_str = cleaned_global_fakes.as_str();
    let normalized_global_bpms_str = normalized_global_bpms.as_str();
    let global_attacks_opt = parsed_data.attacks;

    let entries = parsed_data.notes_list;
    let entry_count = entries.len();
    let mut chart_summaries = Vec::with_capacity(entry_count);
    let mut total_length = 0i32;
    let (Some(mut parity_scratch4), Some(mut parity_scratch8)) = (
        step_parity::timing_rows_scratch::<4>(),
        step_parity::timing_rows_scratch::<8>(),
    ) else {
        return Err("Unsupported lane layout for step parity".to_string());
    };
    let options_ref = &options;
    let compiled_custom_patterns_ref = &compiled_custom_patterns;
    for entry in entries {
        if let Some((summary, chart_length)) = build_chart_summary(
            &entry,
            global_attacks_opt,
            cleaned_global_bpms_str,
            cleaned_global_stops_str,
            cleaned_global_delays_str,
            cleaned_global_warps_str,
            cleaned_global_speeds_str,
            cleaned_global_scrolls_str,
            cleaned_global_fakes_str,
            normalized_global_bpms_str,
            &global_timing_segments,
            &global_bpm_map,
            offset,
            extension,
            timing_format,
            ssc_version,
            allow_steps_timing,
            compiled_custom_patterns_ref,
            &mut parity_scratch4,
            &mut parity_scratch8,
            options_ref,
        ) {
            if chart_length > total_length {
                total_length = chart_length;
            }
            chart_summaries.push(summary);
        }
    }

    if chart_summaries.is_empty() {
        return Err("No matching steps".to_string());
    }

    let total_elapsed = total_start_time.elapsed();

    let offset_rounded = round_dp(offset, 3);
    Ok(SimfileSummary {
        title_str,
        subtitle_str,
        artist_str,
        titletranslit_str,
        subtitletranslit_str,
        artisttranslit_str,
        offset: offset_rounded,
        normalized_bpms: normalized_global_bpms,
        normalized_stops: normalized_global_stops,
        normalized_delays: normalized_global_delays,
        normalized_warps: normalized_global_warps,
        normalized_speeds: normalized_global_speeds,
        normalized_scrolls: normalized_global_scrolls,
        normalized_fakes: normalized_global_fakes,
        normalized_time_signatures: normalized_global_time_signatures,
        normalized_labels: normalized_global_labels,
        normalized_tickcounts: normalized_global_tickcounts,
        normalized_combos: normalized_global_combos,
        ssc_version,
        timing_format,
        banner_path: banner_path_str,
        background_path: background_path_str,
        cdtitle_path: cdtitle_path_str,
        jacket_path: jacket_path_str,
        music_path: music_path_str,
        display_bpm_str,
        sample_start,
        sample_length,
        min_bpm: f64::from(min_bpm_i32),
        max_bpm: f64::from(max_bpm_i32),
        median_bpm,
        average_bpm,
        total_length,
        pattern_counts_enabled: options.compute_pattern_counts,
        tech_counts_enabled: options.compute_tech_counts,
        charts: chart_summaries,
        total_elapsed,
    })
}

pub fn compute_all_hashes(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartHashInfo>, String> {
    // 1. Parse the file structure (fast, just byte slicing)
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;
    let timing_format = timing_format_from_ext(extension);
    let ssc_version = parse_version(parsed_data.version, timing_format);

    // 2. Prepare Global BPMs
    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"")).unwrap_or("");
    let normalized_global_bpms = normalize_float_digits(global_bpms_raw);

    let mut results = Vec::new();

    for entry in parsed_data.notes_list {
        // 3. Split fields to get Metadata (StepType, Difficulty)
        if entry.field_count < 5 {
            continue;
        }
        let fields = entry.fields;
        let chart_data = entry.note_data;
        let Some(lanes) = supported_stepstype_lanes_bytes(fields[0]) else {
            continue;
        };

        let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
        let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
        let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
        let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
        let meter_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
        let difficulty =
            resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        // 4. Minimize Chart (Required for Hash consistency)
        // This strips comments, whitespace, and empty measures.
        let mut minimized_chart = minimize_chart_for_hash(chart_data, lanes);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }

        // 5. Normalize BPMs (Required for Hash consistency)
        let bpms_to_use = entry.chart_bpms.as_deref().map_or(
            Cow::Borrowed(normalized_global_bpms.as_str()),
            |chart_bpms| {
                let normalized =
                    normalize_float_digits(std::str::from_utf8(chart_bpms).unwrap_or(""));
                Cow::Owned(normalized)
            },
        );

        // 6. Compute SHA-1
        let hash = compute_chart_hash(&minimized_chart, bpms_to_use.as_ref());

        results.push(ChartHashInfo {
            step_type,
            difficulty,
            hash,
        });
    }

    Ok(results)
}

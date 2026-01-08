use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Instant;

pub mod bpm;
pub mod graph;
pub mod hashing;
pub mod matrix;
pub mod notes;
pub mod parse;
pub mod patterns;
pub mod report;
pub mod stats;
pub mod step_parity;
pub mod tech;
pub mod timing;
pub mod translate;

pub const RSSP_VERSION: &str = env!("CARGO_PKG_VERSION");

// Re-export the primary data structures for library users
pub use report::{ChartSummary, SimfileSummary};
pub use step_parity::TechCounts;

use crate::bpm::*;
use crate::hashing::*;
use crate::matrix::compute_matrix_rating;
use crate::parse::*;
use crate::patterns::*;
use crate::stats::*;
use crate::tech::parse_tech_notation;
use crate::timing::{
    compute_timing_segments_cleaned,
    round_millis,
    steps_timing_allowed,
    TimingData,
    TimingFormat,
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
    pub parallel: bool,
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
            parallel: true,
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
pub struct ChartDuration {
    pub step_type: String,
    pub difficulty: String,
    pub duration_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct ChartNpsInfo {
    pub step_type: String,
    pub difficulty: String,
    pub peak_nps: f64,
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
fn chart_duration_seconds(last_beat: f64, timing: &TimingData, offsets: TimingOffsets) -> f64 {
    if last_beat <= 0.0 {
        return 0.0;
    }
    round_millis(
        timing.get_time_for_beat_f32(last_beat)
            - offsets.global_offset_seconds
            - offsets.group_offset_seconds,
    )
}

/// Normalizes common difficulty labels to a canonical form (e.g. Expert -> Challenge).
pub fn normalize_difficulty_label(raw: &str) -> String {
    old_style_difficulty_label(raw)
        .map(str::to_string)
        .unwrap_or_else(|| raw.trim().to_string())
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

pub fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "dance-double" => 8,
        _ => 4,
    }
}

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
    let title_out = if title_translit.is_empty() { title } else { title_translit };
    let subtitle_out = if subtitle_translit.is_empty() { subtitle } else { subtitle_translit };
    let artist_out = if artist_translit.is_empty() { artist } else { artist_translit };
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

fn chart_timing_tag_raw(tag: Option<&[u8]>) -> Option<String> {
    let bytes = tag?;
    let text = std::str::from_utf8(bytes).ok()?;
    let cleaned = clean_timing_map(text);
    if cleaned.is_empty() { None } else { Some(cleaned) }
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
        if b == b':' && bs_run % 2 == 0 {
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

fn parse_radar_values_str(
    raw: &str,
    split_players: bool,
) -> Option<[f32; RADAR_CATEGORY_COUNT]> {
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
) -> (HashMap<PatternVariant, u32>, (u32, u32, u32, u32)) {
    let detected_patterns = detect_default_patterns(bitmasks);
    let anchors = count_anchors(bitmasks);
    (detected_patterns, anchors)
}

/// Calculates mono (same-foot patterns) and candle stats.
fn compute_mono_and_candle_stats(
    bitmasks: &[u8],
    stats: &stats::ArrowStats,
    detected_patterns: &HashMap<PatternVariant, u32>,
    options: &AnalysisOptions,
) -> (u32, u32, u32, f64, u32, f64) {
    if stats.total_steps <= 1 {
        return (0, 0, 0, 0.0, 0, 0.0);
    }

    let (facing_left, facing_right) = count_facing_steps(bitmasks, options.mono_threshold);
    let mono_total = facing_left + facing_right;
    let mono_percent = if stats.total_steps > 0 { (mono_total as f64 / stats.total_steps as f64) * 100.0 } else { 0.0 };

    let candle_left = *detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = *detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    let candle_total = candle_left + candle_right;

    let max_candles = (stats.total_steps.saturating_sub(1)) / 2;
    let candle_percent = if max_candles > 0 {
        (candle_total as f64 / max_candles as f64) * 100.0
    } else {
        0.0
    };

    (facing_left, facing_right, mono_total, mono_percent, candle_total, candle_percent)
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
    let tier_bpm = compute_tier_bpm(measure_densities, bpm_map, 4.0);
    let matrix_rating = compute_matrix_rating(measure_densities, bpm_map);

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
    notes_data: &[u8],
    chart_bpms_opt: Option<&[u8]>,
    chart_delays_opt: Option<&[u8]>,
    chart_warps_opt: Option<&[u8]>,
    chart_stops_opt: Option<&[u8]>,
    chart_speeds_opt: Option<&[u8]>,
    chart_scrolls_opt: Option<&[u8]>,
    chart_fakes_opt: Option<&[u8]>,
    chart_time_signatures_opt: Option<&[u8]>,
    chart_labels_opt: Option<&[u8]>,
    chart_tickcounts_opt: Option<&[u8]>,
    chart_combos_opt: Option<&[u8]>,
    chart_display_bpm_opt: Option<&[u8]>,
    chart_offset_opt: Option<&[u8]>,
    chart_radar_values_opt: Option<&[u8]>,
    global_bpms_raw: &str,
    global_stops_raw: &str,
    global_delays_raw: &str,
    global_warps_raw: &str,
    global_speeds_raw: &str,
    global_scrolls_raw: &str,
    global_fakes_raw: &str,
    global_bpms_norm: &str,
    song_offset: f64,
    extension: &str,
    timing_format: TimingFormat,
    ssc_version: f32,
    allow_steps_timing: bool,
    compiled_custom_patterns: &[CompiledPattern],
    options: &AnalysisOptions,
) -> Option<(ChartSummary, i32)> {
    let chart_start_time = Instant::now();

    let (fields, chart_data) = split_notes_fields(notes_data);
    if fields.len() < 5 {
        return None;
    }

    let step_type_str = unescape_trim(decode_bytes(fields[0]).as_ref());
    if step_type_str == "lights-cabinet" {
        return None;
    }

    let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
    let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
    let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
    let rating_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
    let difficulty_str = resolve_difficulty_label(&difficulty_raw, &description, &rating_raw, extension);
    let rating_str = rating_raw;
    let is_ssc = extension.eq_ignore_ascii_case("ssc");
    let credit = if is_ssc {
        unescape_tag(decode_bytes(fields[4]).as_ref())
    } else {
        String::new()
    };
    let step_artist_str = if is_ssc { credit.clone() } else { description.clone() };
    let tech_notation_str = parse_tech_notation(&credit, &description);

    let lanes = step_type_lanes(&step_type_str);
    let compute_patterns = lanes == 4 && options.compute_pattern_counts;
    let (mut minimized_chart, mut stats, measure_densities, row_to_beat, last_beat, bitmasks) =
        if compute_patterns {
            let (chart, stats, densities, row_to_beat, last_beat, bitmasks) =
                minimize_chart_rows_bits(chart_data);
            (chart, stats, densities, row_to_beat, last_beat, Some(bitmasks))
        } else {
            let (chart, stats, densities, row_to_beat, last_beat) =
                minimize_chart_count_rows(chart_data, lanes);
            (chart, stats, densities, row_to_beat, last_beat, None)
        };
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }

    let (chart_bpms, chart_bpms_norm) = chart_timing_tag_pair(chart_bpms_opt);
    let bpms_to_use = chart_bpms_norm
        .clone()
        .unwrap_or_else(|| global_bpms_norm.to_string());
    let chart_stops = chart_timing_tag_raw(chart_stops_opt);
    let chart_speeds = chart_timing_tag_raw(chart_speeds_opt);
    let chart_delays = chart_timing_tag_raw(chart_delays_opt);
    let chart_scrolls = chart_timing_tag_raw(chart_scrolls_opt);
    let chart_warps = chart_timing_tag_raw(chart_warps_opt);
    let chart_fakes = chart_timing_tag_raw(chart_fakes_opt);

    let chart_bpms_timing = if allow_steps_timing { chart_bpms.as_deref() } else { None };
    let chart_stops_timing = if allow_steps_timing { chart_stops.as_deref() } else { None };
    let chart_delays_timing = if allow_steps_timing { chart_delays.as_deref() } else { None };
    let chart_warps_timing = if allow_steps_timing { chart_warps.as_deref() } else { None };
    let chart_speeds_timing = if allow_steps_timing { chart_speeds.as_deref() } else { None };
    let chart_scrolls_timing = if allow_steps_timing { chart_scrolls.as_deref() } else { None };
    let chart_fakes_timing = if allow_steps_timing { chart_fakes.as_deref() } else { None };
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
        let cleaned = clean_tag(&unescape_tag(decoded.as_ref()));
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
    let chart_display_bpm = chart_display_bpm_tag(chart_display_bpm_opt);
    let chart_offset = if allow_steps_timing && chart_offset_opt.is_some() {
        parse_offset_seconds(chart_offset_opt)
    } else {
        song_offset
    };
    let cached_radar_values = if extension.eq_ignore_ascii_case("sm") {
        parse_radar_values_bytes(fields.get(4).copied(), false)
    } else {
        parse_radar_values_bytes(chart_radar_values_opt, true)
    };
    let chart_has_own_timing = allow_steps_timing
        && (chart_bpms_opt.is_some()
            || chart_stops_opt.is_some()
            || chart_delays_opt.is_some()
            || chart_warps_opt.is_some()
            || chart_speeds_opt.is_some()
            || chart_scrolls_opt.is_some()
            || chart_fakes_opt.is_some()
            || chart_time_signatures_opt.is_some()
            || chart_labels_opt.is_some()
            || chart_tickcounts_opt.is_some()
            || chart_combos_opt.is_some()
            || chart_offset_opt.is_some());
    let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
        timing_speeds_global, timing_scrolls_global, timing_fakes_global) = if chart_has_own_timing {
        ("", "", "", "", "", "", "")
    } else {
        (global_bpms_raw, global_stops_raw, global_delays_raw, global_warps_raw,
            global_speeds_raw, global_scrolls_raw, global_fakes_raw)
    };
    let timing_segments = compute_timing_segments_cleaned(
        chart_bpms_timing,
        timing_bpms_global,
        chart_stops_timing,
        timing_stops_global,
        chart_delays_timing,
        timing_delays_global,
        chart_warps_timing,
        timing_warps_global,
        chart_speeds_timing,
        timing_speeds_global,
        chart_scrolls_timing,
        timing_scrolls_global,
        chart_fakes_timing,
        timing_fakes_global,
        timing_format,
    );

    let bpm_map: Vec<(f64, f64)> = timing_segments
        .bpms
        .iter()
        .map(|(beat, bpm)| (*beat as f64, *bpm as f64))
        .collect();

    let metrics =
        compute_derived_chart_metrics(&measure_densities, &bpm_map, &minimized_chart, &bpms_to_use);

    let (detected_patterns, (anchor_left, anchor_down, anchor_up, anchor_right)) =
        if let Some(bitmasks) = bitmasks.as_ref() {
            compute_pattern_and_anchor_stats(bitmasks)
        } else {
            (HashMap::new(), (0, 0, 0, 0))
        };

    let (facing_left, facing_right, mono_total, mono_percent, candle_total, candle_percent) =
        if let Some(bitmasks) = bitmasks.as_ref() {
            compute_mono_and_candle_stats(bitmasks, &stats, &detected_patterns, options)
        } else {
            (0, 0, 0, 0.0, 0, 0.0)
        };

    let custom_patterns = if compute_patterns && !compiled_custom_patterns.is_empty() {
        detect_custom_patterns_compiled(bitmasks.as_ref().unwrap(), compiled_custom_patterns)
    } else {
        Vec::new()
    };

    let timing = TimingData::from_chart_data_cleaned(
        chart_offset,
        0.0,
        chart_bpms_timing,
        timing_bpms_global,
        chart_stops_timing,
        timing_stops_global,
        chart_delays_timing,
        timing_delays_global,
        chart_warps_timing,
        timing_warps_global,
        chart_speeds_timing,
        timing_speeds_global,
        chart_scrolls_timing,
        timing_scrolls_global,
        chart_fakes_timing,
        timing_fakes_global,
        timing_format,
    );

    let duration_seconds =
        chart_duration_seconds(last_beat, &timing, TimingOffsets::default());
    let chart_length = if last_beat <= 0.0 {
        0
    } else {
        let time_chart_f64 = timing.get_time_for_beat(last_beat);
        (time_chart_f64 + (song_offset - chart_offset)).floor() as i32
    };

    let measure_nps_vec = compute_measure_nps_vec_with_timing(&measure_densities, &timing);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

    let tech_counts = if options.compute_tech_counts {
        step_parity::analyze_timing_lanes(&minimized_chart, &timing, lanes)
    } else {
        step_parity::TechCounts::default()
    };

    let raw_total_steps = stats.total_steps;
    let raw_holding = stats.holding;
    let mut timing_stats =
        compute_timing_aware_stats_with_row_to_beat(&minimized_chart, lanes, &timing, &row_to_beat);
    timing_stats.total_steps = raw_total_steps;
    timing_stats.holding = raw_holding;
    let mines_nonfake = timing_stats.mines;
    stats = timing_stats;

    let elapsed_chart = chart_start_time.elapsed();

    Some((ChartSummary {
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
    }, chart_length))
}

pub fn analyze(
    simfile_data: &[u8],
    extension: &str,
    options: AnalysisOptions,
) -> Result<SimfileSummary, String> {
    let total_start_time = Instant::now();

    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let mut title_str = parsed_data
        .title
        .map(|b| clean_tag(&unescape_tag(decode_bytes(b).as_ref())))
        .unwrap_or_else(|| "<invalid-title>".to_string());
    if options.strip_tags {
        title_str = strip_title_tags(&title_str);
    }
    let trimmed_title = title_str.trim();
    if trimmed_title.len() != title_str.len() {
        title_str = trimmed_title.to_string();
    }

    let mut subtitle_str = parsed_data
        .subtitle
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let trimmed_subtitle = subtitle_str.trim();
    if trimmed_subtitle.len() != subtitle_str.len() {
        subtitle_str = trimmed_subtitle.to_string();
    }
    let mut artist_str = parsed_data
        .artist
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let trimmed_artist = artist_str.trim();
    if trimmed_artist.len() != artist_str.len() {
        artist_str = trimmed_artist.to_string();
    }
    let mut titletranslit_str = parsed_data
        .title_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let mut subtitletranslit_str = parsed_data
        .subtitle_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let mut artisttranslit_str = parsed_data
        .artist_translit
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let banner_path_str = parsed_data
        .banner
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let background_path_str = parsed_data
        .background
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let music_path_str = parsed_data
        .music
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
        .unwrap_or_default();
    let timing_format = TimingFormat::from_extension(extension);
    let display_bpm_str = parsed_data
        .display_bpm
        .map(|b| unescape_tag(decode_bytes(b).as_ref()))
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
        let unknown = "Unknown artist".to_string();
        artist_str = unknown.clone();
        artisttranslit_str = unknown;
    }
    let offset = parse_offset_seconds(parsed_data.offset);
    let ssc_version = parse_version(parsed_data.version, timing_format);
    let sample_start = parsed_data.sample_start.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let sample_length = parsed_data.sample_length.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"<invalid-bpms>")).unwrap_or("<invalid-bpms>");
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
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let normalized_global_labels = parsed_data
        .labels
        .map(|b| {
            let first_param = msd_first_param_bytes(b);
            let decoded = decode_bytes(first_param);
            clean_tag(&unescape_tag(decoded.as_ref()))
        })
        .unwrap_or_default();
    let normalized_global_tickcounts = parsed_data
        .tickcounts
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    let normalized_global_combos = parsed_data
        .combos
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(str::trim)
        .unwrap_or("")
        .to_string();

    let allow_steps_timing = steps_timing_allowed(ssc_version, timing_format);
    let compiled_custom_patterns = if options.compute_pattern_counts && !options.custom_patterns.is_empty() {
        compile_custom_patterns(&options.custom_patterns)
    } else {
        Vec::new()
    };
    let global_timing_segments = compute_timing_segments_cleaned(
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
    );
    let global_bpm_map: Vec<(f64, f64)> = global_timing_segments
        .bpms
        .iter()
        .map(|(beat, bpm)| (*beat as f64, *bpm as f64))
        .collect();
    let (min_bpm_i32, max_bpm_i32) = compute_bpm_range(&global_bpm_map);
    let bpm_values: Vec<f64> = global_bpm_map.iter().map(|&(_, bpm)| bpm).collect();
    let (median_bpm, average_bpm) = compute_bpm_stats(&bpm_values);

    let cleaned_global_bpms_str = cleaned_global_bpms.as_str();
    let cleaned_global_stops_str = cleaned_global_stops.as_str();
    let cleaned_global_delays_str = cleaned_global_delays.as_str();
    let cleaned_global_warps_str = cleaned_global_warps.as_str();
    let cleaned_global_speeds_str = cleaned_global_speeds.as_str();
    let cleaned_global_scrolls_str = cleaned_global_scrolls.as_str();
    let cleaned_global_fakes_str = cleaned_global_fakes.as_str();
    let normalized_global_bpms_str = normalized_global_bpms.as_str();

    let entries = parsed_data.notes_list;
    let entry_count = entries.len();
    let mut chart_summaries = Vec::with_capacity(entry_count);
    let mut total_length = 0i32;
    let options_ref = &options;
    let compiled_custom_patterns_ref = &compiled_custom_patterns;
    let allow_parallel = options.parallel
        && entry_count > 1
        && std::thread::available_parallelism()
            .map(|count| count.get())
            .unwrap_or(1)
            > 1;

    if allow_parallel {
        let mut results = Vec::with_capacity(entry_count);
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(entry_count);
            for entry in entries {
                handles.push(scope.spawn(move || {
                    build_chart_summary(
                        &entry.notes,
                        entry.chart_bpms.as_deref(),
                        entry.chart_delays.as_deref(),
                        entry.chart_warps.as_deref(),
                        entry.chart_stops.as_deref(),
                        entry.chart_speeds.as_deref(),
                        entry.chart_scrolls.as_deref(),
                        entry.chart_fakes.as_deref(),
                        entry.chart_time_signatures.as_deref(),
                        entry.chart_labels.as_deref(),
                        entry.chart_tickcounts.as_deref(),
                        entry.chart_combos.as_deref(),
                        entry.chart_display_bpm.as_deref(),
                        entry.chart_offset.as_deref(),
                        entry.chart_radar_values.as_deref(),
                        cleaned_global_bpms_str,
                        cleaned_global_stops_str,
                        cleaned_global_delays_str,
                        cleaned_global_warps_str,
                        cleaned_global_speeds_str,
                        cleaned_global_scrolls_str,
                        cleaned_global_fakes_str,
                        normalized_global_bpms_str,
                        offset,
                        extension,
                        timing_format,
                        ssc_version,
                        allow_steps_timing,
                        compiled_custom_patterns_ref,
                        options_ref,
                    )
                }));
            }
            for handle in handles {
                results.push(handle.join().unwrap());
            }
        });

        for result in results {
            if let Some((summary, chart_length)) = result {
                if chart_length > total_length {
                    total_length = chart_length;
                }
                chart_summaries.push(summary);
            }
        }
    } else {
        for entry in entries {
            if let Some((summary, chart_length)) = build_chart_summary(
                &entry.notes,
                entry.chart_bpms.as_deref(),
                entry.chart_delays.as_deref(),
                entry.chart_warps.as_deref(),
                entry.chart_stops.as_deref(),
                entry.chart_speeds.as_deref(),
                entry.chart_scrolls.as_deref(),
                entry.chart_fakes.as_deref(),
                entry.chart_time_signatures.as_deref(),
                entry.chart_labels.as_deref(),
                entry.chart_tickcounts.as_deref(),
                entry.chart_combos.as_deref(),
                entry.chart_display_bpm.as_deref(),
                entry.chart_offset.as_deref(),
                entry.chart_radar_values.as_deref(),
                cleaned_global_bpms_str,
                cleaned_global_stops_str,
                cleaned_global_delays_str,
                cleaned_global_warps_str,
                cleaned_global_speeds_str,
                cleaned_global_scrolls_str,
                cleaned_global_fakes_str,
                normalized_global_bpms_str,
                offset,
                extension,
                timing_format,
                ssc_version,
                allow_steps_timing,
                compiled_custom_patterns_ref,
                options_ref,
            ) {
                if chart_length > total_length {
                    total_length = chart_length;
                }
                chart_summaries.push(summary);
            }
        }
    }

    let total_elapsed = total_start_time.elapsed();

    Ok(SimfileSummary {
        title_str, subtitle_str, artist_str, titletranslit_str, subtitletranslit_str,
        artisttranslit_str, offset, normalized_bpms: normalized_global_bpms,
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
        music_path: music_path_str,
        display_bpm_str,
        sample_start, sample_length,
        min_bpm: min_bpm_i32 as f64, max_bpm: max_bpm_i32 as f64,
        median_bpm, average_bpm, total_length,
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
    let timing_format = TimingFormat::from_extension(extension);
    let ssc_version = parse_version(parsed_data.version, timing_format);

    // 2. Prepare Global BPMs
    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"")).unwrap_or("");
    let normalized_global_bpms = normalize_float_digits(global_bpms_raw);

    let mut results = Vec::new();

    for entry in parsed_data.notes_list {
        // 3. Split fields to get Metadata (StepType, Difficulty)
        let (fields, chart_data) = split_notes_fields(&entry.notes);
        if fields.len() < 5 {
            continue;
        }

        let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
        let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
        let description = normalize_chart_desc(description_raw, timing_format, ssc_version);
        let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
        let meter_raw = unescape_trim(decode_bytes(fields[3]).as_ref());
        let difficulty = resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        // Skip lights, etc.
        if step_type == "lights-cabinet" {
            continue;
        }

        // 4. Minimize Chart (Required for Hash consistency)
        // This strips comments, whitespace, and empty measures.
        let lanes = step_type_lanes(&step_type);
        let mut minimized_chart = minimize_chart_for_hash(chart_data, lanes);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }

        // 5. Normalize BPMs (Required for Hash consistency)
        let bpms_to_use = if let Some(chart_bpms) = entry.chart_bpms.as_deref() {
            let normalized = normalize_float_digits(std::str::from_utf8(chart_bpms).unwrap_or(""));
            Cow::Owned(normalized)
        } else {
            Cow::Borrowed(normalized_global_bpms.as_str())
        };

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
        let difficulty = resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        let lanes = step_type_lanes(&step_type);
        let (_, _, _, _, last_beat) = minimize_chart_count_rows(chart_data, lanes);

        let chart_offset = if allow_steps_timing && entry.chart_offset.is_some() {
            parse_offset_seconds(entry.chart_offset.as_deref())
        } else {
            song_offset
        };
        let chart_bpms = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_bpms.as_deref())
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_stops.as_deref())
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_delays.as_deref())
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_warps.as_deref())
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_speeds.as_deref())
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_scrolls.as_deref())
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_fakes.as_deref())
        } else {
            None
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
        let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
            timing_speeds_global, timing_scrolls_global, timing_fakes_global) =
            if chart_has_own_timing {
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

        let timing = TimingData::from_chart_data_cleaned(
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
        let difficulty = resolve_difficulty_label(&difficulty_raw, &description, &meter_raw, extension);

        let lanes = step_type_lanes(&step_type);
        let measure_densities = stats::measure_densities(chart_data, lanes);

        let chart_bpms = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_bpms.as_deref())
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_stops.as_deref())
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_delays.as_deref())
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_warps.as_deref())
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_speeds.as_deref())
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_scrolls.as_deref())
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_fakes.as_deref())
        } else {
            None
        };
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
        let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
            timing_speeds_global, timing_scrolls_global, timing_fakes_global) =
            if chart_has_own_timing {
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

        let timing = TimingData::from_chart_data_cleaned(
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

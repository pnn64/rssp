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
use crate::tech::parse_step_artist_and_tech;
use crate::timing::{
    compute_row_to_beat,
    compute_timing_segments,
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
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            strip_tags: false,
            mono_threshold: 0,
            custom_patterns: Vec::new(),
            compute_tech_counts: true,
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

pub(crate) fn resolve_difficulty_label(
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

fn chart_timing_tag_pair(tag: Option<Vec<u8>>) -> (Option<String>, Option<String>) {
    let Some(bytes) = tag else {
        return (None, None);
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return (None, None);
    };
    let raw = clean_timing_map(text);
    let norm = normalize_float_digits(text);
    let raw = if raw.is_empty() { None } else { Some(raw) };
    let norm = if norm.is_empty() { None } else { Some(norm) };
    (raw, norm)
}

fn chart_timing_tag_raw(tag: Option<Vec<u8>>) -> Option<String> {
    let bytes = tag?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let cleaned = clean_timing_map(text);
    if cleaned.is_empty() { None } else { Some(cleaned) }
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
    let cleaned = clean_timing_map(raw);
    if cleaned.is_empty() {
        return None;
    }

    let mut values = Vec::new();
    for part in cleaned.split(',') {
        if part.is_empty() {
            continue;
        }
        let Ok(value) = part.trim().parse::<f32>() else {
            continue;
        };
        values.push(value);
    }

    let slice = if split_players {
        let per_player = values.len() / 2;
        if per_player < RADAR_CATEGORY_COUNT {
            return None;
        }
        &values[..per_player]
    } else {
        if values.len() < RADAR_CATEGORY_COUNT {
            return None;
        }
        &values[..]
    };

    let mut out = [0.0f32; RADAR_CATEGORY_COUNT];
    out.copy_from_slice(&slice[..RADAR_CATEGORY_COUNT]);
    if out
        .iter()
        .skip(RADAR_CATEGORY_NOTES)
        .any(|v| !v.is_finite() || *v < 0.0)
    {
        return None;
    }

    Some(out)
}

/// Parses the minimized chart data string into a sequence of note bitmasks.
fn generate_bitmasks(minimized_chart: &[u8]) -> Vec<u8> {
    minimized_chart
        .split(|&b| b == b'\n')
        .filter_map(|line| {
            // A line must have at least 4 characters to be a valid note line.
            // Also, filter out lines that are just measure separators (',') or empty.
            if line.len() < 4 || line.iter().all(|&b| b == b' ' || b == b',') {
                return None;
            }

            let mut mask = 0u8;
            // The first 4 bytes represent the 4 arrow panels.
            for i in 0..4 {
                // A step can be a tap ('1'), a hold start ('2'), or a roll start ('4').
                if matches!(line[i], b'1' | b'2' | b'4') {
                    mask |= 1 << i;
                }
            }
            Some(mask)
        })
        .collect()
}

/// Detects predefined patterns and counts anchors from note bitmasks.
fn compute_pattern_and_anchor_stats(
    bitmasks: &[u8],
) -> (HashMap<PatternVariant, u32>, (u32, u32, u32, u32)) {
    let patterns_to_detect: Vec<_> =
        DEFAULT_PATTERNS.iter().chain(EXTRA_PATTERNS.iter()).cloned().collect();
    let detected_patterns = detect_patterns(bitmasks, &patterns_to_detect);
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
    measure_nps_vec: Vec<f64>,
    max_nps: f64,
    median_nps: f64,
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

    let measure_nps_vec = compute_measure_nps_vec(measure_densities, bpm_map);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

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
        measure_nps_vec,
        max_nps,
        median_nps,
        short_hash,
        bpm_neutral_hash,
        tier_bpm,
        matrix_rating,
    }
}

/// Processes a single chart's data to produce a `ChartSummary`.
fn build_chart_summary(
    notes_data: Vec<u8>,
    chart_bpms_opt: Option<Vec<u8>>,
    chart_delays_opt: Option<Vec<u8>>,
    chart_warps_opt: Option<Vec<u8>>,
    chart_stops_opt: Option<Vec<u8>>,
    chart_speeds_opt: Option<Vec<u8>>,
    chart_scrolls_opt: Option<Vec<u8>>,
    chart_fakes_opt: Option<Vec<u8>>,
    chart_time_signatures_opt: Option<Vec<u8>>,
    chart_labels_opt: Option<Vec<u8>>,
    chart_tickcounts_opt: Option<Vec<u8>>,
    chart_combos_opt: Option<Vec<u8>>,
    chart_radar_values_opt: Option<Vec<u8>>,
    global_bpms_raw: &str,
    global_stops_raw: &str,
    global_delays_raw: &str,
    global_warps_raw: &str,
    global_speeds_raw: &str,
    global_scrolls_raw: &str,
    global_fakes_raw: &str,
    global_bpms_norm: &str,
    extension: &str,
    timing_format: TimingFormat,
    allow_steps_timing: bool,
    options: &AnalysisOptions,
) -> Option<ChartSummary> {
    let chart_start_time = Instant::now();

    let (fields, chart_data) = split_notes_fields(&notes_data);
    if fields.len() < 5 {
        return None;
    }

    let step_type_str = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_owned();
    if step_type_str == "lights-cabinet" {
        return None;
    }

    let description = std::str::from_utf8(fields[1]).unwrap_or("").trim().to_owned();
    let difficulty_raw = std::str::from_utf8(fields[2]).unwrap_or("").trim();
    let rating_raw = std::str::from_utf8(fields[3]).unwrap_or("").trim();
    let difficulty_str = resolve_difficulty_label(difficulty_raw, &description, rating_raw, extension);
    let rating_str = rating_raw.to_owned();
    let credit = if extension.eq_ignore_ascii_case("ssc") {
        std::str::from_utf8(fields[4]).unwrap_or("").trim().to_owned()
    } else {
        String::new()
    };
    let (step_artist_str, tech_notation_str) = parse_step_artist_and_tech(&credit, &description);

    let lanes = step_type_lanes(&step_type_str);
    let (mut minimized_chart, stats, measure_densities) =
        minimize_chart_and_count_with_lanes(chart_data, lanes);
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }
    let row_to_beat = compute_row_to_beat(&minimized_chart);

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
        std::str::from_utf8(&bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let chart_labels = chart_labels_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(|s| clean_tag(&unescape_tag(s)))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });
    let chart_tickcounts = chart_tickcounts_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let chart_combos = chart_combos_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    });
    let cached_radar_values = if extension.eq_ignore_ascii_case("sm") {
        parse_radar_values_bytes(fields.get(4).copied(), false)
    } else {
        parse_radar_values_bytes(chart_radar_values_opt.as_deref(), true)
    };
    let chart_has_timing = allow_steps_timing
        && (chart_bpms.is_some()
            || chart_stops.is_some()
            || chart_delays.is_some()
            || chart_warps.is_some()
            || chart_speeds.is_some()
            || chart_scrolls.is_some()
            || chart_fakes.is_some());
    let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
        timing_speeds_global, timing_scrolls_global, timing_fakes_global) = if chart_has_timing {
        ("", "", "", "", "", "", "")
    } else {
        (global_bpms_raw, global_stops_raw, global_delays_raw, global_warps_raw,
            global_speeds_raw, global_scrolls_raw, global_fakes_raw)
    };
    let timing_segments = compute_timing_segments(
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

    let bitmasks = if lanes == 4 {
        Some(generate_bitmasks(&minimized_chart))
    } else {
        None
    };

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

    let custom_patterns = if lanes == 4 && !options.custom_patterns.is_empty() {
        detect_custom_patterns(bitmasks.as_ref().unwrap(), &options.custom_patterns)
    } else {
        Vec::new()
    };

    let tech_counts = step_parity::TechCounts::default();

    let elapsed_chart = chart_start_time.elapsed();

    Some(ChartSummary {
        step_type_str,
        step_artist_str,
        difficulty_str,
        rating_str,
        tech_notation_str,
        tier_bpm: metrics.tier_bpm,
        matrix_rating: metrics.matrix_rating,
        stats,
        stream_counts: metrics.stream_counts,
        total_streams: metrics.total_streams,
        mines_nonfake: 0,
        total_measures: measure_densities.len(),
        sn_detailed_breakdown: metrics.sn_detailed_breakdown,
        sn_partial_breakdown: metrics.sn_partial_breakdown,
        sn_simple_breakdown: metrics.sn_simple_breakdown,
        max_nps: metrics.max_nps,
        median_nps: metrics.median_nps,
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
        measure_nps_vec: metrics.measure_nps_vec,
        row_to_beat,
        timing_segments,
        minimized_note_data: minimized_chart,
        chart_stops,
        chart_speeds,
        chart_scrolls,
        chart_bpms,
        chart_delays,
        chart_warps,
        chart_fakes,
        chart_time_signatures,
        chart_labels,
        chart_tickcounts,
        chart_combos,
        cached_radar_values,
    })
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
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(|tag| clean_tag(&unescape_tag(tag)))
        .unwrap_or_else(|| "<invalid-title>".to_string());
    if options.strip_tags {
        title_str = strip_title_tags(&title_str);
    }

    let subtitle_str = parsed_data.subtitle.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let artist_str = parsed_data.artist.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let titletranslit_str = parsed_data.title_translit.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let subtitletranslit_str = parsed_data.subtitle_translit.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let artisttranslit_str = parsed_data.artist_translit.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let banner_path_str = parsed_data.banner.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let background_path_str = parsed_data.background.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let music_path_str = parsed_data.music.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let display_bpm_str = parsed_data.display_bpm.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let timing_format = TimingFormat::from_extension(extension);
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
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(|s| clean_tag(&unescape_tag(s)))
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
    let global_timing_segments = compute_timing_segments(
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

    let mut chart_summaries: Vec<ChartSummary> = parsed_data
        .notes_list
        .into_iter()
        .filter_map(|entry| {
            build_chart_summary(
                entry.notes,
                entry.chart_bpms,
                entry.chart_delays,
                entry.chart_warps,
                entry.chart_stops,
                entry.chart_speeds,
                entry.chart_scrolls,
                entry.chart_fakes,
                entry.chart_time_signatures,
                entry.chart_labels,
                entry.chart_tickcounts,
                entry.chart_combos,
                entry.chart_radar_values,
                &cleaned_global_bpms,
                &cleaned_global_stops,
                &cleaned_global_delays,
                &cleaned_global_warps,
                &cleaned_global_speeds,
                &cleaned_global_scrolls,
                &cleaned_global_fakes,
                &normalized_global_bpms,
                extension,
                timing_format,
                allow_steps_timing,
                &options,
            )
        })
        .collect();

    let total_length = chart_summaries
        .iter_mut()
        .map(|chart| {
            let chart_bpms_timing = if allow_steps_timing {
                chart.chart_bpms.as_deref()
            } else {
                None
            };
            let chart_stops_timing = if allow_steps_timing {
                chart.chart_stops.as_deref()
            } else {
                None
            };
            let chart_delays_timing = if allow_steps_timing {
                chart.chart_delays.as_deref()
            } else {
                None
            };
            let chart_warps_timing = if allow_steps_timing {
                chart.chart_warps.as_deref()
            } else {
                None
            };
            let chart_speeds_timing = if allow_steps_timing {
                chart.chart_speeds.as_deref()
            } else {
                None
            };
            let chart_scrolls_timing = if allow_steps_timing {
                chart.chart_scrolls.as_deref()
            } else {
                None
            };
            let chart_fakes_timing = if allow_steps_timing {
                chart.chart_fakes.as_deref()
            } else {
                None
            };

            let chart_has_timing = allow_steps_timing
                && (chart.chart_bpms.is_some()
                    || chart.chart_stops.is_some()
                    || chart.chart_delays.is_some()
                    || chart.chart_warps.is_some()
                    || chart.chart_speeds.is_some()
                    || chart.chart_scrolls.is_some()
                    || chart.chart_fakes.is_some());
            let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
                timing_speeds_global, timing_scrolls_global, timing_fakes_global) =
                if chart_has_timing {
                    ("", "", "", "", "", "", "")
                } else {
                    (cleaned_global_bpms.as_str(), cleaned_global_stops.as_str(),
                        cleaned_global_delays.as_str(), cleaned_global_warps.as_str(),
                        cleaned_global_speeds.as_str(), cleaned_global_scrolls.as_str(),
                        cleaned_global_fakes.as_str())
                };

            let timing = TimingData::from_chart_data(
                offset,
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
            let lanes = step_type_lanes(&chart.step_type_str);

            let measure_nps_vec =
                compute_measure_nps_vec_with_timing(&chart.measure_densities, &timing);
            let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);
            chart.measure_nps_vec = measure_nps_vec;
            chart.max_nps = max_nps;
            chart.median_nps = median_nps;

            if options.compute_tech_counts {
                chart.tech_counts =
                    step_parity::analyze_timing_lanes(&chart.minimized_note_data, &timing, lanes);
            }

            let timing_stats = compute_timing_aware_stats(&chart.minimized_note_data, lanes, &timing);
            let total_steps = chart.stats.total_steps;
            let holding = chart.stats.holding;
            chart.stats = timing_stats;
            chart.stats.total_steps = total_steps;
            chart.stats.holding = holding;
            chart.mines_nonfake = chart.stats.mines;

            let last_beat = compute_last_beat(&chart.minimized_note_data, lanes);
            if last_beat <= 0.0 {
                0
            } else {
                timing.get_time_for_beat(last_beat).floor() as i32
            }
        })
        .max()
        .unwrap_or(0);

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
        median_bpm, average_bpm, total_length, charts: chart_summaries, total_elapsed,
    })
}

pub fn compute_all_hashes(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartHashInfo>, String> {
    // 1. Parse the file structure (fast, just byte slicing)
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

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

        let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_string();
        let description = std::str::from_utf8(fields[1]).unwrap_or("").trim();
        let difficulty_raw = std::str::from_utf8(fields[2]).unwrap_or("").trim();
        let meter_raw = std::str::from_utf8(fields[3]).unwrap_or("").trim();
        let difficulty = resolve_difficulty_label(difficulty_raw, description, meter_raw, extension);

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
        let bpms_to_use = if let Some(chart_bpms) = entry.chart_bpms {
            let normalized = normalize_float_digits(std::str::from_utf8(&chart_bpms).unwrap_or(""));
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

        let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_string();
        if step_type == "lights-cabinet" {
            continue;
        }
        let description = std::str::from_utf8(fields[1]).unwrap_or("").trim();
        let difficulty_raw = std::str::from_utf8(fields[2]).unwrap_or("").trim();
        let meter_raw = std::str::from_utf8(fields[3]).unwrap_or("").trim();
        let difficulty = resolve_difficulty_label(difficulty_raw, description, meter_raw, extension);

        let lanes = step_type_lanes(&step_type);
        let target_beat = compute_last_beat_from_chart_data(chart_data, lanes);

        let chart_offset = if allow_steps_timing && entry.chart_offset.is_some() {
            parse_offset_seconds(entry.chart_offset.as_deref())
        } else {
            song_offset
        };
        let chart_bpms = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_bpms)
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_stops)
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_delays)
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_warps)
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_speeds)
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_scrolls)
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_fakes)
        } else {
            None
        };

        let chart_has_timing = allow_steps_timing
            && (chart_bpms.is_some()
                || chart_stops.is_some()
                || chart_delays.is_some()
                || chart_warps.is_some()
                || chart_speeds.is_some()
                || chart_scrolls.is_some()
                || chart_fakes.is_some());
        let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
            timing_speeds_global, timing_scrolls_global, timing_fakes_global) =
            if chart_has_timing {
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
        );
        let duration = timing.get_time_for_beat_f32(target_beat)
            - offsets.global_offset_seconds
            - offsets.group_offset_seconds;

        results.push(ChartDuration {
            step_type,
            difficulty,
            duration_seconds: round_millis(duration),
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

        let step_type = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_string();
        if step_type == "lights-cabinet" {
            continue;
        }
        let description = std::str::from_utf8(fields[1]).unwrap_or("").trim();
        let difficulty_raw = std::str::from_utf8(fields[2]).unwrap_or("").trim();
        let meter_raw = std::str::from_utf8(fields[3]).unwrap_or("").trim();
        let difficulty = resolve_difficulty_label(difficulty_raw, description, meter_raw, extension);

        let lanes = step_type_lanes(&step_type);
        let measure_densities = stats::measure_densities(chart_data, lanes);

        let chart_bpms = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_bpms)
        } else {
            None
        };
        let chart_stops = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_stops)
        } else {
            None
        };
        let chart_delays = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_delays)
        } else {
            None
        };
        let chart_warps = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_warps)
        } else {
            None
        };
        let chart_speeds = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_speeds)
        } else {
            None
        };
        let chart_scrolls = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_scrolls)
        } else {
            None
        };
        let chart_fakes = if allow_steps_timing {
            chart_timing_tag_raw(entry.chart_fakes)
        } else {
            None
        };
        let chart_offset = if allow_steps_timing && entry.chart_offset.is_some() {
            parse_offset_seconds(entry.chart_offset.as_deref())
        } else {
            song_offset
        };

        let chart_has_timing = allow_steps_timing
            && (chart_bpms.is_some()
                || chart_stops.is_some()
                || chart_delays.is_some()
                || chart_warps.is_some()
                || chart_speeds.is_some()
                || chart_scrolls.is_some()
                || chart_fakes.is_some());
        let (timing_bpms_global, timing_stops_global, timing_delays_global, timing_warps_global,
            timing_speeds_global, timing_scrolls_global, timing_fakes_global) =
            if chart_has_timing {
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

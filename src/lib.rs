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
use crate::timing::{TimingData, compute_row_to_beat};

/// Options for controlling simfile analysis.
#[derive(Debug, Default, Clone)]
pub struct AnalysisOptions {
    pub strip_tags: bool,
    pub mono_threshold: usize,
    pub custom_patterns: Vec<String>,
}

#[derive(Debug)]
pub struct ChartHashInfo {
    pub step_type: String,
    pub difficulty: String,
    pub hash: String,
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

fn resolve_difficulty_label(
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

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "dance-double" => 8,
        _ => 4,
    }
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

/// Determines the correct BPM map to use (chart-specific or global) and parses it.
fn prepare_bpm_map(
    chart_bpms_opt: Option<Vec<u8>>,
    normalized_global_bpms: &str,
) -> (String, Vec<(f64, f64)>) {
    let bpms_to_use = if let Some(chart_bpms) = chart_bpms_opt {
        normalize_float_digits(std::str::from_utf8(&chart_bpms).unwrap_or(""))
    } else {
        normalized_global_bpms.to_string()
    };
    let bpm_map = parse_bpm_map(&normalize_and_tidy_bpms(&bpms_to_use));
    (bpms_to_use, bpm_map)
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
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
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

    let detailed_breakdown = generate_breakdown(measure_densities, BreakdownMode::Detailed);
    let partial_breakdown = generate_breakdown(measure_densities, BreakdownMode::Partial);
    let simple_breakdown = generate_breakdown(measure_densities, BreakdownMode::Simplified);

    let measure_nps_vec = compute_measure_nps_vec(measure_densities, bpm_map);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

    let short_hash = compute_chart_hash(minimized_chart, bpms_to_use);
    let bpm_neutral_hash = compute_chart_hash(minimized_chart, "0.000=0.000");
    let tier_bpm = compute_tier_bpm(measure_densities, bpm_map, 4.0);
    let matrix_rating = compute_matrix_rating(measure_densities, bpm_map);

    DerivedChartMetrics {
        stream_counts,
        total_streams,
        detailed_breakdown,
        partial_breakdown,
        simple_breakdown,
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
    normalized_global_bpms: &str,
    extension: &str,
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

    let (bpms_to_use, bpm_map) = prepare_bpm_map(chart_bpms_opt.clone(), normalized_global_bpms);
    let chart_stops = chart_stops_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_speeds = chart_speeds_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_delays = chart_delays_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_scrolls = chart_scrolls_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_bpms = chart_bpms_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_warps = chart_warps_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
    let chart_fakes = chart_fakes_opt.and_then(|bytes| {
        std::str::from_utf8(&bytes)
            .ok()
            .map(normalize_float_digits)
            .filter(|s| !s.is_empty())
    });
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

    let metrics =
        compute_derived_chart_metrics(&measure_densities, &bpm_map, &minimized_chart, &bpms_to_use);

    let (detected_patterns, (anchor_left, anchor_down, anchor_up, anchor_right)) = if lanes == 4 {
        let bitmasks = generate_bitmasks(&minimized_chart);
        compute_pattern_and_anchor_stats(&bitmasks)
    } else {
        (HashMap::new(), (0, 0, 0, 0))
    };

    let (facing_left, facing_right, mono_total, mono_percent, candle_total, candle_percent) =
        if lanes == 4 {
            let bitmasks = generate_bitmasks(&minimized_chart);
            compute_mono_and_candle_stats(&bitmasks, &stats, &detected_patterns, options)
        } else {
            (0, 0, 0, 0.0, 0, 0.0)
        };

    let custom_patterns = if lanes == 4 && !options.custom_patterns.is_empty() {
        let bitmasks = generate_bitmasks(&minimized_chart);
        detect_custom_patterns(&bitmasks, &options.custom_patterns)
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
        detailed: metrics.detailed_breakdown,
        partial: metrics.partial_breakdown,
        simple: metrics.simple_breakdown,
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
    let offset = parsed_data.offset.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).map(|f| (f * 1000.0).trunc() / 1000.0).unwrap_or(0.0);
    let sample_start = parsed_data.sample_start.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let sample_length = parsed_data.sample_length.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let global_bpms_raw = std::str::from_utf8(parsed_data.bpms.unwrap_or(b"<invalid-bpms>")).unwrap_or("<invalid-bpms>");
    let normalized_global_bpms = normalize_float_digits(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_stops = normalize_float_digits(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_delays = normalize_float_digits(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_warps = normalize_float_digits(global_warps_raw);
    let global_speeds_raw = parsed_data
        .speeds
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_speeds = normalize_float_digits(global_speeds_raw);
    let global_scrolls_raw = parsed_data
        .scrolls
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_scrolls = normalize_float_digits(global_scrolls_raw);
    let global_fakes_raw = parsed_data
        .fakes
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let normalized_global_fakes = normalize_float_digits(global_fakes_raw);
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

    let global_bpm_map = parse_bpm_map(&normalize_and_tidy_bpms(&normalized_global_bpms));
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
                &normalized_global_bpms,
                extension,
                &options,
            )
        })
        .collect();

    let total_length = chart_summaries
        .iter_mut()
        .map(|chart| {
            let timing = TimingData::from_chart_data(
                offset,
                0.0,
                chart.chart_bpms.as_deref(),
                &normalized_global_bpms,
                chart.chart_stops.as_deref(),
                &normalized_global_stops,
                chart.chart_delays.as_deref(),
                &normalized_global_delays,
                chart.chart_warps.as_deref(),
                &normalized_global_warps,
                chart.chart_speeds.as_deref(),
                &normalized_global_speeds,
                chart.chart_scrolls.as_deref(),
                &normalized_global_scrolls,
                chart.chart_fakes.as_deref(),
                &normalized_global_fakes,
            );
            let lanes = step_type_lanes(&chart.step_type_str);

            if lanes == 4 {
                chart.tech_counts =
                    step_parity::analyze_with_timing(&chart.minimized_note_data, &timing);
            }

            let warp_map: Vec<(f64, f64)> =
                timing.warps().iter().map(|seg| (seg.beat, seg.length)).collect();
            let fake_map: Vec<(f64, f64)> =
                timing.fakes().iter().map(|seg| (seg.beat, seg.length)).collect();
            chart.mines_nonfake = compute_mines_nonfake(
                &chart.minimized_note_data,
                lanes,
                &warp_map,
                &fake_map,
            );

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

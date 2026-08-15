use std::borrow::Cow;
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::duration::{self, TimingOffsets};
use crate::report::{ChartSummary, SimfileSummary};
use crate::stats;
use crate::step_parity;

use crate::bpm::{
    clean_and_normalize_float_digits, clean_and_normalize_speeds_float_digits,
    clean_timing_map_cow, compute_bpm_range_and_stats, compute_bpm_range_and_stats_with_scratch,
    compute_measure_nps_vec_with_timing, compute_tier_bpm, get_nps_stats_with_scratch,
    normalize_float_digits,
};
use crate::hash::compute_chart_hash_pair;
use crate::math::{round_dp, round_sig_figs_6};
use crate::matrix::{MatrixProfile, compute_matrix_profile};
use crate::parse::{
    ParsedChartEntry, SSC_VERSION_CHART_NAME_TAG, decode_bytes, decode_unescape_trim,
    extract_sections, normalize_chart_desc_ref, parse_offset_seconds, parse_version,
    strip_title_tags, unescape_tag,
};
use crate::patterns::{
    CompiledCustomPatterns, PATTERN_COUNT, PatternCounts, PatternVariant,
    analyze_patterns_from_rows, compile_custom_patterns, compiled_custom_empty,
};
use crate::stats::{
    RADAR_CATEGORY_COUNT, StreamCounts, compute_stream_outputs_with_scratch,
    compute_timing_aware_stats_from_rows_with_row_to_beat,
    compute_timing_aware_stats_no_holds_from_rows, compute_timing_aware_stats_with_row_to_beat,
    minimize_chart_count_rows, minimize_rows_typed_in,
};
use crate::tech::parse_tech_notation;
use crate::timing::{
    TimingData, TimingFormat, TimingSegments, compute_timing_segments, get_time_for_beat,
    has_nonjudgable_rows, steps_timing_allowed, timing_data_from_segments, timing_format_from_ext,
};
use crate::{chart_timing_tag_raw, resolve_difficulty_label, supported_stepstype_lanes_bytes};

/// Options for controlling simfile analysis.
#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub strip_tags: bool,
    pub mono_threshold: usize,
    pub custom_patterns: Vec<String>,
    pub compute_tech_counts: bool,
    pub compute_note_annotations: bool,
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
            compute_note_annotations: false,
            compute_pattern_counts: true,
            translate_markers: false,
        }
    }
}

/// Reusable temporary storage for repeated simfile analysis.
///
/// One workspace is single-thread-only and may be reused sequentially across
/// any mix of 4-lane and 8-lane simfiles. It retains the largest parity, BPM,
/// NPS, typed-row, and stream-token buffers it has needed; drop it to release
/// that memory.
#[derive(Default)]
pub struct AnalysisScratch {
    parity4: Option<step_parity::TimingRowsScratch<4>>,
    parity8: Option<step_parity::TimingRowsScratch<8>>,
    global_bpm_map: Vec<(f64, f64)>,
    chart_bpm_map: Vec<(f64, f64)>,
    bpm_values: Vec<f64>,
    nps: Vec<f64>,
    rows4: stats::TypedRowsScratch<4>,
    rows8: stats::TypedRowsScratch<8>,
    stream_tokens: Vec<stats::Token>,
}

impl std::fmt::Debug for AnalysisScratch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AnalysisScratch")
            .field("has_parity4", &self.parity4.is_some())
            .field("has_parity8", &self.parity8.is_some())
            .field("global_bpm_capacity", &self.global_bpm_map.capacity())
            .field("chart_bpm_capacity", &self.chart_bpm_map.capacity())
            .field("bpm_value_capacity", &self.bpm_values.capacity())
            .field("nps_capacity", &self.nps.capacity())
            .field("rows4_capacity", &self.rows4.row_capacity())
            .field("rows8_capacity", &self.rows8.row_capacity())
            .field("stream_capacity", &self.stream_tokens.capacity())
            .finish()
    }
}

/// Analysis options with reusable custom-pattern automata.
///
/// Construct this once for a batch of simfiles. Unlike [`AnalysisOptions`],
/// the custom pattern list is compiled during construction rather than once
/// per call to [`analyze`].
#[derive(Debug)]
pub struct PreparedAnalysis {
    options: AnalysisOptions,
    custom_patterns: CompiledCustomPatterns,
}

impl PreparedAnalysis {
    /// Compiles reusable data for `options`.
    #[must_use]
    pub fn new(options: AnalysisOptions) -> Self {
        let custom_patterns =
            if options.compute_pattern_counts && !options.custom_patterns.is_empty() {
                compile_custom_patterns(&options.custom_patterns)
            } else {
                compiled_custom_empty()
            };
        Self {
            options,
            custom_patterns,
        }
    }

    /// Returns the options represented by this prepared analysis.
    #[must_use]
    pub const fn options(&self) -> &AnalysisOptions {
        &self.options
    }
}

#[derive(Debug)]
pub struct ChartHashInfo {
    pub step_type: String,
    pub difficulty: String,
    pub hash: String,
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
    let (raw, norm) = clean_and_normalize_float_digits(text);
    let raw = if raw.is_empty() { None } else { Some(raw) };
    let norm = if norm.is_empty() { None } else { Some(norm) };
    (raw, norm)
}

fn chart_display_bpm_tag(tag: Option<&[u8]>) -> Option<String> {
    let bytes = tag?;
    let value = decode_trim_owned(bytes);
    if value.is_empty() { None } else { Some(value) }
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

fn unescape_owned(mut value: String) -> String {
    if !value.as_bytes().contains(&b'\\') {
        return value;
    }
    let mut escaped = false;
    value.retain(|ch| {
        if escaped {
            escaped = false;
            true
        } else if ch == '\\' {
            escaped = true;
            false
        } else {
            true
        }
    });
    if escaped {
        value.push('\\');
    }
    value
}

fn decode_unescape_owned(bytes: &[u8]) -> String {
    match decode_bytes(bytes) {
        Cow::Borrowed(value) => unescape_tag(value).into_owned(),
        Cow::Owned(value) => unescape_owned(value),
    }
}

fn trim_owned(value: &mut String) {
    let trimmed = value.trim();
    let start = trimmed.as_ptr() as usize - value.as_ptr() as usize;
    let end = start + trimmed.len();
    value.truncate(end);
    if start != 0 {
        value.drain(..start);
    }
}

fn decode_trim_owned(bytes: &[u8]) -> String {
    match decode_bytes(bytes) {
        Cow::Borrowed(value) => value.trim().to_owned(),
        Cow::Owned(mut value) => {
            trim_owned(&mut value);
            value
        }
    }
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

/// Calculates mono (same-foot patterns) and candle stats.
fn compute_mono_and_candle_stats(
    facing_steps: (u32, u32),
    stats: &stats::ArrowStats,
    detected_patterns: &PatternCounts,
) -> (u32, u32, u32, f64, u32, f64) {
    if stats.total_steps <= 1 {
        return (0, 0, 0, 0.0, 0, 0.0);
    }

    let (facing_left, facing_right) = facing_steps;
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
    matrix_profile: MatrixProfile,
}

fn parity_scratch<const LANES: usize>(
    scratch: &mut Option<step_parity::TimingRowsScratch<LANES>>,
) -> &mut step_parity::TimingRowsScratch<LANES> {
    if scratch.is_none() {
        *scratch = Some(
            step_parity::timing_rows_scratch::<LANES>()
                .expect("4-lane and 8-lane parity layouts are compiled in"),
        );
    }
    scratch
        .as_mut()
        .expect("parity scratch exists after initialization")
}

fn parity_outputs<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    timing: &TimingData,
    has_holds: bool,
    scratch: &mut Option<step_parity::TimingRowsScratch<LANES>>,
    options: &AnalysisOptions,
) -> (
    step_parity::TechCounts,
    Option<Vec<step_parity::RowAnnotation>>,
) {
    match (
        options.compute_tech_counts,
        options.compute_note_annotations,
    ) {
        (true, true) => {
            let (counts, annotations) = step_parity::analyze_and_annotate_timing_rows_known_holds(
                rows,
                row_to_beat,
                timing,
                has_holds,
                parity_scratch(scratch),
            );
            (counts, Some(annotations))
        }
        (true, false) => (
            step_parity::analyze_timing_rows_known_holds(
                rows,
                row_to_beat,
                timing,
                has_holds,
                parity_scratch(scratch),
            ),
            None,
        ),
        (false, true) => (
            step_parity::TechCounts::default(),
            Some(step_parity::annotate_timing_rows_known_holds(
                rows,
                row_to_beat,
                timing,
                has_holds,
                parity_scratch(scratch),
            )),
        ),
        (false, false) => (step_parity::TechCounts::default(), None),
    }
}

// Computes various metrics derived from measure densities and the BPM map.
fn compute_derived_chart_metrics(
    measure_densities: &[usize],
    bpm_map: &[(f64, f64)],
    minimized_chart: &[u8],
    bpms_to_use: &str,
    stream_tokens: &mut Vec<stats::Token>,
) -> DerivedChartMetrics {
    let (stream_counts, sn_breakdowns, standard_breakdowns) =
        compute_stream_outputs_with_scratch(measure_densities, stream_tokens);
    let total_streams = stream_counts.run16_streams
        + stream_counts.run20_streams
        + stream_counts.run24_streams
        + stream_counts.run32_streams;

    let (short_hash, bpm_neutral_hash) = compute_chart_hash_pair(minimized_chart, bpms_to_use);
    if total_streams == 0 {
        let tier_bpm = round_dp(compute_tier_bpm(&[], bpm_map, 4.0), 2);
        let (detailed_breakdown, partial_breakdown, simple_breakdown) = standard_breakdowns;
        return DerivedChartMetrics {
            stream_counts,
            total_streams,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown,
            partial_breakdown,
            simple_breakdown,
            short_hash,
            bpm_neutral_hash,
            tier_bpm,
            matrix_rating: 0.0,
            matrix_profile: MatrixProfile::default(),
        };
    }

    let tier_bpm = round_dp(compute_tier_bpm(measure_densities, bpm_map, 4.0), 2);
    let (sn_detailed_breakdown, sn_partial_breakdown, sn_simple_breakdown) = sn_breakdowns;
    let (detailed_breakdown, partial_breakdown, simple_breakdown) = standard_breakdowns;

    let matrix_profile = compute_matrix_profile(measure_densities, bpm_map);
    let matrix_rating = round_dp(matrix_profile.rating_at_rate(1.0), 2);

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
        matrix_profile,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ChartMetadataStrings {
    step_type: String,
    step_artist: String,
    description: String,
    chart_name: String,
    difficulty: String,
    rating: String,
    tech_notation: String,
}

#[inline(always)]
fn chart_metadata_strings(
    fields: [&[u8]; 5],
    chart_name: Option<&[u8]>,
    timing_format: TimingFormat,
    ssc_version: f32,
    extension: &str,
) -> ChartMetadataStrings {
    let step_type = decode_unescape_trim(fields[0]).into_owned();
    let description_raw = decode_unescape_trim(fields[1]);
    let legacy_ssc = timing_format == TimingFormat::Ssc && ssc_version < SSC_VERSION_CHART_NAME_TAG;
    let (description, chart_name) = if legacy_ssc {
        (String::new(), description_raw.into_owned())
    } else {
        (
            description_raw.into_owned(),
            chart_name
                .map(|bytes| decode_unescape_trim(bytes).into_owned())
                .unwrap_or_default(),
        )
    };
    let difficulty_raw = decode_unescape_trim(fields[2]);
    let rating = decode_unescape_trim(fields[3]).into_owned();
    let difficulty =
        resolve_difficulty_label(difficulty_raw.as_ref(), &description, &rating, extension);
    let is_ssc = extension.eq_ignore_ascii_case("ssc");
    let credit_decoded = if is_ssc {
        decode_bytes(fields[4])
    } else {
        Cow::Borrowed("")
    };
    let credit = unescape_tag(credit_decoded.as_ref());
    let tech_notation = parse_tech_notation(credit.as_ref(), &description);
    let step_artist = if is_ssc {
        credit.into_owned()
    } else {
        description.clone()
    };

    ChartMetadataStrings {
        step_type,
        step_artist,
        description,
        chart_name,
        difficulty,
        rating,
        tech_notation,
    }
}

#[cfg(feature = "profile")]
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub fn profile_chart_metadata_strings(
    fields: [&[u8]; 5],
    chart_name: Option<&[u8]>,
    timing_format: TimingFormat,
    ssc_version: f32,
    extension: &str,
) -> (String, String, String, String, String, String, String) {
    let metadata =
        chart_metadata_strings(fields, chart_name, timing_format, ssc_version, extension);
    (
        metadata.step_type,
        metadata.step_artist,
        metadata.description,
        metadata.chart_name,
        metadata.difficulty,
        metadata.rating,
        metadata.tech_notation,
    )
}

/// Processes a single chart's data to produce a `ChartSummary`.
fn build_chart_summary<const REUSE_BPMS: bool>(
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
    global_timing: &mut Option<TimingData>,
    global_bpm_map: &[(f64, f64)],
    song_offset: f64,
    extension: &str,
    timing_format: TimingFormat,
    ssc_version: f32,
    allow_steps_timing: bool,
    compiled_custom_patterns: &CompiledCustomPatterns,
    parity_scratch4: &mut Option<step_parity::TimingRowsScratch<4>>,
    parity_scratch8: &mut Option<step_parity::TimingRowsScratch<8>>,
    rows4: &mut stats::TypedRowsScratch<4>,
    rows8: &mut stats::TypedRowsScratch<8>,
    chart_bpm_scratch: &mut Vec<(f64, f64)>,
    nps_scratch: &mut Vec<f64>,
    stream_tokens: &mut Vec<stats::Token>,
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
    let chart_attacks_opt = entry.chart_attacks.as_deref();
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
    let chart_music_path = entry
        .chart_music
        .as_deref()
        .map(decode_unescape_owned)
        .unwrap_or_default();

    let metadata = chart_metadata_strings(
        fields,
        entry.chart_name,
        timing_format,
        ssc_version,
        extension,
    );
    let chart_style = entry
        .chart_style
        .as_ref()
        .map(|bytes| decode_unescape_owned(bytes))
        .unwrap_or_default();

    let compute_patterns = lanes == 4 && options.compute_pattern_counts;
    let want_parity_rows =
        matches!(lanes, 4 | 8) && (options.compute_tech_counts || options.compute_note_annotations);
    let rows_collected = compute_patterns || want_parity_rows;
    rows4.clear();
    rows8.clear();
    let (mut minimized_chart, mut stats, measure_densities, row_to_beat, last_beat) =
        if compute_patterns {
            let (chart, stats, densities, row_to_beat, last_beat) =
                minimize_rows_typed_in::<4>(chart_data, rows4);
            (chart, stats, densities, row_to_beat, last_beat)
        } else if !want_parity_rows {
            let (chart, stats, densities, row_to_beat, last_beat) =
                minimize_chart_count_rows(chart_data, lanes);
            (chart, stats, densities, row_to_beat, last_beat)
        } else if lanes == 8 {
            let (chart, stats, densities, row_to_beat, last_beat) =
                minimize_rows_typed_in::<8>(chart_data, rows8);
            (chart, stats, densities, row_to_beat, last_beat)
        } else {
            let (chart, stats, densities, row_to_beat, last_beat) =
                minimize_rows_typed_in::<4>(chart_data, rows4);
            (chart, stats, densities, row_to_beat, last_beat)
        };
    let rows4 = rows4.rows();
    let rows8 = rows8.rows();
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }

    let (chart_bpms, chart_bpms_norm) = chart_timing_tag_pair(chart_bpms_opt);
    let bpms_to_use = chart_bpms_norm.as_deref().unwrap_or(global_bpms_norm);
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
        let value = decode_trim_owned(bytes);
        if value.is_empty() { None } else { Some(value) }
    });
    let chart_labels = chart_labels_opt.and_then(|bytes| {
        let first_param = msd_first_param_bytes(bytes);
        let mut value = decode_unescape_owned(first_param);
        value.retain(|ch| !ch.is_control());
        trim_owned(&mut value);
        if value.is_empty() { None } else { Some(value) }
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
    let chart_attacks = chart_attacks_opt
        .and_then(|bytes| {
            std::str::from_utf8(bytes)
                .ok()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            // Chart attacks was blank (after trimming) or absent - try again with global attacks
            global_attacks_opt.and_then(|bytes| {
                std::str::from_utf8(bytes)
                    .ok()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            })
        });
    let chart_has_own_attacks = chart_attacks.is_some() && entry.chart_attacks.is_some();
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
    let timing_segments = if chart_has_own_timing {
        Arc::new(compute_timing_segments(
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
        ))
    } else {
        Arc::clone(global_timing_segments)
    };
    let owned_bpm_map;
    let bpm_map = if chart_has_own_timing {
        let bpms = timing_segments
            .bpms
            .iter()
            .map(|(beat, bpm)| (f64::from(*beat), f64::from(*bpm)));
        if REUSE_BPMS {
            chart_bpm_scratch.clear();
            chart_bpm_scratch.extend(bpms);
            chart_bpm_scratch.as_slice()
        } else {
            owned_bpm_map = bpms.collect::<Vec<_>>();
            owned_bpm_map.as_slice()
        }
    } else {
        global_bpm_map
    };

    let metrics = compute_derived_chart_metrics(
        &measure_densities,
        bpm_map,
        &minimized_chart,
        bpms_to_use,
        stream_tokens,
    );

    let pattern_analysis = compute_patterns.then(|| {
        analyze_patterns_from_rows(rows4, options.mono_threshold, compiled_custom_patterns)
    });
    let (detected_patterns, (anchor_left, anchor_down, anchor_up, anchor_right)) = pattern_analysis
        .as_ref()
        .map_or(([0u32; PATTERN_COUNT], (0, 0, 0, 0)), |analysis| {
            (analysis.detected_patterns, analysis.anchors)
        });

    let (facing_left, facing_right, mono_total, mono_percent_raw, candle_total, candle_percent_raw) =
        pattern_analysis
            .as_ref()
            .map_or((0, 0, 0, 0.0, 0, 0.0), |analysis| {
                compute_mono_and_candle_stats(analysis.facing_steps, &stats, &detected_patterns)
            });
    let mono_percent = round_dp(mono_percent_raw, 2);
    let candle_percent = round_dp(candle_percent_raw, 2);

    let custom_patterns =
        pattern_analysis.map_or_else(Vec::new, |analysis| analysis.custom_patterns);

    let chart_timing;
    let timing = if chart_has_own_timing {
        chart_timing = timing_data_from_segments(chart_offset, 0.0, &timing_segments);
        &chart_timing
    } else {
        global_timing.get_or_insert_with(|| {
            timing_data_from_segments(song_offset, 0.0, global_timing_segments)
        })
    };

    let duration_seconds =
        duration::chart_duration_seconds(last_beat, timing, TimingOffsets::default());
    let chart_length = if last_beat <= 0.0 {
        0
    } else {
        let time_chart_f64 = get_time_for_beat(timing, last_beat);
        (time_chart_f64 + (song_offset - chart_offset)).floor() as i32
    };

    let mut measure_nps_vec = compute_measure_nps_vec_with_timing(&measure_densities, timing);
    let (max_nps_raw, median_nps_raw) = get_nps_stats_with_scratch(&measure_nps_vec, nps_scratch);
    let max_nps = round_sig_figs_6(max_nps_raw);
    let median_nps = round_dp(median_nps_raw, 2);
    for nps in &mut measure_nps_vec {
        *nps = round_sig_figs_6(*nps);
    }

    let raw_holding = stats.holding;
    let reuse_base_stats =
        stats.holds == 0 && stats.rolls == 0 && stats.lifts == 0 && !has_nonjudgable_rows(timing);
    let has_hold_notes = stats.holds != 0 || stats.rolls != 0;
    let (tech_counts, mut timing_stats, note_annotations) = match lanes {
        4 => {
            let timing_stats = if reuse_base_stats {
                std::mem::take(&mut stats)
            } else if !rows_collected {
                compute_timing_aware_stats_with_row_to_beat(
                    &minimized_chart,
                    lanes,
                    timing,
                    &row_to_beat,
                )
            } else if stats.holds == 0 && stats.rolls == 0 {
                compute_timing_aware_stats_no_holds_from_rows::<4>(rows4, timing, &row_to_beat)
            } else {
                compute_timing_aware_stats_from_rows_with_row_to_beat::<4>(
                    rows4,
                    timing,
                    &row_to_beat,
                )
            };
            let (tech_counts, note_annotations) = parity_outputs(
                rows4,
                &row_to_beat,
                timing,
                has_hold_notes,
                parity_scratch4,
                options,
            );
            (tech_counts, timing_stats, note_annotations)
        }
        8 => {
            let timing_stats = if reuse_base_stats {
                std::mem::take(&mut stats)
            } else if !rows_collected {
                compute_timing_aware_stats_with_row_to_beat(
                    &minimized_chart,
                    lanes,
                    timing,
                    &row_to_beat,
                )
            } else if stats.holds == 0 && stats.rolls == 0 {
                compute_timing_aware_stats_no_holds_from_rows::<8>(rows8, timing, &row_to_beat)
            } else {
                compute_timing_aware_stats_from_rows_with_row_to_beat::<8>(
                    rows8,
                    timing,
                    &row_to_beat,
                )
            };
            let (tech_counts, note_annotations) = parity_outputs(
                rows8,
                &row_to_beat,
                timing,
                has_hold_notes,
                parity_scratch8,
                options,
            );
            (tech_counts, timing_stats, note_annotations)
        }
        _ => {
            let tech_counts = step_parity::TechCounts::default();
            let timing_stats = if reuse_base_stats {
                std::mem::take(&mut stats)
            } else {
                compute_timing_aware_stats_with_row_to_beat(
                    &minimized_chart,
                    lanes,
                    timing,
                    &row_to_beat,
                )
            };
            let note_annotations = options.compute_note_annotations.then(Vec::new);
            (tech_counts, timing_stats, note_annotations)
        }
    };
    timing_stats.holding = raw_holding;
    let mines_nonfake = timing_stats.mines;
    stats = timing_stats;

    let elapsed_chart = chart_start_time.elapsed();

    Some((
        ChartSummary {
            step_type_str: metadata.step_type,
            step_artist_str: metadata.step_artist,
            description_str: metadata.description,
            chart_name_str: metadata.chart_name,
            chart_style_str: chart_style,
            difficulty_str: metadata.difficulty,
            rating_str: metadata.rating,
            tech_notation_str: metadata.tech_notation,
            tier_bpm: metrics.tier_bpm,
            matrix_rating: metrics.matrix_rating,
            matrix_profile: metrics.matrix_profile,
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
            note_annotations,
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
            music_path: chart_music_path,
            chart_attacks,
            chart_has_own_attacks,
            chart_stops,
            chart_speeds,
            chart_scrolls,
            chart_bpms,
            chart_bpms_norm,
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

/// # Errors
///
/// Returns an error when the extension is unsupported, parsing fails, or no
/// supported chart is present.
pub fn analyze(
    simfile_data: &[u8],
    extension: &str,
    options: &AnalysisOptions,
) -> Result<SimfileSummary, String> {
    let mut scratch = AnalysisScratch::default();
    analyze_with_scratch(simfile_data, extension, options, &mut scratch)
}

/// Analyzes a simfile while reusing caller-owned temporary storage.
///
/// This produces the same owned summary as [`analyze`]. Reusing one workspace
/// across a batch avoids rebuilding large parity arenas and median/token
/// buffers for every file.
///
/// # Errors
///
/// Returns an error when the extension is unsupported, parsing fails, or no
/// supported chart is present.
pub fn analyze_with_scratch(
    simfile_data: &[u8],
    extension: &str,
    options: &AnalysisOptions,
    scratch: &mut AnalysisScratch,
) -> Result<SimfileSummary, String> {
    analyze_with_scratch_impl::<true>(simfile_data, extension, options, scratch, None)
}

/// Analyzes a simfile with precompiled options and reusable storage.
///
/// # Errors
///
/// Returns an error when the extension is unsupported, parsing fails, or no
/// supported chart is present.
pub fn analyze_prepared_in(
    simfile_data: &[u8],
    extension: &str,
    prepared: &PreparedAnalysis,
    scratch: &mut AnalysisScratch,
) -> Result<SimfileSummary, String> {
    analyze_with_scratch_impl::<true>(
        simfile_data,
        extension,
        &prepared.options,
        scratch,
        Some(&prepared.custom_patterns),
    )
}

#[cfg(feature = "profile")]
pub(crate) fn profile_analyze_with_allocating_bpms(
    simfile_data: &[u8],
    extension: &str,
    options: &AnalysisOptions,
    scratch: &mut AnalysisScratch,
) -> Result<SimfileSummary, String> {
    analyze_with_scratch_impl::<false>(simfile_data, extension, options, scratch, None)
}

fn analyze_with_scratch_impl<const REUSE_BPMS: bool>(
    simfile_data: &[u8],
    extension: &str,
    options: &AnalysisOptions,
    scratch: &mut AnalysisScratch,
    prepared_patterns: Option<&CompiledCustomPatterns>,
) -> Result<SimfileSummary, String> {
    let total_start_time = Instant::now();

    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let mut title_str = parsed_data.title.map_or_else(
        || "<invalid-title>".to_string(),
        |b| {
            let mut value = decode_unescape_owned(b);
            value.retain(|ch| !ch.is_control());
            value
        },
    );
    if options.strip_tags {
        let stripped = strip_title_tags(&title_str);
        if stripped.as_ref() != title_str.as_str() {
            title_str = stripped.into_owned();
        }
    }
    trim_owned(&mut title_str);

    let mut subtitle_str = parsed_data
        .subtitle
        .map(|b| decode_unescape_trim(b).into_owned())
        .unwrap_or_default();
    let mut artist_str = parsed_data
        .artist
        .map(|b| decode_unescape_trim(b).into_owned())
        .unwrap_or_default();
    let genre_str = parsed_data
        .genre
        .map(|b| decode_unescape_trim(b).into_owned())
        .unwrap_or_default();
    let mut titletranslit_str = parsed_data
        .title_translit
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let mut subtitletranslit_str = parsed_data
        .subtitle_translit
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let mut artisttranslit_str = parsed_data
        .artist_translit
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let origin_str = parsed_data
        .origin
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let credit_str = parsed_data
        .credit
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let banner_path_str = parsed_data
        .banner
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let background_path_str = parsed_data
        .background
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let cdtitle_path_str = parsed_data
        .cdtitle
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let jacket_path_str = parsed_data
        .jacket
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let music_path_str = parsed_data
        .music
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let previewvid_str = parsed_data
        .previewvid
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let cdimage_str = parsed_data
        .cdimage
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let discimage_str = parsed_data
        .discimage
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let lyricspath_str = parsed_data
        .lyricspath
        .map(decode_unescape_owned)
        .unwrap_or_default();
    let selectable_bool = parsed_data
        .selectable
        .map(decode_unescape_owned)
        .unwrap_or_default()
        != "NO";
    let timing_format = timing_format_from_ext(extension);
    let display_bpm_str = parsed_data
        .display_bpm
        .map(decode_unescape_owned)
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
    let (cleaned_global_bpms, normalized_global_bpms) =
        clean_and_normalize_float_digits(global_bpms_raw);
    let global_stops_raw = parsed_data
        .stops
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_stops, normalized_global_stops) =
        clean_and_normalize_float_digits(global_stops_raw);
    let global_delays_raw = parsed_data
        .delays
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_delays, normalized_global_delays) =
        clean_and_normalize_float_digits(global_delays_raw);
    let global_warps_raw = parsed_data
        .warps
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_warps, normalized_global_warps) =
        clean_and_normalize_float_digits(global_warps_raw);
    let global_speeds_raw = parsed_data
        .speeds
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_speeds, normalized_global_speeds) =
        clean_and_normalize_speeds_float_digits(global_speeds_raw);
    let global_scrolls_raw = parsed_data
        .scrolls
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_scrolls, normalized_global_scrolls) =
        clean_and_normalize_float_digits(global_scrolls_raw);
    let global_fakes_raw = parsed_data
        .fakes
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let (cleaned_global_fakes, normalized_global_fakes) =
        clean_and_normalize_float_digits(global_fakes_raw);
    let normalized_global_time_signatures = parsed_data
        .time_signatures
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_labels = parsed_data
        .labels
        .map(|b| {
            let first_param = msd_first_param_bytes(b);
            let mut value = decode_unescape_owned(first_param);
            value.retain(|ch| !ch.is_control());
            value
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
    let normalized_global_bgchanges = parsed_data
        .bgchanges
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_fgchanges = parsed_data
        .fgchanges
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_keysounds = parsed_data
        .keysounds
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let normalized_global_attacks = parsed_data
        .attacks
        .as_deref()
        .and_then(|b| std::str::from_utf8(b).ok())
        .map_or("", str::trim)
        .to_string();
    let last_second_hint = parsed_data
        .last_second_hint
        .map(|b| parse_offset_seconds(Some(b)))
        .and_then(|n| if n <= 0.0 { None } else { Some(n) });

    let allow_steps_timing = steps_timing_allowed(ssc_version, timing_format);
    let owned_patterns;
    let compiled_custom_patterns = if let Some(compiled) = prepared_patterns {
        compiled
    } else {
        owned_patterns = if options.compute_pattern_counts && !options.custom_patterns.is_empty() {
            compile_custom_patterns(&options.custom_patterns)
        } else {
            compiled_custom_empty()
        };
        &owned_patterns
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
    let global_bpms = global_timing_segments
        .bpms
        .iter()
        .map(|(beat, bpm)| (f64::from(*beat), f64::from(*bpm)));
    let owned_global_bpm_map;
    let global_bpm_map = if REUSE_BPMS {
        scratch.global_bpm_map.clear();
        scratch.global_bpm_map.extend(global_bpms);
        scratch.global_bpm_map.as_slice()
    } else {
        owned_global_bpm_map = global_bpms.collect::<Vec<_>>();
        owned_global_bpm_map.as_slice()
    };
    let (min_bpm_i32, max_bpm_i32, median_bpm_raw, average_bpm_raw) = if REUSE_BPMS {
        compute_bpm_range_and_stats_with_scratch(global_bpm_map, &mut scratch.bpm_values)
    } else {
        compute_bpm_range_and_stats(global_bpm_map)
    };
    let median_bpm = round_dp(median_bpm_raw, 2);
    let average_bpm = round_dp(average_bpm_raw, 2);
    let global_attacks_opt = parsed_data.attacks.as_deref();

    let entries = parsed_data.notes_list;
    let entry_count = entries.len();
    let mut chart_summaries = Vec::with_capacity(entry_count);
    let mut total_length = 0i32;
    let mut global_timing = None;
    let options_ref = &options;
    let compiled_custom_patterns_ref = compiled_custom_patterns;
    for entry in entries {
        if let Some((summary, chart_length)) = build_chart_summary::<REUSE_BPMS>(
            &entry,
            global_attacks_opt,
            &cleaned_global_bpms,
            &cleaned_global_stops,
            &cleaned_global_delays,
            &cleaned_global_warps,
            &cleaned_global_speeds,
            &cleaned_global_scrolls,
            &cleaned_global_fakes,
            &normalized_global_bpms,
            &global_timing_segments,
            &mut global_timing,
            global_bpm_map,
            offset,
            extension,
            timing_format,
            ssc_version,
            allow_steps_timing,
            compiled_custom_patterns_ref,
            &mut scratch.parity4,
            &mut scratch.parity8,
            &mut scratch.rows4,
            &mut scratch.rows8,
            &mut scratch.chart_bpm_map,
            &mut scratch.nps,
            &mut scratch.stream_tokens,
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
        genre_str,
        titletranslit_str,
        subtitletranslit_str,
        artisttranslit_str,
        origin_str,
        credit_str,
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
        normalized_bgchanges: normalized_global_bgchanges,
        normalized_fgchanges: normalized_global_fgchanges,
        normalized_keysounds: normalized_global_keysounds,
        normalized_attacks: normalized_global_attacks,
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
        global_timing_segments,
        previewvid_path: previewvid_str,
        cdimage_path: cdimage_str,
        discimage_path: discimage_str,
        lyrics_path: lyricspath_str,
        selectable: selectable_bool,
        last_second_hint,
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

    let entries = parsed_data.notes_list;
    let mut results = Vec::with_capacity(entries.len());

    for entry in entries {
        // 3. Split fields to get Metadata (StepType, Difficulty)
        if entry.field_count < 5 {
            continue;
        }
        let fields = entry.fields;
        let chart_data = entry.note_data;
        let Some(lanes) = supported_stepstype_lanes_bytes(fields[0]) else {
            continue;
        };

        let step_type = decode_unescape_trim(fields[0]).into_owned();
        let description_raw = decode_unescape_trim(fields[1]);
        let description =
            normalize_chart_desc_ref(description_raw.as_ref(), timing_format, ssc_version);
        let difficulty_raw = decode_unescape_trim(fields[2]);
        let meter_raw = decode_unescape_trim(fields[3]);
        let difficulty = resolve_difficulty_label(
            difficulty_raw.as_ref(),
            description,
            meter_raw.as_ref(),
            extension,
        );

        // 4. Normalize BPMs (Required for Hash consistency)
        let bpms_to_use = entry.chart_bpms.as_deref().map_or(
            Cow::Borrowed(normalized_global_bpms.as_str()),
            |chart_bpms| {
                let normalized =
                    normalize_float_digits(std::str::from_utf8(chart_bpms).unwrap_or(""));
                Cow::Owned(normalized)
            },
        );

        // 5. Minimize rows directly into SHA-1 without materializing the chart.
        let hash = crate::hash::compute_note_data_hash(chart_data, lanes, bpms_to_use.as_ref());

        results.push(ChartHashInfo {
            step_type,
            difficulty,
            hash,
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::{
        AnalysisOptions, AnalysisScratch, ChartMetadataStrings, PreparedAnalysis, analyze,
        analyze_prepared_in, analyze_with_scratch, analyze_with_scratch_impl,
        chart_metadata_strings, compute_all_hashes, decode_trim_owned, decode_unescape_owned,
    };
    use crate::parse::{
        decode_bytes, normalize_chart_desc, normalize_chart_name, unescape_tag, unescape_trim,
    };
    use crate::tech::parse_tech_notation;
    use crate::{resolve_difficulty_label, timing::TimingFormat};
    use std::borrow::Cow;

    const FIXTURE: &[u8] = include_bytes!("../benches/fixtures/hash_fixture.ssc");

    fn json(summary: &crate::SimfileSummary) -> Vec<u8> {
        let mut out = Vec::new();
        crate::report::write_reports(summary, crate::report::OutputMode::JSON, &mut out)
            .expect("summary should serialize");
        out
    }

    #[test]
    fn reused_analysis_scratch_preserves_output() {
        let options = AnalysisOptions {
            compute_note_annotations: true,
            ..AnalysisOptions::default()
        };
        let mut legacy_scratch = AnalysisScratch::default();
        let expected =
            analyze_with_scratch_impl::<false>(FIXTURE, "ssc", &options, &mut legacy_scratch, None)
                .expect("legacy analysis should succeed");
        let mut scratch = AnalysisScratch::default();

        let first = analyze_with_scratch(FIXTURE, "ssc", &options, &mut scratch)
            .expect("first reused analysis should succeed");
        let scratch_capacities = (
            scratch.global_bpm_map.capacity(),
            scratch.chart_bpm_map.capacity(),
            scratch.bpm_values.capacity(),
            scratch.rows4.row_capacity(),
            scratch.rows8.row_capacity(),
        );
        let second = analyze_with_scratch(FIXTURE, "ssc", &options, &mut scratch)
            .expect("second reused analysis should succeed");

        assert_eq!(json(&first), json(&expected));
        assert_eq!(json(&second), json(&expected));
        assert!(scratch_capacities.0 > 0 && scratch_capacities.2 > 0);
        assert!(scratch_capacities.3 > 0);
        assert_eq!(
            (
                scratch.global_bpm_map.capacity(),
                scratch.chart_bpm_map.capacity(),
                scratch.bpm_values.capacity(),
                scratch.rows4.row_capacity(),
                scratch.rows8.row_capacity(),
            ),
            scratch_capacities
        );

        const LOCAL_TIMING: &[u8] = concat!(
            "#VERSION:0.83;\n#BPMS:0=120,8=180;\n",
            "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n",
            "#DESCRIPTION:local timing;\n#DIFFICULTY:Challenge;\n#METER:10;\n#CREDIT:;\n",
            "#BPMS:0=150,4=200;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n"
        )
        .as_bytes();
        let expected = analyze_with_scratch_impl::<false>(
            LOCAL_TIMING,
            "ssc",
            &options,
            &mut legacy_scratch,
            None,
        )
        .expect("legacy local timing analysis should succeed");
        let first = analyze_with_scratch(LOCAL_TIMING, "ssc", &options, &mut scratch)
            .expect("local timing analysis should succeed");
        let chart_capacity = scratch.chart_bpm_map.capacity();
        let second = analyze_with_scratch(LOCAL_TIMING, "ssc", &options, &mut scratch)
            .expect("repeated local timing analysis should succeed");
        assert_eq!(json(&first), json(&expected));
        assert_eq!(json(&second), json(&expected));
        assert!(chart_capacity > 0);
        assert_eq!(scratch.chart_bpm_map.capacity(), chart_capacity);
    }

    #[test]
    fn prepared_analysis_preserves_custom_pattern_output() {
        let options = AnalysisOptions {
            custom_patterns: vec!["LDR".to_string(), "RDL".to_string()],
            compute_tech_counts: false,
            ..AnalysisOptions::default()
        };
        let expected = analyze(FIXTURE, "ssc", &options).expect("analysis should succeed");
        let prepared = PreparedAnalysis::new(options.clone());
        let mut scratch = AnalysisScratch::default();
        let first = analyze_prepared_in(FIXTURE, "ssc", &prepared, &mut scratch)
            .expect("prepared analysis should succeed");
        let second = analyze_prepared_in(FIXTURE, "ssc", &prepared, &mut scratch)
            .expect("repeated prepared analysis should succeed");

        assert_eq!(prepared.options().custom_patterns, options.custom_patterns);
        assert_eq!(json(&first), json(&expected));
        assert_eq!(json(&second), json(&expected));
    }

    #[test]
    fn cached_chart_bpms_preserve_report_output() {
        const CHART: &[u8] = concat!(
            "#VERSION:0.83;\n#BPMS:0=60;\n",
            "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n",
            "#DESCRIPTION:cached BPMs;\n#DIFFICULTY:Challenge;\n#METER:10;\n#CREDIT:;\n",
            "#BPMS: 0 = 120, bad, 4.0004 = 150.9995;\n",
            "#NOTES:\n1000\n0100\n0010\n0001\n;\n"
        )
        .as_bytes();
        let summary = analyze(CHART, "ssc", &AnalysisOptions::default())
            .expect("chart BPM fixture should analyze");
        assert_eq!(
            summary.charts[0].chart_bpms_norm.as_deref(),
            Some("0.000=120.000,4.000=151.000")
        );

        let mut legacy = summary.clone();
        legacy
            .charts
            .first_mut()
            .expect("chart BPM fixture should contain a chart")
            .chart_bpms_norm = None;
        assert_eq!(json(&summary), json(&legacy));
    }

    #[test]
    fn owned_metadata_decoding_matches_existing_semantics() {
        let cases: [&[u8]; 5] = [
            b"plain metadata",
            b"escaped\\: metadata",
            b"trailing slash\\",
            b"double\\\\slash",
            &[b' ', 0x93, b'T', b'i', b't', b'l', b'e', 0x94, b' '],
        ];

        for bytes in cases {
            let decoded = decode_bytes(bytes);
            assert_eq!(
                decode_unescape_owned(bytes),
                unescape_tag(decoded.as_ref()).as_ref()
            );
            assert_eq!(decode_trim_owned(bytes), decoded.trim());
        }
    }

    fn materialized_chart_metadata(
        fields: [&[u8]; 5],
        chart_name: Option<&[u8]>,
        timing_format: TimingFormat,
        ssc_version: f32,
        extension: &str,
    ) -> ChartMetadataStrings {
        let step_type = unescape_trim(decode_bytes(fields[0]).as_ref());
        let description_raw = unescape_trim(decode_bytes(fields[1]).as_ref());
        let chart_name_raw = chart_name.map_or_else(String::new, |bytes| {
            unescape_trim(decode_bytes(bytes).as_ref())
        });
        let description = normalize_chart_desc(description_raw.clone(), timing_format, ssc_version);
        let chart_name =
            normalize_chart_name(chart_name_raw, &description_raw, timing_format, ssc_version);
        let difficulty_raw = unescape_trim(decode_bytes(fields[2]).as_ref());
        let rating = unescape_trim(decode_bytes(fields[3]).as_ref());
        let difficulty =
            resolve_difficulty_label(&difficulty_raw, &description, &rating, extension);
        let is_ssc = extension.eq_ignore_ascii_case("ssc");
        let credit_decoded = if is_ssc {
            decode_bytes(fields[4])
        } else {
            Cow::Borrowed("")
        };
        let credit = unescape_tag(credit_decoded.as_ref());
        let tech_notation = parse_tech_notation(credit.as_ref(), &description);
        let step_artist = if is_ssc {
            credit.into_owned()
        } else {
            description.clone()
        };

        ChartMetadataStrings {
            step_type,
            step_artist,
            description,
            chart_name,
            difficulty,
            rating,
            tech_notation,
        }
    }

    #[test]
    fn borrowed_chart_metadata_matches_materialized_pipeline() {
        let cases = [
            (
                [
                    b" dance-single " as &[u8],
                    b" BR+ Description ",
                    b"Hard",
                    b"12",
                    b"Artist\\: Name",
                ],
                Some(b" Modern\\: Name " as &[u8]),
                TimingFormat::Ssc,
                0.83,
                "ssc",
            ),
            (
                [
                    b"dance-single" as &[u8],
                    b" Legacy Description ",
                    b"Challenge",
                    b"10",
                    b"Credit",
                ],
                Some(b"Ignored Chart Name" as &[u8]),
                TimingFormat::Ssc,
                0.70,
                "ssc",
            ),
            (
                [
                    b"dance-single" as &[u8],
                    b" smaniac ",
                    b"Hard",
                    b"13",
                    b"0,0,0,0,0",
                ],
                None,
                TimingFormat::Sm,
                0.0,
                "sm",
            ),
            (
                [
                    b"dance-single" as &[u8],
                    b"\x93CP1252\x94",
                    b"Expert",
                    b" 9 ",
                    b"\x96 Credit",
                ],
                Some(b"\x93Name\x94" as &[u8]),
                TimingFormat::Ssc,
                0.83,
                "SSC",
            ),
        ];

        for (fields, chart_name, timing_format, version, extension) in cases {
            assert_eq!(
                chart_metadata_strings(fields, chart_name, timing_format, version, extension,),
                materialized_chart_metadata(fields, chart_name, timing_format, version, extension,)
            );
        }
    }

    #[test]
    fn combined_parity_outputs_match_independent_analysis_options() {
        let counts = analyze(FIXTURE, "ssc", &AnalysisOptions::default())
            .expect("count-only analysis should succeed");
        let annotations = analyze(
            FIXTURE,
            "ssc",
            &AnalysisOptions {
                compute_tech_counts: false,
                compute_note_annotations: true,
                ..AnalysisOptions::default()
            },
        )
        .expect("annotation-only analysis should succeed");
        let combined = analyze(
            FIXTURE,
            "ssc",
            &AnalysisOptions {
                compute_note_annotations: true,
                ..AnalysisOptions::default()
            },
        )
        .expect("combined analysis should succeed");

        assert_eq!(combined.charts.len(), counts.charts.len());
        assert_eq!(combined.charts.len(), annotations.charts.len());
        for ((combined_chart, counts_chart), annotations_chart) in combined
            .charts
            .iter()
            .zip(&counts.charts)
            .zip(&annotations.charts)
        {
            assert_eq!(combined_chart.tech_counts, counts_chart.tech_counts);
            assert_eq!(
                combined_chart.note_annotations,
                annotations_chart.note_annotations
            );
        }
    }

    #[test]
    fn batch_hashes_match_analysis_outputs() {
        let hashes = compute_all_hashes(FIXTURE, "ssc").expect("hashing should succeed");
        let summary =
            analyze(FIXTURE, "ssc", &AnalysisOptions::default()).expect("analysis should succeed");

        assert_eq!(hashes.len(), summary.charts.len());
        for (hash, chart) in hashes.iter().zip(&summary.charts) {
            assert_eq!(hash.step_type, chart.step_type_str);
            assert_eq!(hash.difficulty, chart.difficulty_str);
            assert_eq!(hash.hash, chart.short_hash);
        }
    }

    #[test]
    fn repeated_sm_attacks_reach_every_chart() {
        let simfile = b"#TITLE:Repeated Attacks;\n\
            #MUSIC:test.ogg;\n\
            #OFFSET:0;\n\
            #BPMS:0=120;\n\
            #ATTACKS:TIME=0:END=9999:MODS=overhead;\n\
            #ATTACKS:TIME=0.241:END=0.438:MODS=*1.875 15% invert;\n\
            #ATTACKS:TIME=0.338:END=0.515:MODS=*1.946 no invert;\n\
            #NOTES:\n\
                dance-single:\n\
                :\n\
                Easy:\n\
                1:\n\
                0,0,0,0,0:\n\
            0000\n\
            1000\n\
            0000\n\
            0000\n\
            ;";
        let summary = analyze(simfile, "sm", &AnalysisOptions::default())
            .expect("SM analysis should succeed");
        let expected = "TIME=0:END=9999:MODS=overhead:\
            TIME=0.241:END=0.438:MODS=*1.875 15% invert:\
            TIME=0.338:END=0.515:MODS=*1.946 no invert";

        assert_eq!(summary.normalized_attacks, expected);
        assert_eq!(summary.charts.len(), 1);
        assert_eq!(summary.charts[0].chart_attacks.as_deref(), Some(expected));
    }
}

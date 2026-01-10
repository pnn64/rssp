use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::report::format_json_float;
use rssp::timing::round_millis;
use rssp::{display_metadata, normalize_difficulty_label};

// --skip-slow disables pattern/tech counts; fast_all_parity skips those checks when missing.
const RSSP_ARGS: [&str; 2] = ["--json", "--skip-slow"];

#[derive(Debug, Clone, PartialEq)]
struct ExpectedMetadata {
    title: String,
    subtitle: String,
    artist: String,
    title_translated: String,
    subtitle_translated: String,
    artist_translated: String,
}

#[derive(Debug, Clone)]
struct ParsedMetadata {
    title: String,
    subtitle: String,
    artist: String,
    title_translated: String,
    subtitle_translated: String,
    artist_translated: String,
}

#[derive(Debug, Clone)]
struct TestCase {
    name: String,
    path: PathBuf,
    extension: String,
}

#[derive(Debug, Clone)]
struct Failure {
    name: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct HarnessChart {
    #[serde(rename = "steps_type")]
    step_type: String,
    difficulty: String,
    #[serde(default)]
    meter: Option<u32>,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    hash_bpms: String,
    #[serde(default)]
    bpms: String,
    #[serde(default)]
    bpm_min: f64,
    #[serde(default)]
    bpm_max: f64,
    #[serde(default)]
    display_bpm: String,
    #[serde(default)]
    display_bpm_min: f64,
    #[serde(default)]
    display_bpm_max: f64,
    #[serde(default)]
    duration_seconds: f64,
    #[serde(default)]
    streams_breakdown: String,
    #[serde(default)]
    streams_breakdown_level1: String,
    #[serde(default)]
    streams_breakdown_level2: String,
    #[serde(default)]
    total_stream_measures: u32,
    #[serde(default)]
    total_break_measures: u32,
    #[serde(default)]
    stream_sequences: Vec<StreamSequence>,
    #[serde(default)]
    peak_nps: f64,
    #[serde(default)]
    notes_per_measure: Vec<u32>,
    #[serde(default)]
    nps_per_measure: Vec<f64>,
    #[serde(default)]
    equally_spaced_per_measure: Vec<bool>,
    #[serde(default)]
    holds: u32,
    #[serde(default)]
    mines: u32,
    #[serde(default)]
    rolls: u32,
    #[serde(default)]
    notes: u32,
    #[serde(default)]
    lifts: u32,
    #[serde(default)]
    fakes: u32,
    #[serde(default)]
    jumps: u32,
    #[serde(default)]
    hands: u32,
    #[serde(default)]
    total_steps: u32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subtitle: String,
    #[serde(default)]
    artist: String,
    #[serde(rename = "title_translated", default)]
    title_translated: String,
    #[serde(rename = "subtitle_translated", default)]
    subtitle_translated: String,
    #[serde(rename = "artist_translated", default)]
    artist_translated: String,
    #[serde(default)]
    step_artist: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    timing: Option<HarnessTiming>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct StreamSequence {
    #[serde(rename = "stream_start")]
    stream_start: u32,
    #[serde(rename = "stream_end")]
    stream_end: u32,
    is_break: bool,
}

#[derive(Debug, Deserialize)]
struct HarnessTiming {
    beat0_offset_seconds: f64,
    beat0_group_offset_seconds: f64,
    bpms: Vec<(f64, f64)>,
    stops: Vec<(f64, f64)>,
    delays: Vec<(f64, f64)>,
    time_signatures: Vec<(f64, i32, i32)>,
    warps: Vec<(f64, f64)>,
    labels: Vec<(f64, String)>,
    tickcounts: Vec<(f64, i32)>,
    combos: Vec<(f64, i32, i32)>,
    speeds: Vec<(f64, f64, f64, i32)>,
    scrolls: Vec<(f64, f64)>,
    fakes: Vec<(f64, f64)>,
}

#[derive(Debug, Deserialize)]
struct RsspJsonFile {
    title: String,
    subtitle: String,
    artist: String,
    #[serde(rename = "title_trans", default)]
    title_trans: String,
    #[serde(rename = "subtitle_trans", default)]
    subtitle_trans: String,
    #[serde(rename = "artist_trans", default)]
    artist_trans: String,
    charts: Vec<RsspJsonChart>,
}

#[derive(Debug, Deserialize)]
struct RsspJsonChart {
    chart_info: RsspChartInfo,
    arrow_stats: RsspArrowStats,
    gimmicks: RsspGimmicks,
    stream_info: RsspStreamInfo,
    nps: RsspNps,
    breakdown: RsspSnBreakdown,
    stream_breakdown: RsspStreamBreakdown,
    timing: RsspTiming,
    #[serde(default)]
    mono_candle_stats: Option<RsspMonoCandleStats>,
    #[serde(default)]
    pattern_counts: Option<RsspPatternCounts>,
}

#[derive(Debug, Deserialize)]
struct RsspChartInfo {
    step_type: String,
    difficulty: String,
    rating: String,
    matrix_rating: f64,
    #[serde(default)]
    step_artists: String,
    #[serde(default)]
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct RsspArrowStats {
    total_arrows: u32,
    total_steps: u32,
    jumps: u32,
    hands: u32,
    holds: u32,
    rolls: u32,
    mines: u32,
}

#[derive(Debug, Deserialize)]
struct RsspGimmicks {
    lifts: u32,
    fakes: u32,
}

#[derive(Debug, Deserialize)]
struct RsspStreamInfo {
    total_streams: u32,
    total_breaks: u32,
    sn_breaks: u32,
    #[serde(default)]
    stream_sequences: Vec<StreamSequence>,
}

#[derive(Debug, Deserialize)]
struct RsspNps {
    max_nps: f64,
    #[serde(default)]
    notes_per_measure: Vec<u32>,
    #[serde(default)]
    nps_per_measure: Vec<f64>,
    #[serde(default)]
    equally_spaced_per_measure: Vec<bool>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct RsspSnBreakdown {
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct RsspStreamBreakdown {
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
}

#[derive(Debug, Deserialize)]
struct RsspTiming {
    beat0_offset_seconds: f64,
    beat0_group_offset_seconds: f64,
    bpms: Vec<(f64, f64)>,
    stops: Vec<(f64, f64)>,
    delays: Vec<(f64, f64)>,
    time_signatures: Vec<(f64, i32, i32)>,
    warps: Vec<(f64, f64)>,
    labels: Vec<(f64, String)>,
    tickcounts: Vec<(f64, i32)>,
    combos: Vec<(f64, i32, i32)>,
    speeds: Vec<(f64, f64, f64, i32)>,
    scrolls: Vec<(f64, f64)>,
    fakes: Vec<(f64, f64)>,
    bpms_formatted: String,
    bpm_min: f64,
    bpm_max: f64,
    #[serde(default)]
    display_bpm: String,
    #[serde(default)]
    display_bpm_min: f64,
    #[serde(default)]
    display_bpm_max: f64,
    #[serde(default)]
    hash_bpms: Option<String>,
    #[serde(default)]
    duration_seconds: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RsspBaselineFile {
    charts: Vec<RsspBaselineChart>,
}

#[derive(Debug, Deserialize)]
struct RsspBaselineChart {
    chart_info: RsspBaselineChartInfo,
    breakdown: RsspSnBreakdown,
    stream_info: RsspBaselineStreamInfo,
    #[serde(default)]
    mono_candle_stats: Option<RsspMonoCandleStats>,
    #[serde(default)]
    pattern_counts: Option<RsspPatternCounts>,
}

#[derive(Debug, Deserialize)]
struct RsspBaselineChartInfo {
    step_type: String,
    difficulty: String,
    rating: String,
    matrix_rating: f64,
}

#[derive(Debug, Deserialize)]
struct RsspBaselineStreamInfo {
    sn_breaks: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct RsspMonoCandleStats {
    total_candles: u32,
    left_foot_candles: u32,
    right_foot_candles: u32,
    candles_percent: f64,
    total_mono: u32,
    left_face_mono: u32,
    right_face_mono: u32,
    mono_percent: f64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct RsspPatternCounts {
    boxes: BoxesCounts,
    anchors: AnchorsCounts,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct BoxesCounts {
    total_boxes: u32,
    lr_boxes: u32,
    ud_boxes: u32,
    corner_boxes: u32,
    ld_boxes: u32,
    lu_boxes: u32,
    rd_boxes: u32,
    ru_boxes: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct AnchorsCounts {
    total_anchors: u32,
    left_anchors: u32,
    down_anchors: u32,
    up_anchors: u32,
    right_anchors: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct MonoCandleStats {
    total_candles: u32,
    left_foot_candles: u32,
    right_foot_candles: u32,
    candles_percent: String,
    total_mono: u32,
    left_face_mono: u32,
    right_face_mono: u32,
    mono_percent: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ChartUniqueValues {
    matrix_rating: String,
    breakdown: RsspSnBreakdown,
    mono_candle_stats: Option<MonoCandleStats>,
    pattern_counts: Option<RsspPatternCounts>,
}

fn normalize_step_type(raw: &str) -> String {
    raw.trim().replace('_', "-").to_ascii_lowercase()
}

fn chart_key(step_type: &str, difficulty: &str) -> Option<(String, String)> {
    let step_type = normalize_step_type(step_type);
    if step_type != "dance-single" && step_type != "dance-double" {
        return None;
    }
    let difficulty = normalize_difficulty_label(difficulty).to_ascii_lowercase();
    Some((step_type, difficulty))
}

fn build_index<T, FStep, FDiff>(
    items: &[T],
    step: FStep,
    diff: FDiff,
) -> HashMap<(String, String), Vec<usize>>
where
    FStep: Fn(&T) -> &str,
    FDiff: Fn(&T) -> &str,
{
    let mut map: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (idx, item) in items.iter().enumerate() {
        let Some(key) = chart_key(step(item), diff(item)) else {
            continue;
        };
        map.entry(key).or_default().push(idx);
    }
    map
}

fn sorted_entries(
    map: HashMap<(String, String), Vec<usize>>,
) -> Vec<((String, String), Vec<usize>)> {
    let mut entries: Vec<_> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

fn format_count(value: Option<u32>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn format_len<T>(value: Option<&[T]>) -> String {
    value
        .map(|v| v.len().to_string())
        .unwrap_or_else(|| "-".to_string())
}

const TIMING_EPS: f64 = 1e-3;

fn timing_approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= TIMING_EPS
}

fn timing_matches(expected: &HarnessTiming, actual: &RsspTiming) -> bool {
    if !timing_approx_eq(expected.beat0_offset_seconds, actual.beat0_offset_seconds) {
        return false;
    }
    if !timing_approx_eq(
        expected.beat0_group_offset_seconds,
        actual.beat0_group_offset_seconds,
    ) {
        return false;
    }
    if !compare_pairs(&expected.bpms, &actual.bpms) {
        return false;
    }
    if !compare_pairs(&expected.stops, &actual.stops) {
        return false;
    }
    if !compare_pairs(&expected.delays, &actual.delays) {
        return false;
    }
    if !compare_pairs(&expected.warps, &actual.warps) {
        return false;
    }
    if !compare_pairs(&expected.scrolls, &actual.scrolls) {
        return false;
    }
    if !compare_pairs(&expected.fakes, &actual.fakes) {
        return false;
    }
    if !compare_time_signatures(&expected.time_signatures, &actual.time_signatures) {
        return false;
    }
    if !compare_labels(&expected.labels, &actual.labels) {
        return false;
    }
    if !compare_tickcounts(&expected.tickcounts, &actual.tickcounts) {
        return false;
    }
    if !compare_combos(&expected.combos, &actual.combos) {
        return false;
    }
    if !compare_speeds(&expected.speeds, &actual.speeds) {
        return false;
    }

    true
}

fn compare_pairs(expected: &[(f64, f64)], actual: &[(f64, f64)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(e.0, a.0) && timing_approx_eq(e.1, a.1))
}

fn compare_time_signatures(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(e.0, a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_labels(expected: &[(f64, String)], actual: &[(f64, String)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(e.0, a.0) && e.1 == a.1)
}

fn compare_tickcounts(expected: &[(f64, i32)], actual: &[(f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(e.0, a.0) && e.1 == a.1)
}

fn compare_combos(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(e.0, a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_speeds(expected: &[(f64, f64, f64, i32)], actual: &[(f64, f64, f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(e, a)| {
            timing_approx_eq(e.0, a.0)
                && timing_approx_eq(e.1, a.1)
                && timing_approx_eq(e.2, a.2)
                && e.3 == a.3
        })
}

fn format_timing_counts(
    bpms: usize,
    stops: usize,
    delays: usize,
    warps: usize,
    speeds: usize,
    scrolls: usize,
    time_signatures: usize,
    labels: usize,
    tickcounts: usize,
    combos: usize,
    fakes: usize,
) -> String {
    format!(
        "bpms:{} stops:{} delays:{} warps:{} speeds:{} scrolls:{} time_sigs:{} labels:{} tickcounts:{} combos:{} fakes:{}",
        bpms,
        stops,
        delays,
        warps,
        speeds,
        scrolls,
        time_signatures,
        labels,
        tickcounts,
        combos,
        fakes
    )
}

fn timing_counts_expected(timing: &HarnessTiming) -> String {
    format_timing_counts(
        timing.bpms.len(),
        timing.stops.len(),
        timing.delays.len(),
        timing.warps.len(),
        timing.speeds.len(),
        timing.scrolls.len(),
        timing.time_signatures.len(),
        timing.labels.len(),
        timing.tickcounts.len(),
        timing.combos.len(),
        timing.fakes.len(),
    )
}

fn timing_counts_actual(timing: &RsspTiming) -> String {
    format_timing_counts(
        timing.bpms.len(),
        timing.stops.len(),
        timing.delays.len(),
        timing.warps.len(),
        timing.speeds.len(),
        timing.scrolls.len(),
        timing.time_signatures.len(),
        timing.labels.len(),
        timing.tickcounts.len(),
        timing.combos.len(),
        timing.fakes.len(),
    )
}

fn has_hash_prefix(value: &str) -> bool {
    value.trim_start().starts_with('#')
}

fn expected_metadata(entries: &[HarnessChart], path: &Path) -> Result<ExpectedMetadata, String> {
    let mut expected: Option<ExpectedMetadata> = None;

    for entry in entries {
        let current = ExpectedMetadata {
            title: entry.title.clone(),
            subtitle: entry.subtitle.clone(),
            artist: entry.artist.clone(),
            title_translated: entry.title_translated.clone(),
            subtitle_translated: entry.subtitle_translated.clone(),
            artist_translated: entry.artist_translated.clone(),
        };
        if let Some(ref expected_value) = expected {
            if expected_value != &current {
                return Err(format!(
                    "\n\nINCONSISTENT BASELINE\nFile: {}\nExpected: {:?}\nFound: {:?}\n",
                    path.display(),
                    expected_value,
                    current
                ));
            }
        } else {
            expected = Some(current);
        }
    }

    expected.ok_or_else(|| format!("\n\nMISSING BASELINE METADATA\nFile: {}\n", path.display()))
}

fn parse_metadata(actual: &RsspJsonFile) -> ParsedMetadata {
    let (title_translated, subtitle_translated, artist_translated) = display_metadata(
        &actual.title,
        &actual.subtitle,
        &actual.artist,
        &actual.title_trans,
        &actual.subtitle_trans,
        &actual.artist_trans,
        false,
    );

    ParsedMetadata {
        title: actual.title.clone(),
        subtitle: actual.subtitle.clone(),
        artist: actual.artist.clone(),
        title_translated,
        subtitle_translated,
        artist_translated,
    }
}

fn compare_metadata(
    path: &Path,
    expected: &ExpectedMetadata,
    actual: &ParsedMetadata,
) -> Result<(), String> {
    let title_ok = actual.title == expected.title;
    let subtitle_ok = actual.subtitle == expected.subtitle
        || (expected.subtitle.is_empty() && has_hash_prefix(&actual.subtitle));
    let artist_ok = actual.artist == expected.artist
        || (expected.artist == "Unknown artist" && has_hash_prefix(&actual.artist));
    let title_translated_ok = actual.title_translated == expected.title_translated;
    let subtitle_translated_ok = actual.subtitle_translated == expected.subtitle_translated
        || (expected.subtitle_translated.is_empty()
            && has_hash_prefix(&actual.subtitle_translated));
    let artist_translated_ok = actual.artist_translated == expected.artist_translated
        || (expected.artist_translated == "Unknown artist"
            && has_hash_prefix(&actual.artist_translated));

    let title_status = if title_ok { "....ok" } else { "....MISMATCH" };
    let subtitle_status = if subtitle_ok {
        "....ok"
    } else {
        "....MISMATCH"
    };
    let artist_status = if artist_ok { "....ok" } else { "....MISMATCH" };
    let title_translated_status = if title_translated_ok {
        "....ok"
    } else {
        "....MISMATCH"
    };
    let subtitle_translated_status = if subtitle_translated_ok {
        "....ok"
    } else {
        "....MISMATCH"
    };
    let artist_translated_status = if artist_translated_ok {
        "....ok"
    } else {
        "....MISMATCH"
    };

    println!(
        "  title: baseline: {} -> rssp: {} {}",
        expected.title, actual.title, title_status
    );
    println!(
        "  subtitle: baseline: {} -> rssp: {} {}",
        expected.subtitle, actual.subtitle, subtitle_status
    );
    println!(
        "  artist: baseline: {} -> rssp: {} {}",
        expected.artist, actual.artist, artist_status
    );
    println!(
        "  title_translated: baseline: {} -> rssp: {} {}",
        expected.title_translated, actual.title_translated, title_translated_status
    );
    println!(
        "  subtitle_translated: baseline: {} -> rssp: {} {}",
        expected.subtitle_translated, actual.subtitle_translated, subtitle_translated_status
    );
    println!(
        "  artist_translated: baseline: {} -> rssp: {} {}",
        expected.artist_translated, actual.artist_translated, artist_translated_status
    );

    let metadata_ok = title_ok
        && subtitle_ok
        && artist_ok
        && title_translated_ok
        && subtitle_translated_ok
        && artist_translated_ok;
    if metadata_ok {
        return Ok(());
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\nRSSP title:    {:?}\nGolden title:  {:?}\nRSSP subtitle: {:?}\nGolden subtitle: {:?}\nRSSP artist:   {:?}\nGolden artist: {:?}\nRSSP title_translated:    {:?}\nGolden title_translated:  {:?}\nRSSP subtitle_translated: {:?}\nGolden subtitle_translated: {:?}\nRSSP artist_translated:   {:?}\nGolden artist_translated: {:?}\n",
        path.display(),
        actual.title,
        expected.title,
        actual.subtitle,
        expected.subtitle,
        actual.artist,
        expected.artist,
        actual.title_translated,
        expected.title_translated,
        actual.subtitle_translated,
        expected.subtitle_translated,
        actual.artist_translated,
        expected.artist_translated,
    ))
}

fn compare_step_artists(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    let mut step_artist_ok = true;
    let mut step_artist_errors: Vec<String> = Vec::new();

    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  step_artist {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            step_artist_ok = false;
            step_artist_errors.push(format!(
                "Step artist chart missing: {} {}",
                step_type, difficulty
            ));
            continue;
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());
            let desc_label = expected
                .map(|entry| entry.description.trim())
                .filter(|label| !label.is_empty())
                .map(|label| format!("{} {}", meter_label, label))
                .unwrap_or_else(|| meter_label.clone());

            let expected_val = expected.map(|entry| entry.step_artist.as_str());
            let actual_val = actual.map(|entry| entry.chart_info.step_artists.as_str());
            let status = if expected_val.is_some() && expected_val == actual_val {
                "....ok"
            } else {
                step_artist_ok = false;
                "....MISMATCH"
            };

            println!(
                "  step_artist {} {} [{}]: baseline: {} -> rssp: {} {}",
                step_type,
                difficulty,
                desc_label,
                expected_val.unwrap_or("-"),
                actual_val.unwrap_or("-"),
                status
            );

            if status != "....ok" {
                step_artist_errors.push(format!(
                    "Step artist mismatch {} {} [{}]: RSSP step_artist: {:?}, Golden step_artist: {:?}",
                    step_type,
                    difficulty,
                    desc_label,
                    actual_val,
                    expected_val
                ));
            }
        }
    }

    if step_artist_ok {
        return Ok(());
    }

    let mut error_details = String::from("Step artist mismatches:\n");
    for line in step_artist_errors {
        error_details.push_str(&line);
        error_details.push('\n');
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
        path.display(),
        error_details
    ))
}

fn compare_bpm(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_hash = expected.map(|entry| entry.hash_bpms.as_str());
            let actual_hash = actual.and_then(|entry| entry.timing.hash_bpms.as_deref());
            let expected_bpms = expected.map(|entry| entry.bpms.as_str());
            let actual_bpms = actual.map(|entry| entry.timing.bpms_formatted.as_str());
            let expected_min = expected.map(|entry| entry.bpm_min);
            let actual_min = actual.map(|entry| entry.timing.bpm_min);
            let expected_max = expected.map(|entry| entry.bpm_max);
            let actual_max = actual.map(|entry| entry.timing.bpm_max);
            let expected_display = expected.map(|entry| entry.display_bpm.as_str());
            let actual_display = actual.map(|entry| entry.timing.display_bpm.as_str());
            let expected_display_min = expected.map(|entry| entry.display_bpm_min);
            let actual_display_min = actual.map(|entry| entry.timing.display_bpm_min);
            let expected_display_max = expected.map(|entry| entry.display_bpm_max);
            let actual_display_max = actual.map(|entry| entry.timing.display_bpm_max);

            let hash_matches = expected_hash.is_some() && expected_hash == actual_hash;
            let bpms_matches = expected_bpms.is_some() && expected_bpms == actual_bpms;
            let min_matches = expected_min.is_some() && expected_min == actual_min;
            let max_matches = expected_max.is_some() && expected_max == actual_max;
            let display_matches = expected_display.is_some() && expected_display == actual_display;
            let display_min_matches =
                expected_display_min.is_some() && expected_display_min == actual_display_min;
            let display_max_matches =
                expected_display_max.is_some() && expected_display_max == actual_display_max;
            let status = if hash_matches
                && bpms_matches
                && min_matches
                && max_matches
                && display_matches
                && display_min_matches
                && display_max_matches
            {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: hash_bpms: {} -> {} | bpms: {} -> {} | bpm_min: {} -> {} | bpm_max: {} -> {} | display_bpm: {} -> {} | display_min: {} -> {} | display_max: {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_hash.unwrap_or("-"),
                actual_hash.unwrap_or("-"),
                expected_bpms.unwrap_or("-"),
                actual_bpms.unwrap_or("-"),
                expected_min
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_min
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                expected_max
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_max
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                expected_display.unwrap_or("-"),
                actual_display.unwrap_or("-"),
                expected_display_min
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_display_min
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                expected_display_max
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_display_max
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = &harness_charts[*expected_idx];
                    let actual = &actual_charts[*actual_idx];
                    expected.hash_bpms == actual.timing.hash_bpms.clone().unwrap_or_default()
                        && expected.bpms == actual.timing.bpms_formatted
                        && expected.bpm_min == actual.timing.bpm_min
                        && expected.bpm_max == actual.timing.bpm_max
                        && expected.display_bpm == actual.timing.display_bpm
                        && expected.display_bpm_min == actual.timing.display_bpm_min
                        && expected.display_bpm_max == actual.timing.display_bpm_max
                });
        if !matches {
            let expected_hashes: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].hash_bpms.clone())
                .collect();
            let actual_hashes: Vec<String> = actual_indices
                .iter()
                .map(|&i| {
                    actual_charts[i]
                        .timing
                        .hash_bpms
                        .clone()
                        .unwrap_or_default()
                })
                .collect();
            let expected_bpms: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].bpms.clone())
                .collect();
            let actual_bpms: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.bpms_formatted.clone())
                .collect();
            let expected_mins: Vec<f64> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].bpm_min)
                .collect();
            let actual_mins: Vec<f64> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.bpm_min)
                .collect();
            let expected_maxes: Vec<f64> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].bpm_max)
                .collect();
            let actual_maxes: Vec<f64> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.bpm_max)
                .collect();
            let expected_display: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].display_bpm.clone())
                .collect();
            let actual_display: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.display_bpm.clone())
                .collect();
            let expected_display_mins: Vec<f64> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].display_bpm_min)
                .collect();
            let actual_display_mins: Vec<f64> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.display_bpm_min)
                .collect();
            let expected_display_maxes: Vec<f64> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].display_bpm_max)
                .collect();
            let actual_display_maxes: Vec<f64> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].timing.display_bpm_max)
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP hash_bpms:   {:?}\nGolden hash_bpms: {:?}\nRSSP bpms:        {:?}\nGolden bpms:      {:?}\nRSSP bpm_min:     {:?}\nGolden bpm_min:   {:?}\nRSSP bpm_max:     {:?}\nGolden bpm_max:   {:?}\nRSSP display_bpm:     {:?}\nGolden display_bpm:   {:?}\nRSSP display_min:     {:?}\nGolden display_min:   {:?}\nRSSP display_max:     {:?}\nGolden display_max:   {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_hashes,
                expected_hashes,
                actual_bpms,
                expected_bpms,
                actual_mins,
                expected_mins,
                actual_maxes,
                expected_maxes,
                actual_display,
                expected_display,
                actual_display_mins,
                expected_display_mins,
                actual_display_maxes,
                expected_display_maxes
            ));
        }
    }

    Ok(())
}

fn compare_hashes(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());
            let expected_hash = expected.map(|entry| entry.hash.as_str());
            let actual_hash = actual.map(|entry| entry.chart_info.sha1.as_str());
            let status = if expected_hash.is_some() && expected_hash == actual_hash {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: baseline: {} -> rssp: {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_hash.unwrap_or("-"),
                actual_hash.unwrap_or("-"),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    harness_charts[*expected_idx].hash == actual_charts[*actual_idx].chart_info.sha1
                });
        if !matches {
            let expected_hashes: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].hash.clone())
                .collect();
            let actual_hashes: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].chart_info.sha1.clone())
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP Hashes:   {:?}\nGolden Hashes: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_hashes,
                expected_hashes
            ));
        }
    }

    Ok(())
}

fn compare_durations(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_val = expected.map(|e| round_millis(e.duration_seconds));
            let actual_val = actual
                .and_then(|a| a.timing.duration_seconds)
                .map(round_millis);
            let status = if expected_val.is_some() && expected_val == actual_val {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: duration_seconds {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_val
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_val
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = round_millis(harness_charts[*expected_idx].duration_seconds);
                    let actual = actual_charts[*actual_idx]
                        .timing
                        .duration_seconds
                        .map(round_millis)
                        .unwrap_or_default();
                    expected == actual
                });
        if !matches {
            let expected_vals: Vec<f64> = expected_indices
                .iter()
                .map(|&i| round_millis(harness_charts[i].duration_seconds))
                .collect();
            let actual_vals: Vec<f64> = actual_indices
                .iter()
                .map(|&i| {
                    actual_charts[i]
                        .timing
                        .duration_seconds
                        .map(round_millis)
                        .unwrap_or_default()
                })
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP duration_seconds:   {:?}\nGolden duration_seconds: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_vals,
                expected_vals
            ));
        }
    }

    Ok(())
}

fn compare_timing(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_timing = expected.and_then(|entry| entry.timing.as_ref());
            let actual_timing = actual.map(|entry| &entry.timing);
            let matches = match (expected_timing, actual_timing) {
                (Some(exp), Some(act)) => timing_matches(exp, act),
                _ => false,
            };
            let status = if matches { "....ok" } else { "....MISMATCH" };

            println!(
                "  {} {} [{}]: timing {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_timing.map_or_else(|| "-".to_string(), timing_counts_expected),
                actual_timing.map_or_else(|| "-".to_string(), timing_counts_actual),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let Some(expected_timing) = harness_charts[*expected_idx].timing.as_ref()
                    else {
                        return false;
                    };
                    timing_matches(expected_timing, &actual_charts[*actual_idx].timing)
                });
        if !matches {
            let expected_values: Vec<&HarnessTiming> = expected_indices
                .iter()
                .filter_map(|&i| harness_charts[i].timing.as_ref())
                .collect();
            let actual_values: Vec<&RsspTiming> = actual_indices
                .iter()
                .map(|&i| &actual_charts[i].timing)
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP timing:   {:?}\nGolden timing: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_values,
                expected_values
            ));
        }
    }

    Ok(())
}

fn compare_nps(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_peak = expected.map(|e| e.peak_nps);
            let actual_peak = actual.map(|a| a.nps.max_nps);
            let expected_notes = expected.map(|e| e.notes_per_measure.as_slice());
            let actual_notes = actual.map(|a| a.nps.notes_per_measure.as_slice());
            let notes_match = match (expected_notes, actual_notes) {
                (Some(exp), Some(act)) => exp == act,
                _ => false,
            };
            let expected_nps = expected.map(|e| e.nps_per_measure.as_slice());
            let actual_nps = actual.map(|a| a.nps.nps_per_measure.as_slice());
            let nps_match = match (expected_nps, actual_nps) {
                (Some(exp), Some(act)) => exp == act,
                _ => false,
            };
            let expected_spaced = expected.map(|e| e.equally_spaced_per_measure.as_slice());
            let actual_spaced = actual.map(|a| a.nps.equally_spaced_per_measure.as_slice());
            let spaced_match = match (expected_spaced, actual_spaced) {
                (Some(exp), Some(act)) => exp == act,
                _ => false,
            };
            let status = if expected_peak.is_some()
                && expected_peak == actual_peak
                && notes_match
                && nps_match
                && spaced_match
            {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: peak_nps {} -> {} | notes_per_measure len {} -> {} | nps_per_measure len {} -> {} | equally_spaced len {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_peak
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                actual_peak
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                format_len(expected_notes),
                format_len(actual_notes),
                format_len(expected_nps),
                format_len(actual_nps),
                format_len(expected_spaced),
                format_len(actual_spaced),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = &harness_charts[*expected_idx];
                    let actual = &actual_charts[*actual_idx];
                    expected.peak_nps == actual.nps.max_nps
                        && expected.notes_per_measure == actual.nps.notes_per_measure
                        && expected.nps_per_measure == actual.nps.nps_per_measure
                        && expected.equally_spaced_per_measure
                            == actual.nps.equally_spaced_per_measure
                });
        if !matches {
            let expected_vals: Vec<f64> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].peak_nps)
                .collect();
            let actual_vals: Vec<f64> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].nps.max_nps)
                .collect();
            let expected_notes: Vec<Vec<u32>> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].notes_per_measure.clone())
                .collect();
            let actual_notes: Vec<Vec<u32>> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].nps.notes_per_measure.clone())
                .collect();
            let expected_nps: Vec<Vec<f64>> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].nps_per_measure.clone())
                .collect();
            let actual_nps: Vec<Vec<f64>> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].nps.nps_per_measure.clone())
                .collect();
            let expected_spaced: Vec<Vec<bool>> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].equally_spaced_per_measure.clone())
                .collect();
            let actual_spaced: Vec<Vec<bool>> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].nps.equally_spaced_per_measure.clone())
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP peak_nps:   {:?}\nGolden peak_nps: {:?}\nRSSP notes_per_measure:   {:?}\nGolden notes_per_measure: {:?}\nRSSP nps_per_measure:     {:?}\nGolden nps_per_measure:   {:?}\nRSSP equally_spaced_per_measure:   {:?}\nGolden equally_spaced_per_measure: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_vals,
                expected_vals,
                actual_notes,
                expected_notes,
                actual_nps,
                expected_nps,
                actual_spaced,
                expected_spaced
            ));
        }
    }

    Ok(())
}

fn compare_step_counts(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let mut all_match = true;
            let mut field = |label: &str, expected: Option<u32>, actual: Option<u32>| -> String {
                let status = if expected.is_some() && expected == actual {
                    "ok"
                } else {
                    all_match = false;
                    "MISMATCH"
                };
                format!(
                    "{} {} -> {} {}",
                    label,
                    format_count(expected),
                    format_count(actual),
                    status
                )
            };

            let holds = field(
                "holds",
                expected.map(|e| e.holds),
                actual.map(|a| a.arrow_stats.holds),
            );
            let mines = field(
                "mines",
                expected.map(|e| e.mines),
                actual.map(|a| a.arrow_stats.mines),
            );
            let rolls = field(
                "rolls",
                expected.map(|e| e.rolls),
                actual.map(|a| a.arrow_stats.rolls),
            );
            let notes = field(
                "notes",
                expected.map(|e| e.notes),
                actual.map(|a| a.arrow_stats.total_arrows),
            );
            let lifts = field(
                "lifts",
                expected.map(|e| e.lifts),
                actual.map(|a| a.gimmicks.lifts),
            );
            let fakes = field(
                "fakes",
                expected.map(|e| e.fakes),
                actual.map(|a| a.gimmicks.fakes),
            );
            let jumps = field(
                "jumps",
                expected.map(|e| e.jumps),
                actual.map(|a| a.arrow_stats.jumps),
            );
            let hands = field(
                "hands",
                expected.map(|e| e.hands),
                actual.map(|a| a.arrow_stats.hands),
            );
            let total_steps = field(
                "total_steps",
                expected.map(|e| e.total_steps),
                actual.map(|a| a.arrow_stats.total_steps),
            );
            let status = if all_match { "....ok" } else { "....MISMATCH" };

            println!(
                "  {} {} [{}]: {} | {} | {} | {} | {} | {} | {} | {} | {} {}",
                step_type,
                difficulty,
                meter_label,
                holds,
                mines,
                rolls,
                notes,
                lifts,
                fakes,
                jumps,
                hands,
                total_steps,
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = &harness_charts[*expected_idx];
                    let actual = &actual_charts[*actual_idx];
                    expected.holds == actual.arrow_stats.holds
                        && expected.mines == actual.arrow_stats.mines
                        && expected.rolls == actual.arrow_stats.rolls
                        && expected.notes == actual.arrow_stats.total_arrows
                        && expected.lifts == actual.gimmicks.lifts
                        && expected.fakes == actual.gimmicks.fakes
                        && expected.jumps == actual.arrow_stats.jumps
                        && expected.hands == actual.arrow_stats.hands
                        && expected.total_steps == actual.arrow_stats.total_steps
                });
        if !matches {
            let expected_holds: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].holds)
                .collect();
            let actual_holds: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.holds)
                .collect();
            let expected_mines: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].mines)
                .collect();
            let actual_mines: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.mines)
                .collect();
            let expected_rolls: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].rolls)
                .collect();
            let actual_rolls: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.rolls)
                .collect();
            let expected_notes: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].notes)
                .collect();
            let actual_notes: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.total_arrows)
                .collect();
            let expected_lifts: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].lifts)
                .collect();
            let actual_lifts: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].gimmicks.lifts)
                .collect();
            let expected_fakes: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].fakes)
                .collect();
            let actual_fakes: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].gimmicks.fakes)
                .collect();
            let expected_jumps: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].jumps)
                .collect();
            let actual_jumps: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.jumps)
                .collect();
            let expected_hands: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].hands)
                .collect();
            let actual_hands: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.hands)
                .collect();
            let expected_total_steps: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].total_steps)
                .collect();
            let actual_total_steps: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].arrow_stats.total_steps)
                .collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP notes:      {:?}\nGolden notes:    {:?}\nRSSP total_steps: {:?}\nGolden total_steps: {:?}\nRSSP holds:      {:?}\nGolden holds:    {:?}\nRSSP mines:      {:?}\nGolden mines:    {:?}\nRSSP rolls:      {:?}\nGolden rolls:    {:?}\nRSSP lifts:      {:?}\nGolden lifts:    {:?}\nRSSP fakes:      {:?}\nGolden fakes:    {:?}\nRSSP jumps:      {:?}\nGolden jumps:    {:?}\nRSSP hands:      {:?}\nGolden hands:    {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_notes,
                expected_notes,
                actual_total_steps,
                expected_total_steps,
                actual_holds,
                expected_holds,
                actual_mines,
                expected_mines,
                actual_rolls,
                expected_rolls,
                actual_lifts,
                expected_lifts,
                actual_fakes,
                expected_fakes,
                actual_jumps,
                expected_jumps,
                actual_hands,
                expected_hands
            ));
        }
    }

    Ok(())
}

fn compare_stream_breakdown(
    path: &Path,
    harness_entries: &[((String, String), Vec<usize>)],
    harness_charts: &[HarnessChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in harness_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &harness_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_detail = expected
                .map(|v| v.streams_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual
                .map(|v| v.stream_breakdown.detailed_breakdown.as_str())
                .unwrap_or("-");
            let expected_partial = expected
                .map(|v| v.streams_breakdown_level1.as_str())
                .unwrap_or("-");
            let actual_partial = actual
                .map(|v| v.stream_breakdown.partial_breakdown.as_str())
                .unwrap_or("-");
            let expected_simple = expected
                .map(|v| v.streams_breakdown_level2.as_str())
                .unwrap_or("-");
            let actual_simple = actual
                .map(|v| v.stream_breakdown.simple_breakdown.as_str())
                .unwrap_or("-");
            let expected_total_streams = expected.map(|v| v.total_stream_measures);
            let actual_total_streams = actual.map(|v| v.stream_info.total_streams);
            let expected_total_breaks = expected.map(|v| v.total_break_measures);
            let actual_total_breaks = actual.map(|v| v.stream_info.total_breaks);
            let expected_sequences = expected.map(|v| v.stream_sequences.as_slice());
            let actual_sequences = actual.map(|v| v.stream_info.stream_sequences.as_slice());
            let sequences_match = match (expected_sequences, actual_sequences) {
                (Some(exp), Some(act)) => exp == act,
                _ => false,
            };

            let matches = expected.is_some()
                && actual.is_some()
                && expected_detail == actual_detail
                && expected_partial == actual_partial
                && expected_simple == actual_simple
                && expected_total_streams == actual_total_streams
                && expected_total_breaks == actual_total_breaks
                && sequences_match;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_total_streams = expected_total_streams
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_total_streams = actual_total_streams
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let expected_total_breaks = expected_total_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_total_breaks = actual_total_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());

            println!(
                "  {} {} [{}]: detailed {} -> {} | partial {} -> {} | simple {} -> {} | total_streams {} -> {} | total_breaks {} -> {} | sequences len {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_total_streams,
                actual_total_streams,
                expected_total_breaks,
                actual_total_breaks,
                format_len(expected_sequences),
                format_len(actual_sequences),
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = &harness_charts[*expected_idx];
                    let actual = &actual_charts[*actual_idx];
                    expected.streams_breakdown == actual.stream_breakdown.detailed_breakdown
                        && expected.streams_breakdown_level1
                            == actual.stream_breakdown.partial_breakdown
                        && expected.streams_breakdown_level2
                            == actual.stream_breakdown.simple_breakdown
                        && expected.total_stream_measures == actual.stream_info.total_streams
                        && expected.total_break_measures == actual.stream_info.total_breaks
                        && expected.stream_sequences == actual.stream_info.stream_sequences
                });
        if !matches {
            let expected_detail: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].streams_breakdown.clone())
                .collect();
            let actual_detail: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_breakdown.detailed_breakdown.clone())
                .collect();
            let expected_partial: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].streams_breakdown_level1.clone())
                .collect();
            let actual_partial: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_breakdown.partial_breakdown.clone())
                .collect();
            let expected_simple: Vec<String> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].streams_breakdown_level2.clone())
                .collect();
            let actual_simple: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_breakdown.simple_breakdown.clone())
                .collect();
            let expected_total_streams: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].total_stream_measures)
                .collect();
            let actual_total_streams: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_info.total_streams)
                .collect();
            let expected_total_breaks: Vec<u32> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].total_break_measures)
                .collect();
            let actual_total_breaks: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_info.total_breaks)
                .collect();
            let expected_sequences: Vec<Vec<StreamSequence>> = expected_indices
                .iter()
                .map(|&i| harness_charts[i].stream_sequences.clone())
                .collect();
            let actual_sequences: Vec<Vec<StreamSequence>> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_info.stream_sequences.clone())
                .collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP detailed: {:?}\nGolden detailed: {:?}\nRSSP partial: {:?}\nGolden partial: {:?}\nRSSP simple: {:?}\nGolden simple: {:?}\nRSSP total_streams: {:?}\nGolden total_streams: {:?}\nRSSP total_breaks: {:?}\nGolden total_breaks: {:?}\nRSSP stream_sequences: {:?}\nGolden stream_sequences: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_detail,
                expected_detail,
                actual_partial,
                expected_partial,
                actual_simple,
                expected_simple,
                actual_total_streams,
                expected_total_streams,
                actual_total_breaks,
                expected_total_breaks,
                actual_sequences,
                expected_sequences
            ));
        }
    }

    Ok(())
}

fn compare_sn_breakdown(
    path: &Path,
    rssp_entries: &[((String, String), Vec<usize>)],
    rssp_charts: &[RsspBaselineChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    for ((step_type, difficulty), expected_indices) in rssp_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &rssp_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);
            let meter_label = expected
                .map(|entry| entry.chart_info.rating.as_str())
                .filter(|label| !label.is_empty())
                .or_else(|| {
                    actual
                        .map(|entry| entry.chart_info.rating.as_str())
                        .filter(|label| !label.is_empty())
                })
                .map(|label| label.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_detail = expected
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let expected_partial = expected
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let actual_partial = actual
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let expected_simple = expected
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let actual_simple = actual
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let expected_sn_breaks = expected.map(|v| v.stream_info.sn_breaks);
            let actual_sn_breaks = actual.map(|v| v.stream_info.sn_breaks);

            let matches = expected.is_some()
                && actual.is_some()
                && expected_detail == actual_detail
                && expected_partial == actual_partial
                && expected_simple == actual_simple
                && expected_sn_breaks == actual_sn_breaks;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_sn_breaks = expected_sn_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_sn_breaks = actual_sn_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());

            println!(
                "  {} {} [{}]: sn_detailed {} -> {} | sn_partial {} -> {} | sn_simple {} -> {} | sn_breaks {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_sn_breaks,
                actual_sn_breaks,
                status
            );
        }

        let matches = expected_indices.len() == actual_indices.len()
            && expected_indices
                .iter()
                .zip(actual_indices)
                .all(|(expected_idx, actual_idx)| {
                    let expected = &rssp_charts[*expected_idx];
                    let actual = &actual_charts[*actual_idx];
                    expected.breakdown == actual.breakdown
                        && expected.stream_info.sn_breaks == actual.stream_info.sn_breaks
                });
        if !matches {
            let expected_detail: Vec<String> = expected_indices
                .iter()
                .map(|&i| rssp_charts[i].breakdown.sn_detailed_breakdown.clone())
                .collect();
            let actual_detail: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].breakdown.sn_detailed_breakdown.clone())
                .collect();
            let expected_partial: Vec<String> = expected_indices
                .iter()
                .map(|&i| rssp_charts[i].breakdown.sn_partial_breakdown.clone())
                .collect();
            let actual_partial: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].breakdown.sn_partial_breakdown.clone())
                .collect();
            let expected_simple: Vec<String> = expected_indices
                .iter()
                .map(|&i| rssp_charts[i].breakdown.sn_simple_breakdown.clone())
                .collect();
            let actual_simple: Vec<String> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].breakdown.sn_simple_breakdown.clone())
                .collect();
            let expected_sn_breaks: Vec<u32> = expected_indices
                .iter()
                .map(|&i| rssp_charts[i].stream_info.sn_breaks)
                .collect();
            let actual_sn_breaks: Vec<u32> = actual_indices
                .iter()
                .map(|&i| actual_charts[i].stream_info.sn_breaks)
                .collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP sn_detailed: {:?}\nGolden sn_detailed: {:?}\nRSSP sn_partial: {:?}\nGolden sn_partial: {:?}\nRSSP sn_simple: {:?}\nGolden sn_simple: {:?}\nRSSP sn_breaks: {:?}\nGolden sn_breaks: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_detail,
                expected_detail,
                actual_partial,
                expected_partial,
                actual_simple,
                expected_simple,
                actual_sn_breaks,
                expected_sn_breaks
            ));
        }
    }

    Ok(())
}

fn format_candles(stats: Option<&MonoCandleStats>) -> String {
    stats
        .map(|s| {
            format!(
                "{} (L {} R {}) {}%",
                s.total_candles, s.left_foot_candles, s.right_foot_candles, s.candles_percent
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_mono(stats: Option<&MonoCandleStats>) -> String {
    stats
        .map(|s| {
            format!(
                "{} (L {} R {}) {}%",
                s.total_mono, s.left_face_mono, s.right_face_mono, s.mono_percent
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_boxes(patterns: Option<&RsspPatternCounts>) -> String {
    patterns
        .map(|p| {
            let b = &p.boxes;
            format!(
                "{} (LR {} UD {} LD {} LU {} RD {} RU {})",
                b.total_boxes,
                b.lr_boxes,
                b.ud_boxes,
                b.ld_boxes,
                b.lu_boxes,
                b.rd_boxes,
                b.ru_boxes
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_anchors(patterns: Option<&RsspPatternCounts>) -> String {
    patterns
        .map(|p| {
            let a = &p.anchors;
            format!(
                "{} (L {} D {} U {} R {})",
                a.total_anchors, a.left_anchors, a.down_anchors, a.up_anchors, a.right_anchors
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn build_mono_stats(mono: &RsspMonoCandleStats) -> MonoCandleStats {
    MonoCandleStats {
        total_candles: mono.total_candles,
        left_foot_candles: mono.left_foot_candles,
        right_foot_candles: mono.right_foot_candles,
        candles_percent: format_json_float(mono.candles_percent),
        total_mono: mono.total_mono,
        left_face_mono: mono.left_face_mono,
        right_face_mono: mono.right_face_mono,
        mono_percent: format_json_float(mono.mono_percent),
    }
}

fn unique_from_rssp(chart: &RsspBaselineChart, include_patterns: bool) -> ChartUniqueValues {
    let mono_candle_stats = if include_patterns {
        chart.mono_candle_stats.as_ref().map(build_mono_stats)
    } else {
        None
    };
    let pattern_counts = if include_patterns {
        chart.pattern_counts.clone()
    } else {
        None
    };

    ChartUniqueValues {
        matrix_rating: format_json_float(chart.chart_info.matrix_rating),
        breakdown: chart.breakdown.clone(),
        mono_candle_stats,
        pattern_counts,
    }
}

fn unique_from_actual(chart: &RsspJsonChart, include_patterns: bool) -> ChartUniqueValues {
    let mono_candle_stats = if include_patterns {
        chart.mono_candle_stats.as_ref().map(build_mono_stats)
    } else {
        None
    };
    let pattern_counts = if include_patterns {
        chart.pattern_counts.clone()
    } else {
        None
    };

    ChartUniqueValues {
        matrix_rating: format_json_float(chart.chart_info.matrix_rating),
        breakdown: chart.breakdown.clone(),
        mono_candle_stats,
        pattern_counts,
    }
}

fn compare_rssp_unique(
    path: &Path,
    rssp_entries: &[((String, String), Vec<usize>)],
    rssp_charts: &[RsspBaselineChart],
    actual_map: &HashMap<(String, String), Vec<usize>>,
    actual_charts: &[RsspJsonChart],
) -> Result<(), String> {
    let compare_patterns = actual_charts
        .iter()
        .any(|chart| chart.mono_candle_stats.is_some() || chart.pattern_counts.is_some());

    for ((step_type, difficulty), expected_indices) in rssp_entries {
        let Some(actual_indices) = actual_map.get(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {} {}: baseline present, RSSP missing chart",
                step_type, difficulty
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_indices.len().max(actual_indices.len());
        for idx in 0..count {
            let expected = expected_indices.get(idx).map(|&i| &rssp_charts[i]);
            let actual = actual_indices.get(idx).map(|&i| &actual_charts[i]);

            let meter_label = expected
                .map(|entry| entry.chart_info.rating.as_str())
                .filter(|label| !label.is_empty())
                .or_else(|| {
                    actual
                        .map(|entry| entry.chart_info.rating.as_str())
                        .filter(|label| !label.is_empty())
                })
                .map(|label| label.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_values = expected.map(|entry| unique_from_rssp(entry, compare_patterns));
            let actual_values = actual.map(|entry| unique_from_actual(entry, compare_patterns));
            let matches = expected_values.is_some()
                && actual_values.is_some()
                && expected_values == actual_values;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_matrix = expected_values
                .as_ref()
                .map(|v| v.matrix_rating.as_str())
                .unwrap_or("-");
            let actual_matrix = actual_values
                .as_ref()
                .map(|v| v.matrix_rating.as_str())
                .unwrap_or("-");
            let expected_detail = expected_values
                .as_ref()
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual_values
                .as_ref()
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let expected_partial = expected_values
                .as_ref()
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let actual_partial = actual_values
                .as_ref()
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let expected_simple = expected_values
                .as_ref()
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let actual_simple = actual_values
                .as_ref()
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let expected_candles = format_candles(
                expected_values
                    .as_ref()
                    .and_then(|v| v.mono_candle_stats.as_ref()),
            );
            let actual_candles = format_candles(
                actual_values
                    .as_ref()
                    .and_then(|v| v.mono_candle_stats.as_ref()),
            );
            let expected_mono = format_mono(
                expected_values
                    .as_ref()
                    .and_then(|v| v.mono_candle_stats.as_ref()),
            );
            let actual_mono = format_mono(
                actual_values
                    .as_ref()
                    .and_then(|v| v.mono_candle_stats.as_ref()),
            );
            let expected_boxes = format_boxes(
                expected_values
                    .as_ref()
                    .and_then(|v| v.pattern_counts.as_ref()),
            );
            let actual_boxes = format_boxes(
                actual_values
                    .as_ref()
                    .and_then(|v| v.pattern_counts.as_ref()),
            );
            let expected_anchors = format_anchors(
                expected_values
                    .as_ref()
                    .and_then(|v| v.pattern_counts.as_ref()),
            );
            let actual_anchors = format_anchors(
                actual_values
                    .as_ref()
                    .and_then(|v| v.pattern_counts.as_ref()),
            );

            println!(
                "  {} {} [{}]: matrix_rating {} -> {} | detailed {} -> {} | partial {} -> {} | simple {} -> {} | candles {} -> {} | mono {} -> {} | boxes {} -> {} | anchors {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_matrix,
                actual_matrix,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_candles,
                actual_candles,
                expected_mono,
                actual_mono,
                expected_boxes,
                actual_boxes,
                expected_anchors,
                actual_anchors,
                status
            );
        }

        let expected_entries: Vec<ChartUniqueValues> = expected_indices
            .iter()
            .map(|&i| unique_from_rssp(&rssp_charts[i], compare_patterns))
            .collect();
        let actual_entries: Vec<ChartUniqueValues> = actual_indices
            .iter()
            .map(|&i| unique_from_actual(&actual_charts[i], compare_patterns))
            .collect();
        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries
                .iter()
                .zip(&actual_entries)
                .all(|(expected, actual)| expected == actual);
        if !matches {
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP values:   {:?}\nGolden values: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_entries,
                expected_entries
            ));
        }
    }

    Ok(())
}

fn run_rssp_json(
    bin_path: &Path,
    raw_bytes: &[u8],
    extension: &str,
    file_hash: &str,
) -> Result<RsspJsonFile, String> {
    let pid = std::process::id();
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push(format!("rssp_fast_all_{}_{}.{}", pid, file_hash, extension));

    fs::write(&tmp_path, raw_bytes).map_err(|e| format!("Failed to write temp simfile: {}", e))?;

    let output = Command::new(bin_path)
        .arg(&tmp_path)
        .args(RSSP_ARGS)
        .output();

    let _ = fs::remove_file(&tmp_path);

    let output = output.map_err(|e| format!("Failed to run rssp: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "rssp failed: exit={} stderr={}",
            output.status, stderr
        ));
    }

    serde_json::from_slice(&output.stdout).map_err(|e| format!("Failed to parse rssp JSON: {}", e))
}

fn read_zst(path: &Path) -> Result<Vec<u8>, String> {
    let compressed = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    zstd::decode_all(&compressed[..]).map_err(|e| format!("Failed to decompress file: {}", e))
}

fn resolve_rssp_bin() -> Result<PathBuf, String> {
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_rssp") {
        return Ok(PathBuf::from(bin));
    }
    if let Some(bin) = option_env!("CARGO_BIN_EXE_rssp") {
        return Ok(PathBuf::from(bin));
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let exe_name = if cfg!(windows) { "rssp.exe" } else { "rssp" };
    let candidate = manifest_dir.join(target_dir).join(profile).join(exe_name);

    if candidate.is_file() {
        return Ok(candidate);
    }

    Err(format!(
        "CARGO_BIN_EXE_rssp is not set and {} does not exist; run `cargo build --release --bin rssp` or set CARGO_BIN_EXE_rssp",
        candidate.display()
    ))
}

fn check_file(
    path: &Path,
    extension: &str,
    baseline_dir: &Path,
    rssp_bin: &Path,
) -> Result<(), String> {
    let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {}", e))?;

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];

    let harness_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.json.zst", file_hash));
    if !harness_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            harness_path.display()
        ));
    }

    let rssp_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.rssp.json.zst", file_hash));
    if !rssp_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            rssp_path.display()
        ));
    }

    let harness_json = read_zst(&harness_path)?;
    let harness_charts: Vec<HarnessChart> = serde_json::from_slice(&harness_json)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_json = read_zst(&rssp_path)?;
    let rssp_file: RsspBaselineFile = serde_json::from_slice(&rssp_json)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let actual = run_rssp_json(rssp_bin, &raw_bytes, extension, &file_hash)?;

    let harness_map = build_index(&harness_charts, |c| &c.step_type, |c| &c.difficulty);
    let actual_map = build_index(
        &actual.charts,
        |c| &c.chart_info.step_type,
        |c| &c.chart_info.difficulty,
    );
    let rssp_map = build_index(
        &rssp_file.charts,
        |c| &c.chart_info.step_type,
        |c| &c.chart_info.difficulty,
    );

    let harness_entries = sorted_entries(harness_map);
    let rssp_entries = sorted_entries(rssp_map);

    println!("File: {}", path.display());

    let expected = expected_metadata(&harness_charts, path)?;
    let actual_metadata = parse_metadata(&actual);
    compare_metadata(path, &expected, &actual_metadata)?;
    compare_step_artists(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_bpm(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_hashes(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_durations(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_timing(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_nps(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_step_counts(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_stream_breakdown(
        path,
        &harness_entries,
        &harness_charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_sn_breakdown(
        path,
        &rssp_entries,
        &rssp_file.charts,
        &actual_map,
        &actual.charts,
    )?;
    compare_rssp_unique(
        path,
        &rssp_entries,
        &rssp_file.charts,
        &actual_map,
        &actual.charts,
    )?;

    Ok(())
}

fn main() {
    let args = Arguments::from_args();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = manifest_dir.join("tests/data/packs");
    let baseline_dir = manifest_dir.join("tests/data/baseline");

    let rssp_bin = match resolve_rssp_bin() {
        Ok(path) => path,
        Err(msg) => {
            println!("{}", msg);
            return;
        }
    };

    if !packs_dir.exists() {
        println!("No tests/packs directory found.");
        return;
    }

    let mut tests = Vec::new();

    for entry in WalkDir::new(&packs_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "zst" {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let inner_path = Path::new(stem);
        let inner_extension = inner_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if inner_extension != "sm" && inner_extension != "ssc" {
            continue;
        }

        let test_name = path
            .strip_prefix(&packs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        tests.push(TestCase {
            name: test_name,
            path: path.to_path_buf(),
            extension: inner_extension,
        });
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));

    let mut tests: Vec<_> = tests
        .into_iter()
        .filter(|t| match &args.filter {
            None => true,
            Some(filter) => {
                if args.exact {
                    &t.name == filter
                } else {
                    t.name.contains(filter)
                }
            }
        })
        .filter(|t| args.skip.iter().all(|skip| !t.name.contains(skip)))
        .collect();

    if args.ignored {
        tests.clear();
    }

    if args.list {
        for t in &tests {
            println!("{}", t.name);
        }
        return;
    }

    println!("running {} tests", tests.len());

    let mut num_passed = 0u64;
    let mut num_failed = 0u64;
    let mut failures: Vec<Failure> = Vec::new();

    for test in tests {
        let TestCase {
            name,
            path,
            extension,
        } = test;

        let res = check_file(&path, &extension, &baseline_dir, &rssp_bin);
        match res {
            Ok(()) => {
                println!("test {} ... ok", name);
                num_passed += 1;
            }
            Err(msg) => {
                println!("test {} ... FAILED", name);
                failures.push(Failure {
                    name,
                    message: msg.trim().to_string(),
                });
                num_failed += 1;
            }
        }

        let _ = io::stdout().flush();
    }

    println!();
    if !failures.is_empty() {
        println!("failures:");
        for failure in &failures {
            println!("    {}", failure.name);
        }

        for failure in &failures {
            println!();
            println!("---- {} ----", failure.name);
            if !failure.message.is_empty() {
                println!("{}", failure.message);
            }
            println!();
            println!(
                "rerun: cargo test --test fast_all_parity -- --exact {:?}",
                failure.name
            );
        }
        println!();
    }

    if num_failed == 0 {
        println!("test result: ok. {} passed; 0 failed", num_passed);
        return;
    }

    println!(
        "test result: FAILED. {} passed; {} failed",
        num_passed, num_failed
    );
    std::process::exit(101);
}

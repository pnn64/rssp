use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use libtest_mimic::Arguments;
use rssp::parse::extension_is_ssc;
use walkdir::WalkDir;

use rssp::report::{TimingSnapshot, build_timing_snapshot};
use rssp::serialize::*;
use rssp::stats::RADAR_CATEGORY_COUNT;
use rssp::{AnalysisOptions, ChartSummary, SimfileSummary, analyze};

pub const NORM_DEFAULT_BPMS: &[u8] = b"0.000=60.000";
pub const NORM_DEFAULT_SPEEDS: &[u8] = b"0.000=1.000=0.000=0";
pub const NORM_DEFAULT_SCROLLS: &[u8] = b"0.000=1.000";

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

const TIMING_EPS: f64 = 1e-3;

fn timing_approx_eq(a: &f64, b: &f64) -> bool {
    (a - b).abs() <= TIMING_EPS
}

macro_rules! timing_matches_entry {
    ($field: ident, $comparator: expr, $expected: expr, $actual: expr) => {
        (
            String::from(stringify!($field)),
            $comparator(&$expected.$field, &$actual.$field),
        )
    };
}

fn build_timing_comparison_table(
    expected: &TimingSnapshot,
    actual: &TimingSnapshot,
) -> Vec<(String, bool)> {
    vec![
        timing_matches_entry!(beat0_offset_seconds, timing_approx_eq, expected, actual),
        timing_matches_entry!(bpms, compare_pairs, expected, actual),
        timing_matches_entry!(stops, compare_pairs, expected, actual),
        timing_matches_entry!(delays, compare_pairs, expected, actual),
        timing_matches_entry!(warps, compare_pairs, expected, actual),
        timing_matches_entry!(scrolls, compare_pairs, expected, actual),
        timing_matches_entry!(fakes, compare_pairs, expected, actual),
        timing_matches_entry!(time_signatures, compare_time_signatures, expected, actual),
        timing_matches_entry!(labels, compare_labels, expected, actual),
        timing_matches_entry!(tickcounts, compare_tickcounts, expected, actual),
        timing_matches_entry!(combos, compare_combos, expected, actual),
        timing_matches_entry!(speeds, compare_speeds, expected, actual),
    ]
}

fn compare_pairs(expected: &[(f64, f64)], actual: &[(f64, f64)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(&e.0, &a.0) && timing_approx_eq(&e.1, &a.1))
}

fn compare_time_signatures(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(&e.0, &a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_labels(expected: &[(f64, String)], actual: &[(f64, String)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(&e.0, &a.0) && e.1 == a.1)
}

fn compare_tickcounts(expected: &[(f64, i32)], actual: &[(f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(&e.0, &a.0) && e.1 == a.1)
}

fn compare_combos(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| timing_approx_eq(&e.0, &a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_speeds(expected: &[(f64, f64, f64, i32)], actual: &[(f64, f64, f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(e, a)| {
            timing_approx_eq(&e.0, &a.0)
                && timing_approx_eq(&e.1, &a.1)
                && timing_approx_eq(&e.2, &a.2)
                && e.3 == a.3
        })
}

fn compare_radar_values(
    expected_opt: Option<&[f32; RADAR_CATEGORY_COUNT]>,
    actual_opt: Option<&[f32; RADAR_CATEGORY_COUNT]>,
) -> bool {
    match (expected_opt, actual_opt) {
        (None, None) => true,
        (Some(expected), Some(actual)) => {
            for i in 0..RADAR_CATEGORY_COUNT {
                if expected[i] - actual[i] > 1e-6 {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn format_radar_values(radar_values: Option<[f32; RADAR_CATEGORY_COUNT]>) -> String {
    match radar_values {
        Some(radar_values) => radar_values
            .iter()
            .map(|&f| rssp_core::math::fmt_dec6_itg(f as f64))
            .collect::<Vec<String>>()
            .join(","),
        None => String::from(""),
    }
}

macro_rules! build_comparison_entry {
    ($field: ident, $expected: expr, $actual: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            $expected.$field == $actual.$field,
            None,
        )
    };
    ($field: ident, $expected: expr, $actual: expr, $default: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            $expected.$field == $actual.$field
                || ($expected.$field.is_empty() && $actual.$field.as_bytes() == $default),
            Some($default),
        )
    };
}

macro_rules! build_comparison_entry_opt {
    ($field: ident, $expected: expr, $actual: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            $expected.$field == $actual.$field,
            None,
        )
    };
    ($field: ident, $expected: expr, $actual: expr, $default: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            $expected.$field == $actual.$field
                || ($expected.$field.as_ref().is_none_or(|f| f.is_empty())
                    && $actual
                        .$field
                        .as_ref()
                        .is_some_and(|f| f.as_bytes() == $default)),
            Some($default),
        )
    };
}

fn normalized_eq(a: &str, b: &str) -> bool {
    let mut a_lines = a.lines();
    let mut b_lines = b.lines();
    for (line_a, line_b) in a_lines.by_ref().zip(b_lines.by_ref()) {
        if line_a != line_b {
            return false;
        }
    }
    if a_lines.next().is_some() || b_lines.next().is_some() {
        return false;
    }
    true
}

macro_rules! build_normalized_comparison_entry {
    ($field: ident, $expected: expr, $actual: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            normalized_eq(&$expected.$field, &$actual.$field),
            None,
        )
    };
    ($field: ident, $expected: expr, $actual: expr, $default: expr) => {
        (
            stringify!($field),
            &$expected.$field,
            &$actual.$field,
            normalized_eq(&$expected.$field, &$actual.$field)
                || ($expected.$field.is_empty() && $actual.$field.as_bytes() == $default),
            Some($default),
        )
    };
}

fn compare_simfile_str_fields(
    path: &Path,
    extension: &str,
    expected: &SimfileSummary,
    actual: &SimfileSummary,
) -> Result<(), String> {
    let is_ssc = extension_is_ssc(extension).map_err(|e| e.to_string())?;
    let expected_ssc_version_display = format!("{:.2}", expected.ssc_version);
    let actual_ssc_version_display = format!("{:.2}", actual.ssc_version);

    let mut comparison_table: Vec<(&str, &str, &str, bool, Option<&[u8]>)> = vec![
        build_comparison_entry!(title_str, expected, actual, DEFAULT_TITLE),
        build_comparison_entry!(subtitle_str, expected, actual),
        build_comparison_entry!(artist_str, expected, actual, DEFAULT_ARTIST),
        build_comparison_entry!(titletranslit_str, expected, actual),
        build_comparison_entry!(subtitletranslit_str, expected, actual),
        build_comparison_entry!(artisttranslit_str, expected, actual),
        build_comparison_entry!(display_bpm_str, expected, actual),
        build_comparison_entry!(credit_str, expected, actual),
        build_comparison_entry!(genre_str, expected, actual),
        build_comparison_entry!(lyrics_path, expected, actual),
        build_comparison_entry!(music_path, expected, actual),
        build_comparison_entry!(cdtitle_path, expected, actual),
        build_comparison_entry!(background_path, expected, actual),
        build_comparison_entry!(banner_path, expected, actual),
    ];
    if is_ssc {
        comparison_table.extend_from_slice(&[
            build_comparison_entry!(origin_str, expected, actual),
            build_comparison_entry!(discimage_path, expected, actual),
            build_comparison_entry!(cdimage_path, expected, actual),
            build_comparison_entry!(previewvid_path, expected, actual),
            build_comparison_entry!(jacket_path, expected, actual),
            (
                "ssc_version",
                &expected_ssc_version_display,
                &actual_ssc_version_display,
                expected.ssc_version == actual.ssc_version,
                Some(DEFAULT_VERSION),
            ),
        ]);
    }

    let all_ok = comparison_table.iter().all(|entry| entry.3);

    if all_ok {
        return Ok(());
    }

    // Build error string
    let mut buffer = vec![];
    {
        let mut cursor = io::Cursor::new(&mut buffer);
        for (field_name, expected, actual, cmp, _) in comparison_table {
            writeln!(
                &mut cursor,
                "  {}: baseline: {:?} -> reencoded: {:?} {}",
                field_name,
                expected,
                actual,
                match cmp {
                    true => "....ok",
                    false => "....MISMATCH",
                }
            )
            .map_err(|e| e.to_string())?;
        }
    } // Drop cursor

    let err_output = String::from_utf8(buffer).unwrap();

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}",
        path.display(),
        err_output,
    ))
}

fn compare_simfile_normalized_fields(
    path: &Path,
    expected: &SimfileSummary,
    actual: &SimfileSummary,
) -> Result<(), String> {
    let comparison_table: Vec<(&str, &str, &str, bool, Option<&[u8]>)> = vec![
        build_normalized_comparison_entry!(normalized_bpms, expected, actual, NORM_DEFAULT_BPMS),
        build_normalized_comparison_entry!(normalized_stops, expected, actual),
        build_normalized_comparison_entry!(normalized_delays, expected, actual),
        build_normalized_comparison_entry!(
            normalized_speeds,
            expected,
            actual,
            NORM_DEFAULT_SPEEDS
        ),
        build_normalized_comparison_entry!(
            normalized_scrolls,
            expected,
            actual,
            NORM_DEFAULT_SCROLLS
        ),
        build_normalized_comparison_entry!(normalized_fakes, expected, actual),
        build_normalized_comparison_entry!(
            normalized_time_signatures,
            expected,
            actual,
            DEFAULT_TIME_SIGNATURES
        ),
        build_normalized_comparison_entry!(normalized_labels, expected, actual, DEFAULT_LABELS),
        build_normalized_comparison_entry!(
            normalized_tickcounts,
            expected,
            actual,
            DEFAULT_TICKCOUNTS
        ),
        build_normalized_comparison_entry!(normalized_combos, expected, actual, DEFAULT_COMBOS),
        build_normalized_comparison_entry!(normalized_bgchanges, expected, actual),
        build_normalized_comparison_entry!(normalized_fgchanges, expected, actual),
        build_normalized_comparison_entry!(normalized_keysounds, expected, actual),
        build_normalized_comparison_entry!(normalized_attacks, expected, actual),
    ];

    let all_ok = comparison_table.iter().all(|entry| entry.3);

    if all_ok {
        return Ok(());
    }

    // Build error string
    let mut buffer = vec![];
    {
        let mut cursor = io::Cursor::new(&mut buffer);
        for (field_name, expected, actual, cmp, _) in comparison_table {
            writeln!(
                &mut cursor,
                "  {}: baseline: {:?} -> reencoded: {:?} {}",
                field_name,
                expected,
                actual,
                match cmp {
                    true => "....ok",
                    false => "....MISMATCH",
                }
            )
            .map_err(|e| e.to_string())?;
        }
    } // Drop cursor

    let err_output = String::from_utf8(buffer).unwrap();

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}",
        path.display(),
        err_output,
    ))
}

fn compare_chart_str_fields_and_hashes(
    path: &Path,
    expected_charts: &[ChartSummary],
    actual_charts: &[ChartSummary],
) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();

    let count = expected_charts.len().max(actual_charts.len());
    for idx in 0..count {
        let expected_chart_opt = expected_charts.get(idx);
        let actual_chart_opt = actual_charts.get(idx);

        match (expected_chart_opt, actual_chart_opt) {
            (None, None) => panic!(), // Unreachable
            (None, Some(actual_chart)) => {
                errors.push(format!(
                    "Unexpected {} {} chart at index {}",
                    actual_chart.step_type_str, actual_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), None) => {
                errors.push(format!(
                    "Expected {} {} chart at index {}",
                    expected_chart.step_type_str, expected_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), Some(actual_chart)) => {
                let expected_radar_values = format_radar_values(expected_chart.cached_radar_values);
                let actual_radar_values = format_radar_values(actual_chart.cached_radar_values);
                let comparison_table: Vec<(&str, &str, &str, bool, Option<&[u8]>)> = vec![
                    build_comparison_entry!(
                        step_type_str,
                        expected_chart,
                        actual_chart,
                        DEFAULT_STEPSTYPE
                    ),
                    build_comparison_entry!(step_artist_str, expected_chart, actual_chart),
                    build_comparison_entry!(description_str, expected_chart, actual_chart),
                    build_comparison_entry!(chart_name_str, expected_chart, actual_chart),
                    build_comparison_entry!(chart_style_str, expected_chart, actual_chart),
                    build_comparison_entry!(
                        difficulty_str,
                        expected_chart,
                        actual_chart,
                        DEFAULT_DIFFICULTY
                    ),
                    build_comparison_entry!(
                        rating_str,
                        expected_chart,
                        actual_chart,
                        DEFAULT_METER
                    ),
                    build_comparison_entry!(short_hash, expected_chart, actual_chart),
                    build_comparison_entry!(bpm_neutral_hash, expected_chart, actual_chart),
                    (
                        "cached_radar_values",
                        &expected_radar_values,
                        &actual_radar_values,
                        compare_radar_values(
                            expected_chart.cached_radar_values.as_ref(),
                            actual_chart.cached_radar_values.as_ref(),
                        ),
                        None,
                    ),
                ];

                let all_ok = comparison_table.iter().all(|entry| entry.3);

                if all_ok {
                    continue;
                }

                // Build error string
                let mut buffer = vec![];
                {
                    let mut cursor = io::Cursor::new(&mut buffer);
                    for (field_name, expected, actual, cmp, _) in comparison_table {
                        writeln!(
                            &mut cursor,
                            "  {}: baseline: {:?} -> reencoded: {:?} {}",
                            field_name,
                            expected,
                            actual,
                            match cmp {
                                true => "....ok",
                                false => "....MISMATCH",
                            }
                        )
                        .map_err(|e| e.to_string())?;
                    }
                } // Drop cursor

                let err_output = String::from_utf8(buffer).unwrap();
                errors.push(err_output);
            }
        }
    }

    if errors.is_empty() {
        return Ok(());
    }

    let mut error_details = String::from("Step artist mismatches:\n");
    for line in errors {
        error_details.push_str(&line);
        error_details.push('\n');
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
        path.display(),
        error_details
    ))
}

fn compare_chart_timing_fields(
    path: &Path,
    expected_charts: &[ChartSummary],
    actual_charts: &[ChartSummary],
) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();

    let count = expected_charts.len().max(actual_charts.len());
    for idx in 0..count {
        let expected_chart_opt = expected_charts.get(idx);
        let actual_chart_opt = actual_charts.get(idx);

        match (expected_chart_opt, actual_chart_opt) {
            (None, None) => panic!(), // Unreachable
            (None, Some(actual_chart)) => {
                errors.push(format!(
                    "Unexpected {} {} chart at index {}",
                    actual_chart.step_type_str, actual_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), None) => {
                errors.push(format!(
                    "Expected {} {} chart at index {}",
                    expected_chart.step_type_str, expected_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), Some(actual_chart)) => {
                // Massage this boolean into a string to keep the comparison table simple
                let expected_chart_has_own_timing =
                    Some(expected_chart.chart_has_own_timing.to_string());
                let actual_chart_has_own_timing =
                    Some(actual_chart.chart_has_own_timing.to_string());

                let comparison_table: Vec<(
                    &str,
                    &Option<String>,
                    &Option<String>,
                    bool,
                    Option<&[u8]>,
                )> = vec![
                    (
                        "chart_has_own_timing",
                        &expected_chart_has_own_timing,
                        &actual_chart_has_own_timing,
                        expected_chart.chart_has_own_timing == actual_chart.chart_has_own_timing,
                        None,
                    ),
                    build_comparison_entry_opt!(chart_attacks, expected_chart, actual_chart),
                    build_comparison_entry_opt!(chart_stops, expected_chart, actual_chart),
                    build_comparison_entry_opt!(
                        chart_speeds,
                        expected_chart,
                        actual_chart,
                        DEFAULT_SPEEDS
                    ),
                    build_comparison_entry_opt!(
                        chart_scrolls,
                        expected_chart,
                        actual_chart,
                        DEFAULT_SCROLLS
                    ),
                    build_comparison_entry_opt!(
                        chart_bpms,
                        expected_chart,
                        actual_chart,
                        DEFAULT_BPMS
                    ),
                    build_comparison_entry_opt!(chart_delays, expected_chart, actual_chart),
                    build_comparison_entry_opt!(chart_warps, expected_chart, actual_chart),
                    build_comparison_entry_opt!(chart_fakes, expected_chart, actual_chart),
                    build_comparison_entry_opt!(chart_display_bpm, expected_chart, actual_chart),
                    build_comparison_entry_opt!(
                        chart_time_signatures,
                        expected_chart,
                        actual_chart,
                        DEFAULT_TIME_SIGNATURES
                    ),
                    build_comparison_entry_opt!(
                        chart_labels,
                        expected_chart,
                        actual_chart,
                        DEFAULT_LABELS
                    ),
                    build_comparison_entry_opt!(
                        chart_tickcounts,
                        expected_chart,
                        actual_chart,
                        DEFAULT_TICKCOUNTS
                    ),
                    build_comparison_entry_opt!(
                        chart_combos,
                        expected_chart,
                        actual_chart,
                        DEFAULT_COMBOS
                    ),
                ];

                let all_ok = comparison_table.iter().all(|entry| entry.3);

                if all_ok {
                    continue;
                }

                // Build error string
                let mut buffer = vec![];
                {
                    let mut cursor = io::Cursor::new(&mut buffer);
                    for (field_name, expected, actual, cmp, _) in comparison_table {
                        writeln!(
                            &mut cursor,
                            "  {}: baseline: {:?} -> reencoded: {:?} {}",
                            field_name,
                            expected,
                            actual,
                            match cmp {
                                true => "....ok",
                                false => "....MISMATCH",
                            }
                        )
                        .map_err(|e| e.to_string())?;
                    }
                } // Drop cursor

                let err_output = String::from_utf8(buffer).unwrap();
                errors.push(err_output);
            }
        }
    }

    if errors.is_empty() {
        return Ok(());
    }

    let mut error_details = String::from("Chart mismatches:\n");
    for line in errors {
        error_details.push_str(&line);
        error_details.push('\n');
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
        path.display(),
        error_details
    ))
}

fn compare_timing(
    path: &Path,
    expected_simfile: &SimfileSummary,
    actual_simfile: &SimfileSummary,
) -> Result<(), String> {
    let mut errors: Vec<String> = Vec::new();

    let count = expected_simfile
        .charts
        .len()
        .max(actual_simfile.charts.len());
    for idx in 0..count {
        let expected_chart_opt = expected_simfile.charts.get(idx);
        let actual_chart_opt = actual_simfile.charts.get(idx);

        match (expected_chart_opt, actual_chart_opt) {
            (None, None) => panic!(), // Unreachable
            (None, Some(actual_chart)) => {
                errors.push(format!(
                    "Unexpected {} {} chart at index {}",
                    actual_chart.step_type_str, actual_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), None) => {
                errors.push(format!(
                    "Expected {} {} chart at index {}",
                    expected_chart.step_type_str, expected_chart.difficulty_str, idx
                ));
            }
            (Some(expected_chart), Some(actual_chart)) => {
                let expected_timing = build_timing_snapshot(expected_chart, expected_simfile);
                let actual_timing = build_timing_snapshot(actual_chart, actual_simfile);
                let timing_comparison_table =
                    build_timing_comparison_table(&expected_timing, &actual_timing);

                let all_ok = timing_comparison_table.iter().all(|entry| entry.1);

                if all_ok {
                    continue;
                }

                // Build error string
                let mut error = String::new();
                for (field_name, matches) in timing_comparison_table {
                    error += &format!(
                        "  {}: {}",
                        field_name,
                        match matches {
                            true => "....ok",
                            false => "....MISMATCH",
                        }
                    );
                }

                error += &format!(
                    "\nExpected: {:?}\nActual: {:?}\n",
                    expected_timing, actual_timing
                );

                errors.push(error);
            }
        }
    }

    if errors.is_empty() {
        return Ok(());
    }

    let mut error_details = String::from("Timing mismatches:\n");
    for line in errors {
        error_details.push_str(&line);
        error_details.push('\n');
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
        path.display(),
        error_details
    ))
}

fn run_rssp_analyze(raw_bytes: &[u8], extension: &str) -> Result<SimfileSummary, String> {
    let options = AnalysisOptions {
        strip_tags: false,
        mono_threshold: 6,
        custom_patterns: Vec::new(),
        compute_tech_counts: false,
        compute_note_annotations: false,
        compute_pattern_counts: false,
        translate_markers: false,
    };
    analyze(raw_bytes, extension, &options)
}

fn check_file(path: &Path, extension: &str) -> Result<(), String> {
    let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {e}"))?;

    let base_summary = run_rssp_analyze(&raw_bytes, extension)?;

    // Serialize to simfile bytes in memory
    let mut buffer = vec![];
    {
        let mut cursor = io::Cursor::new(&mut buffer);
        serialize_simfile(&base_summary, extension, &mut cursor).map_err(|e| e.to_string())?;
    };

    // Re-run rssp analyze on the serialized simfile
    let reencoded_summary = run_rssp_analyze(&buffer, extension)?;

    println!("File: {}", path.display());

    // let result = String::from_utf8(buffer);
    // match result {
    //     Ok(buffer_str) => println!("{}", buffer_str),
    //     Err(_) => println!("debug failed"),
    // }
    // println!("{:?}", base_summary);

    compare_simfile_str_fields(path, extension, &base_summary, &reencoded_summary)?;
    compare_simfile_normalized_fields(path, &base_summary, &reencoded_summary)?;
    compare_chart_str_fields_and_hashes(path, &base_summary.charts, &reencoded_summary.charts)?;
    compare_chart_timing_fields(path, &base_summary.charts, &reencoded_summary.charts)?;
    compare_timing(path, &base_summary, &reencoded_summary)?;

    Ok(())
}

fn main() {
    let interrupt = Arc::new(AtomicBool::new(false));
    let handler_interrupt = interrupt.clone();
    ctrlc::set_handler(move || {
        handler_interrupt.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let args = Arguments::from_args();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = manifest_dir.join("tests/data/packs");

    if !packs_dir.exists() {
        println!("No tests/packs directory found.");
        return;
    }

    let mut tests = Vec::new();

    for entry in WalkDir::new(&packs_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
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
            .map(str::to_lowercase)
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
        if interrupt.load(Ordering::SeqCst) {
            break;
        }

        let TestCase {
            name,
            path,
            extension,
        } = test;

        let res = check_file(&path, &extension);
        match res {
            Ok(()) => {
                println!("test {name} ... ok");
                num_passed += 1;
            }
            Err(msg) => {
                println!("test {name} ... FAILED");
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
                "rerun: cargo test --test serialize_parity -- --exact {:?}",
                failure.name
            );
        }
        println!();
    }

    if num_failed == 0 {
        println!("test result: ok. {num_passed} passed; 0 failed");
        return;
    }

    println!("test result: FAILED. {num_passed} passed; {num_failed} failed");
    std::process::exit(101);
}

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use libtest_mimic::Arguments;
use walkdir::WalkDir;

use rssp::parse::extension_is_ssc;
use rssp::serialize::*;
use rssp::stats::RADAR_CATEGORY_COUNT;
use rssp::timing::{SpeedUnit, TimingSegments};
use rssp::{AnalysisOptions, ChartSummary, SimfileSummary, analyze};

pub const NORM_DEFAULT_BPMS: &[u8] = b"0.000=60.000";
pub const NORM_DEFAULT_SPEEDS: &[u8] = b"0.000=1.000=0.000=0";
pub const NORM_DEFAULT_SCROLLS: &[u8] = b"0.000=1.000";
pub const TIMING_DEFAULT_BPMS: &[(f32, f32)] = &[(0.0f32, 60.0f32)];
pub const TIMING_DEFAULT_SPEEDS: &[(f32, f32, f32, SpeedUnit)] =
    &[(0.0f32, 1.0f32, 0.0f32, SpeedUnit::Beats)];
pub const TIMING_DEFAULT_SCROLLS: &[(f32, f32)] = &[(0.0f32, 1.0f32)];

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

const TIMING_EPS: f32 = 1e-3;

fn timing_approx_eq(a: &f32, b: &f32) -> bool {
    (a - b).abs() <= TIMING_EPS
}

fn radar_values_eq(
    expected_opt: &Option<[f32; RADAR_CATEGORY_COUNT]>,
    actual_opt: &Option<[f32; RADAR_CATEGORY_COUNT]>,
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

macro_rules! compare_fields {
    ($errors: expr, $expected: expr, $actual: expr, $eq_fn: ident, $field: ident) => {
        compare_fields!($errors, $expected, $actual, $eq_fn, $field, (|_, _| false))
    };
    ($errors: expr, $expected: expr, $actual: expr, $eq_fn: ident, $field: ident, $is_default_fn: expr) => {
        (if !$eq_fn(&$expected.$field, &$actual.$field)
            && !$is_default_fn(&$expected.$field, &$actual.$field)
        {
            $errors += &format!(
                "{}: baseline: {:?} -> reencoded: {:?} ....MISMATCH\n",
                stringify!($field),
                $expected.$field,
                $actual.$field,
            );
        })
    };
}

fn eq<T: Eq>(expected: &T, actual: &T) -> bool {
    expected == actual
}

fn timing_pairs_eq(expected: &Vec<(f32, f32)>, actual: &Vec<(f32, f32)>) -> bool {
    expected
        .iter()
        .zip(actual.iter())
        .all(|(a, b)| timing_approx_eq(&a.0, &b.0) && timing_approx_eq(&a.1, &b.1))
}

fn timing_speeds_eq(
    expected: &Vec<(f32, f32, f32, SpeedUnit)>,
    actual: &Vec<(f32, f32, f32, SpeedUnit)>,
) -> bool {
    expected.iter().zip(actual.iter()).all(|(a, b)| {
        timing_approx_eq(&a.0, &b.0)
            && timing_approx_eq(&a.1, &b.1)
            && timing_approx_eq(&a.2, &b.2)
            && a.3 == b.3
    })
}

fn version_eq(expected: &f32, actual: &f32) -> bool {
    let expected_str = if expected.is_finite() {
        format!("{:.2}", expected)
    } else {
        String::new()
    };
    let actual_str = if actual.is_finite() {
        format!("{:.2}", actual)
    } else {
        String::new()
    };
    expected_str == actual_str
}

fn normalized_eq(expected: &str, actual: &str) -> bool {
    let mut a_lines = expected.lines();
    let mut b_lines = actual.lines();
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

fn normalized_opt_eq(expected: &Option<String>, actual: &Option<String>) -> bool {
    match (expected, actual) {
        (Some(a), Some(b)) => normalized_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn is_default_str(default: &[u8]) -> impl Fn(&str, &str) -> bool {
    move |a, b| a.is_empty() && b.as_bytes() == default
}

fn is_default_version(default: f32) -> impl Fn(&f32, &f32) -> bool {
    move |a, b| !a.is_finite() && *b == default
}

fn is_default_bytes_opt(default: &[u8]) -> impl Fn(&Option<String>, &Option<String>) -> bool {
    move |e, a| {
        e.as_ref().is_none_or(|e| e.is_empty())
            && a.as_ref().is_some_and(|a| a.as_bytes() == default)
    }
}

fn is_default_pairs(default: &[(f32, f32)]) -> impl Fn(&Vec<(f32, f32)>, &Vec<(f32, f32)>) -> bool {
    move |e, a| e.is_empty() && a == &default
}

fn is_default_speeds(
    default: &[(f32, f32, f32, SpeedUnit)],
) -> impl Fn(&Vec<(f32, f32, f32, SpeedUnit)>, &Vec<(f32, f32, f32, SpeedUnit)>) -> bool {
    move |e, a| e.is_empty() && a == &default
}

// StepMania and DeadSync both strip certain BPMs during parsing,
// which will inevitably change the normalized BPMs and chart hashes after serializing.
// Use this function to determine when to ignore changes to those fields.
fn has_hash_breaking_bpms(normalized_bpms: &str) -> bool {
    let mut previous_beat: &str = "";
    let mut previous_value: &str = "";
    for pair in normalized_bpms.split(",") {
        if let Some((beat, value)) = pair.split_once('=') {
            if value == "0.000" {
                return true;
            } else if beat == previous_beat || value == previous_value {
                return true;
            }
            previous_beat = beat;
            previous_value = value;
        }
    }
    false
}

#[inline(always)]
fn has_warp_hacks(is_ssc: bool, timing_segments: &TimingSegments) -> bool {
    !is_ssc && !timing_segments.warps.is_empty()
}

struct Exemptions {
    is_ssc: bool,
    has_hash_breaking_bpms: bool,
    has_warp_hacks: bool,
}

impl Exemptions {
    fn for_simfile(expected: &SimfileSummary, is_ssc: bool) -> Exemptions {
        Exemptions {
            is_ssc,
            has_hash_breaking_bpms: has_hash_breaking_bpms(&expected.normalized_bpms),
            has_warp_hacks: has_warp_hacks(is_ssc, &expected.global_timing_segments),
        }
    }
}

#[rustfmt::skip]
fn compare_simfile_str_fields(
    expected: &SimfileSummary,
    actual: &SimfileSummary,
    exemptions: &Exemptions,
) -> Result<(), String> {

    let mut errors = String::new();

    compare_fields!(errors, expected, actual, eq, title_str, is_default_str(DEFAULT_TITLE));
    compare_fields!(errors, expected, actual, eq, subtitle_str);
    compare_fields!(errors, expected, actual, eq, artist_str, is_default_str(DEFAULT_ARTIST));
    compare_fields!(errors, expected, actual, eq, titletranslit_str);
    compare_fields!(errors, expected, actual, eq, subtitletranslit_str);
    compare_fields!(errors, expected, actual, eq, artisttranslit_str);
    compare_fields!(errors, expected, actual, eq, display_bpm_str);
    compare_fields!(errors, expected, actual, eq, credit_str);
    compare_fields!(errors, expected, actual, eq, genre_str);
    compare_fields!(errors, expected, actual, eq, lyrics_path);
    compare_fields!(errors, expected, actual, eq, music_path);
    compare_fields!(errors, expected, actual, eq, cdtitle_path);
    compare_fields!(errors, expected, actual, eq, background_path);
    compare_fields!(errors, expected, actual, eq, banner_path);

    if exemptions.is_ssc {
        compare_fields!(errors, expected, actual, version_eq, ssc_version, is_default_version(0.83));
        compare_fields!(errors, expected, actual, eq, origin_str);
        compare_fields!(errors, expected, actual, eq, discimage_path);
        compare_fields!(errors, expected, actual, eq, cdimage_path);
        compare_fields!(errors, expected, actual, eq, previewvid_path);
        compare_fields!(errors, expected, actual, eq, jacket_path);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(String::from("Simfile string field mismatches:\n") + &errors)
    }
}

#[rustfmt::skip]
fn compare_simfile_normalized_fields(
    expected: &SimfileSummary,
    actual: &SimfileSummary,
    exemptions: &Exemptions,
) -> Result<(), String> {
    let mut errors = String::new();

    if !exemptions.has_hash_breaking_bpms && !exemptions.has_warp_hacks {
        compare_fields!(errors, expected, actual, normalized_eq, normalized_bpms, is_default_str(NORM_DEFAULT_BPMS));
    }
    if !exemptions.has_warp_hacks {
        compare_fields!(errors, expected, actual, normalized_eq, normalized_stops);
        compare_fields!(errors, expected, actual, normalized_eq, normalized_delays);
        compare_fields!(errors, expected, actual, normalized_eq, normalized_warps);
    }
    compare_fields!(errors, expected, actual, normalized_eq, normalized_speeds, is_default_str(NORM_DEFAULT_SPEEDS));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_scrolls, is_default_str(NORM_DEFAULT_SCROLLS));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_fakes);
    compare_fields!(errors, expected, actual, normalized_eq, normalized_time_signatures, is_default_str(DEFAULT_TIME_SIGNATURES));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_labels, is_default_str(DEFAULT_LABELS));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_tickcounts, is_default_str(DEFAULT_TICKCOUNTS));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_combos, is_default_str(DEFAULT_COMBOS));
    compare_fields!(errors, expected, actual, normalized_eq, normalized_bgchanges);
    compare_fields!(errors, expected, actual, normalized_eq, normalized_fgchanges);
    compare_fields!(errors, expected, actual, normalized_eq, normalized_keysounds);
    compare_fields!(errors, expected, actual, normalized_eq, normalized_attacks);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(String::from("Simfile normalized field mismatches:\n") + &errors)
    }
}

#[rustfmt::skip]
fn compare_simfile_timing_fields(
    expected: &SimfileSummary,
    actual: &SimfileSummary,
    exemptions: &Exemptions,
) -> Result<(), String> {
    let mut errors = String::new();
    let expected_timing = &expected.global_timing_segments;
    let actual_timing = &actual.global_timing_segments;

    if !exemptions.has_warp_hacks {
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, bpms, is_default_pairs(TIMING_DEFAULT_BPMS));
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, stops);
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, delays);
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, warps);
    }
    compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, scrolls, is_default_pairs(TIMING_DEFAULT_SCROLLS));
    compare_fields!(errors, expected_timing, actual_timing, timing_speeds_eq, speeds, is_default_speeds(TIMING_DEFAULT_SPEEDS));
    compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, fakes);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(String::from("Simfile timing mismatches:\n") + &errors)
    }
}

#[rustfmt::skip]
fn compare_chart_str_fields(
    expected: &ChartSummary,
    actual: &ChartSummary,
    _exemptions: &Exemptions,
) -> Result<(), String> {
    let mut errors = String::new();

    compare_fields!(errors, expected, actual, eq, step_type_str, is_default_str(DEFAULT_STEPSTYPE));
    compare_fields!(errors, expected, actual, eq, step_artist_str);
    compare_fields!(errors, expected, actual, eq, description_str);
    compare_fields!(errors, expected, actual, eq, chart_name_str);
    compare_fields!(errors, expected, actual, eq, chart_style_str);
    compare_fields!(errors, expected, actual, eq, difficulty_str, is_default_str(DEFAULT_DIFFICULTY));
    compare_fields!(errors, expected, actual, eq, rating_str, is_default_str(DEFAULT_METER));
    compare_fields!(errors, expected, actual, radar_values_eq, cached_radar_values);
    compare_fields!(errors, expected, actual, eq, chart_display_bpm);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(String::from("Chart string field mismatches:\n") + &errors)
    }
}

#[rustfmt::skip]
fn compare_chart_hashes_and_timing(
    expected: &ChartSummary,
    actual: &ChartSummary,
    exemptions: &Exemptions,
) -> Result<(), String> {
    let mut errors = String::new();
    let expected_timing = &expected.timing_segments;
    let actual_timing = &actual.timing_segments;

    if !exemptions.has_hash_breaking_bpms && !exemptions.has_warp_hacks {
        compare_fields!(errors, expected, actual, eq, short_hash);
    }
    compare_fields!(errors, expected, actual, eq, bpm_neutral_hash);
    compare_fields!(errors, expected, actual, eq, chart_has_own_timing);
    compare_fields!(errors, expected, actual, normalized_opt_eq, chart_time_signatures, is_default_bytes_opt(DEFAULT_TIME_SIGNATURES));
    compare_fields!(errors, expected, actual, normalized_opt_eq, chart_labels, is_default_bytes_opt(DEFAULT_LABELS));
    compare_fields!(errors, expected, actual, normalized_opt_eq, chart_tickcounts, is_default_bytes_opt(DEFAULT_TICKCOUNTS));
    compare_fields!(errors, expected, actual, normalized_opt_eq, chart_combos, is_default_bytes_opt(DEFAULT_COMBOS));
    if !exemptions.has_warp_hacks {
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, bpms, is_default_pairs(&[(0.0f32, 60.0f32)]));
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, stops);
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, delays);
        compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, warps);
    }
    compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, scrolls, is_default_pairs(&[(0.0f32, 1.0f32)]));
    compare_fields!(errors, expected_timing, actual_timing, timing_speeds_eq, speeds, is_default_speeds(&[(0.0f32, 1.0f32, 0.0f32, SpeedUnit::Beats)]));
    compare_fields!(errors, expected_timing, actual_timing, timing_pairs_eq, fakes);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(String::from("Chart hash / timing mismatches:\n") + &errors)
    }
}

fn compare_charts(
    expected_simfile: &SimfileSummary,
    actual_simfile: &SimfileSummary,
    exemptions: &Exemptions,
) -> Result<(), String> {
    let mut errors = String::new();

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
                errors += &format!(
                    "Unexpected {} {} chart at index {}\n",
                    actual_chart.step_type_str, actual_chart.difficulty_str, idx
                );
            }
            (Some(expected_chart), None) => {
                errors += &format!(
                    "Expected {} {} chart at index {}\n",
                    expected_chart.step_type_str, expected_chart.difficulty_str, idx
                );
            }
            (Some(expected_chart), Some(actual_chart)) => {
                match compare_chart_str_fields(expected_chart, actual_chart, exemptions) {
                    Err(errs) => errors += &errs,
                    _ => {}
                }
                match compare_chart_hashes_and_timing(expected_chart, actual_chart, exemptions) {
                    Err(errs) => errors += &errs,
                    _ => {}
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
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
    let is_ssc = extension_is_ssc(extension).map_err(|e| e.to_string())?;
    let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {e}"))?;

    let base_summary = run_rssp_analyze(&raw_bytes, extension)?;
    let exemptions = Exemptions::for_simfile(&base_summary, is_ssc);

    // Serialize to simfile bytes in memory
    let mut buffer = vec![];
    {
        let mut cursor = io::Cursor::new(&mut buffer);
        serialize_simfile(&base_summary, extension, &mut cursor).map_err(|e| e.to_string())?;
    };

    // Re-run rssp analyze on the serialized simfile
    let reencoded_summary = run_rssp_analyze(&buffer, extension)?;

    println!("File: {}", path.display());

    let mut comparison_results = Vec::with_capacity(6); // should match # of tests below

    comparison_results.push(compare_simfile_str_fields(
        &base_summary,
        &reencoded_summary,
        &exemptions,
    ));
    comparison_results.push(compare_simfile_normalized_fields(
        &base_summary,
        &reencoded_summary,
        &exemptions,
    ));
    comparison_results.push(compare_simfile_timing_fields(
        &base_summary,
        &reencoded_summary,
        &exemptions,
    ));

    // A few SSC files in the corpus are missing a #VERSION tag
    // but have per-chart timing data, which is invalid.
    // Don't bother validating these.
    if is_ssc && !base_summary.ssc_version.is_finite() {
        println!("SSC file is missing a version - skipping chart tests");
    } else {
        comparison_results.push(compare_charts(
            &base_summary,
            &reencoded_summary,
            &exemptions,
        ));
    }

    let all_ok = comparison_results.iter().all(|r| r.is_ok());
    if all_ok {
        Ok(())
    } else {
        let mut err = "\n\nMISMATCH DETECTED\n".to_string();
        for result in comparison_results {
            match result {
                Err(e) => err.push_str(&e),
                Ok(_) => {}
            }
        }
        Err(err)
    }
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
                    name: path.to_string_lossy().to_string(),
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

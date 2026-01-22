use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::report::{TimingSnapshot, build_timing_snapshot};
use rssp::{AnalysisOptions, analyze};

#[derive(Debug, Deserialize)]
struct GoldenTiming {
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
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    timing: Option<GoldenTiming>,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartTimingInfo {
    step_type: String,
    difficulty: String,
    timing: TimingSnapshot,
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

const EPS: f64 = 1e-3;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= EPS
}

fn timing_matches(expected: &GoldenTiming, actual: &TimingSnapshot) -> bool {
    if !approx_eq(expected.beat0_offset_seconds, actual.beat0_offset_seconds) {
        return false;
    }
    if !approx_eq(
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
            .all(|(e, a)| approx_eq(e.0, a.0) && approx_eq(e.1, a.1))
}

fn compare_time_signatures(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| approx_eq(e.0, a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_labels(expected: &[(f64, String)], actual: &[(f64, String)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| approx_eq(e.0, a.0) && e.1 == a.1)
}

fn compare_tickcounts(expected: &[(f64, i32)], actual: &[(f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| approx_eq(e.0, a.0) && e.1 == a.1)
}

fn compare_combos(expected: &[(f64, i32, i32)], actual: &[(f64, i32, i32)]) -> bool {
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .all(|(e, a)| approx_eq(e.0, a.0) && e.1 == a.1 && e.2 == a.2)
}

fn compare_speeds(expected: &[(f64, f64, f64, i32)], actual: &[(f64, f64, f64, i32)]) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(e, a)| {
            approx_eq(e.0, a.0) && approx_eq(e.1, a.1) && approx_eq(e.2, a.2) && e.3 == a.3
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
        "bpms:{bpms} stops:{stops} delays:{delays} warps:{warps} speeds:{speeds} scrolls:{scrolls} time_sigs:{time_signatures} labels:{labels} tickcounts:{tickcounts} combos:{combos} fakes:{fakes}"
    )
}

fn timing_counts_snapshot(timing: &TimingSnapshot) -> String {
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

fn timing_counts_expected(timing: &GoldenTiming) -> String {
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

fn compute_chart_timings(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartTimingInfo>, String> {
    let summary =
        analyze(simfile_data, extension, AnalysisOptions::default()).map_err(|e| e)?;

    let mut results = Vec::new();
    for chart in &summary.charts {
        let timing = build_timing_snapshot(chart, &summary);
        results.push(ChartTimingInfo {
            step_type: chart.step_type_str.clone(),
            difficulty: chart.difficulty_str.clone(),
            timing,
        });
    }

    Ok(results)
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {e}"))?;

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];

    let golden_path = baseline_dir
        .join(subfolder)
        .join(format!("{file_hash}.json.zst"));

    if !golden_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            golden_path.display()
        ));
    }

    let compressed_golden =
        fs::read(&golden_path).map_err(|e| format!("Failed to read baseline file: {e}"))?;

    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {e}"))?;

    let golden_charts: Vec<GoldenChart> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {e}"))?;

    let rssp_charts = compute_chart_timings(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {e}"))?;

    let mut golden_map: HashMap<(String, String), Vec<GoldenChart>> = HashMap::new();
    for golden in golden_charts {
        let step_type_lower = golden.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let difficulty = rssp::normalize_difficulty_label(&golden.difficulty);
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        golden_map.entry(key).or_default().push(golden);
    }

    let mut rssp_map: HashMap<(String, String), Vec<ChartTimingInfo>> = HashMap::new();
    for chart in rssp_charts {
        let step_type_lower = chart.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let key = (step_type_lower, chart.difficulty.to_ascii_lowercase());
        rssp_map.entry(key).or_default().push(chart);
    }

    let mut golden_entries: Vec<_> = golden_map.into_iter().collect();
    golden_entries.sort_by(|a, b| a.0.cmp(&b.0));

    println!("File: {}", path.display());

    for ((step_type, difficulty), expected_entries) in golden_entries {
        let Some(actual_entries) = rssp_map.remove(&(step_type.clone(), difficulty.clone())) else {
            println!(
                "  {step_type} {difficulty}: baseline present, RSSP missing chart"
            );
            return Err(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                step_type,
                difficulty
            ));
        };

        let count = expected_entries.len().max(actual_entries.len());
        for idx in 0..count {
            let expected = expected_entries.get(idx);
            let actual = actual_entries.get(idx);
            let meter_label = expected
                .and_then(|entry| entry.meter).map_or_else(|| (idx + 1).to_string(), |meter| meter.to_string());

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
                actual_timing.map_or_else(|| "-".to_string(), timing_counts_snapshot),
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries
                .iter()
                .zip(&actual_entries)
                .all(|(expected, actual)| {
                    let Some(expected_timing) = expected.timing.as_ref() else {
                        return false;
                    };
                    timing_matches(expected_timing, &actual.timing)
                });
        if !matches {
            let expected_values: Vec<&GoldenTiming> = expected_entries
                .iter()
                .filter_map(|e| e.timing.as_ref())
                .collect();
            let actual_values: Vec<&TimingSnapshot> =
                actual_entries.iter().map(|a| &a.timing).collect();
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

fn main() {
    let args = Arguments::from_args();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = manifest_dir.join("tests/data/packs");
    let baseline_dir = manifest_dir.join("tests/data/baseline");

    if !packs_dir.exists() {
        println!("No tests/packs directory found.");
        return;
    }

    let mut tests = Vec::new();

    for entry in WalkDir::new(&packs_dir).into_iter().filter_map(std::result::Result::ok) {
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
        let TestCase {
            name,
            path,
            extension,
        } = test;

        let res = check_file(&path, &extension, &baseline_dir);
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
                "rerun: cargo test --test timing_parity -- --exact {:?}",
                failure.name
            );
        }
        println!();
    }

    if num_failed == 0 {
        println!("test result: ok. {num_passed} passed; 0 failed");
        return;
    }

    println!(
        "test result: FAILED. {num_passed} passed; {num_failed} failed"
    );
    std::process::exit(101);
}

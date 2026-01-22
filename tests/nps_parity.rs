use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::stats::measure_equally_spaced;
use rssp::{AnalysisOptions, analyze, step_type_lanes};

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    peak_nps: f64,
    notes_per_measure: Vec<u32>,
    nps_per_measure: Vec<f64>,
    equally_spaced_per_measure: Vec<bool>,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartMeasureInfo {
    step_type: String,
    difficulty: String,
    peak_nps: f64,
    notes_per_measure: Vec<u32>,
    nps_per_measure: Vec<f64>,
    equally_spaced_per_measure: Vec<bool>,
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

const NPS_EPS: f64 = 1e-4;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= NPS_EPS
}

fn format_len<T>(opt: Option<&[T]>) -> String {
    opt.map_or_else(|| "-".to_string(), |v| v.len().to_string())
}

fn compute_chart_nps(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartMeasureInfo>, String> {
    let options = AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..AnalysisOptions::default()
    };
    let summary = analyze(simfile_data, extension, options).map_err(|e| e)?;

    Ok(summary
        .charts
        .into_iter()
        .map(|chart| {
            let lanes = step_type_lanes(&chart.step_type_str);
            ChartMeasureInfo {
                step_type: chart.step_type_str,
                difficulty: chart.difficulty_str,
                peak_nps: chart.max_nps,
                notes_per_measure: chart.measure_densities.iter().map(|&v| v as u32).collect(),
                nps_per_measure: chart.measure_nps_vec,
                equally_spaced_per_measure: measure_equally_spaced(
                    &chart.minimized_note_data,
                    lanes,
                ),
            }
        })
        .collect())
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

    let rssp_charts = compute_chart_nps(&raw_bytes, extension)
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

    let mut rssp_map: HashMap<(String, String), Vec<ChartMeasureInfo>> = HashMap::new();
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

            let expected_val = expected.map(|e| e.peak_nps);
            let actual_val = actual.map(|a| a.peak_nps);
            let peak_matches = match (expected_val, actual_val) {
                (Some(exp), Some(act)) => approx_eq(exp, act),
                _ => false,
            };
            let notes_matches = match (expected, actual) {
                (Some(exp), Some(act)) => exp.notes_per_measure == act.notes_per_measure,
                _ => false,
            };
            let nps_matches = match (expected, actual) {
                (Some(exp), Some(act)) => {
                    exp.nps_per_measure.len() == act.nps_per_measure.len()
                        && exp
                            .nps_per_measure
                            .iter()
                            .zip(&act.nps_per_measure)
                            .all(|(e, a)| approx_eq(*e, *a))
                }
                _ => false,
            };
            let spacing_matches = match (expected, actual) {
                (Some(exp), Some(act)) => {
                    exp.equally_spaced_per_measure == act.equally_spaced_per_measure
                }
                _ => false,
            };
            let status = if peak_matches && notes_matches && nps_matches && spacing_matches {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: peak_nps: {} -> {} | notes_per_measure len {} -> {} | nps_per_measure len {} -> {} | equally_spaced len {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_val.map_or_else(|| "-".to_string(), |v| format!("{v:.5}")),
                actual_val.map_or_else(|| "-".to_string(), |v| format!("{v:.5}")),
                format_len(expected.map(|e| e.notes_per_measure.as_slice())),
                format_len(actual.map(|a| a.notes_per_measure.as_slice())),
                format_len(expected.map(|e| e.nps_per_measure.as_slice())),
                format_len(actual.map(|a| a.nps_per_measure.as_slice())),
                format_len(expected.map(|e| e.equally_spaced_per_measure.as_slice())),
                format_len(actual.map(|a| a.equally_spaced_per_measure.as_slice())),
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries.iter().zip(&actual_entries).all(|(e, a)| {
                approx_eq(e.peak_nps, a.peak_nps)
                    && e.notes_per_measure == a.notes_per_measure
                    && e.nps_per_measure.len() == a.nps_per_measure.len()
                    && e.nps_per_measure
                        .iter()
                        .zip(&a.nps_per_measure)
                        .all(|(exp, act)| approx_eq(*exp, *act))
                    && e.equally_spaced_per_measure == a.equally_spaced_per_measure
            });
        if !matches {
            let expected_values: Vec<f64> = expected_entries.iter().map(|e| e.peak_nps).collect();
            let actual_values: Vec<f64> = actual_entries.iter().map(|a| a.peak_nps).collect();
            let expected_notes: Vec<Vec<u32>> = expected_entries
                .iter()
                .map(|e| e.notes_per_measure.clone())
                .collect();
            let actual_notes: Vec<Vec<u32>> = actual_entries
                .iter()
                .map(|a| a.notes_per_measure.clone())
                .collect();
            let expected_nps: Vec<Vec<f64>> = expected_entries
                .iter()
                .map(|e| e.nps_per_measure.clone())
                .collect();
            let actual_nps: Vec<Vec<f64>> = actual_entries
                .iter()
                .map(|a| a.nps_per_measure.clone())
                .collect();
            let expected_spaced: Vec<Vec<bool>> = expected_entries
                .iter()
                .map(|e| e.equally_spaced_per_measure.clone())
                .collect();
            let actual_spaced: Vec<Vec<bool>> = actual_entries
                .iter()
                .map(|a| a.equally_spaced_per_measure.clone())
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP peak_nps:   {:?}\nGolden peak_nps: {:?}\nRSSP notes_per_measure:   {:?}\nGolden notes_per_measure: {:?}\nRSSP nps_per_measure:     {:?}\nGolden nps_per_measure:   {:?}\nRSSP equally_spaced_per_measure:   {:?}\nGolden equally_spaced_per_measure: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_values,
                expected_values,
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
                "rerun: cargo test --test nps_parity -- --exact {:?}",
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

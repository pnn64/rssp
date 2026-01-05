use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::bpm::chart_bpm_snapshots;

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    bpms: String,
    hash_bpms: String,
    bpm_min: f64,
    bpm_max: f64,
    display_bpm: String,
    display_bpm_min: f64,
    display_bpm_max: f64,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartBpmInfo {
    step_type: String,
    difficulty: String,
    hash_bpms: String,
    bpms: String,
    bpm_min: f64,
    bpm_max: f64,
    display_bpm: String,
    display_bpm_min: f64,
    display_bpm_max: f64,
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

const BPM_EPS: f64 = 1e-3;

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= BPM_EPS
}

fn compute_chart_bpms(simfile_data: &[u8], extension: &str) -> Result<Vec<ChartBpmInfo>, String> {
    let snapshots = chart_bpm_snapshots(simfile_data, extension)
        .map_err(|e| e.to_string())?;

    Ok(snapshots
        .into_iter()
        .map(|chart| {
            ChartBpmInfo {
                step_type: chart.step_type,
                difficulty: chart.difficulty,
                hash_bpms: chart.hash_bpms,
                bpms: chart.bpms_formatted,
                bpm_min: chart.bpm_min,
                bpm_max: chart.bpm_max,
                display_bpm: chart.display_bpm,
                display_bpm_min: chart.display_bpm_min,
                display_bpm_max: chart.display_bpm_max,
            }
        })
        .collect())
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let compressed_bytes = fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {}", e))?;

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];

    let golden_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.json.zst", file_hash));

    if !golden_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            golden_path.display()
        ));
    }

    let compressed_golden = fs::read(&golden_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let golden_charts: Vec<GoldenChart> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_charts = compute_chart_bpms(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

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

    let mut rssp_map: HashMap<(String, String), Vec<ChartBpmInfo>> = HashMap::new();
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

        let count = expected_entries.len().max(actual_entries.len());
        for idx in 0..count {
            let expected = expected_entries.get(idx);
            let actual = actual_entries.get(idx);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());
            let expected_hash = expected.map(|entry| entry.hash_bpms.as_str());
            let actual_hash = actual.map(|entry| entry.hash_bpms.as_str());
            let expected_bpms = expected.map(|entry| entry.bpms.as_str());
            let actual_bpms = actual.map(|entry| entry.bpms.as_str());
            let expected_min = expected.map(|entry| entry.bpm_min);
            let actual_min = actual.map(|entry| entry.bpm_min);
            let expected_max = expected.map(|entry| entry.bpm_max);
            let actual_max = actual.map(|entry| entry.bpm_max);
            let expected_display = expected.map(|entry| entry.display_bpm.as_str());
            let actual_display = actual.map(|entry| entry.display_bpm.as_str());
            let expected_display_min = expected.map(|entry| entry.display_bpm_min);
            let actual_display_min = actual.map(|entry| entry.display_bpm_min);
            let expected_display_max = expected.map(|entry| entry.display_bpm_max);
            let actual_display_max = actual.map(|entry| entry.display_bpm_max);

            let hash_matches = expected_hash.is_some() && expected_hash == actual_hash;
            let bpms_matches = expected_bpms.is_some() && expected_bpms == actual_bpms;
            let min_matches = match (expected_min, actual_min) {
                (Some(exp), Some(act)) => approx_eq(exp, act),
                _ => false,
            };
            let max_matches = match (expected_max, actual_max) {
                (Some(exp), Some(act)) => approx_eq(exp, act),
                _ => false,
            };
            let display_matches = expected_display.is_some() && expected_display == actual_display;
            let display_min_matches = match (expected_display_min, actual_display_min) {
                (Some(exp), Some(act)) => approx_eq(exp, act),
                _ => false,
            };
            let display_max_matches = match (expected_display_max, actual_display_max) {
                (Some(exp), Some(act)) => approx_eq(exp, act),
                _ => false,
            };
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
                "  {} {} [{}]: hash_bpms: {} -> {} | bpms: {} -> {} | bpm_min: {} -> {} | bpm_max: {} -> {} | display_bpm: {} -> {} | display_bpm_min: {} -> {} | display_bpm_max: {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_hash.unwrap_or("-"),
                actual_hash.unwrap_or("-"),
                expected_bpms.unwrap_or("-"),
                actual_bpms.unwrap_or("-"),
                expected_min
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                actual_min
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                expected_max
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                actual_max
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                expected_display.unwrap_or("-"),
                actual_display.unwrap_or("-"),
                expected_display_min
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                actual_display_min
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                expected_display_max
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                actual_display_max
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "-".to_string()),
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries
                .iter()
                .zip(&actual_entries)
                .all(|(expected, actual)| {
                    expected.hash_bpms == actual.hash_bpms
                        && expected.bpms == actual.bpms
                        && approx_eq(expected.bpm_min, actual.bpm_min)
                        && approx_eq(expected.bpm_max, actual.bpm_max)
                        && expected.display_bpm == actual.display_bpm
                        && approx_eq(expected.display_bpm_min, actual.display_bpm_min)
                        && approx_eq(expected.display_bpm_max, actual.display_bpm_max)
                });
        if !matches {
            let expected_hashes: Vec<String> = expected_entries
                .iter()
                .map(|entry| entry.hash_bpms.clone())
                .collect();
            let actual_hashes: Vec<String> = actual_entries
                .iter()
                .map(|entry| entry.hash_bpms.clone())
                .collect();
            let expected_bpms: Vec<String> = expected_entries
                .iter()
                .map(|entry| entry.bpms.clone())
                .collect();
            let actual_bpms: Vec<String> = actual_entries
                .iter()
                .map(|entry| entry.bpms.clone())
                .collect();
            let expected_min: Vec<f64> = expected_entries.iter().map(|entry| entry.bpm_min).collect();
            let actual_min: Vec<f64> = actual_entries.iter().map(|entry| entry.bpm_min).collect();
            let expected_max: Vec<f64> = expected_entries.iter().map(|entry| entry.bpm_max).collect();
            let actual_max: Vec<f64> = actual_entries.iter().map(|entry| entry.bpm_max).collect();
            let expected_display: Vec<String> = expected_entries
                .iter()
                .map(|entry| entry.display_bpm.clone())
                .collect();
            let actual_display: Vec<String> = actual_entries
                .iter()
                .map(|entry| entry.display_bpm.clone())
                .collect();
            let expected_display_min: Vec<f64> = expected_entries
                .iter()
                .map(|entry| entry.display_bpm_min)
                .collect();
            let actual_display_min: Vec<f64> = actual_entries
                .iter()
                .map(|entry| entry.display_bpm_min)
                .collect();
            let expected_display_max: Vec<f64> = expected_entries
                .iter()
                .map(|entry| entry.display_bpm_max)
                .collect();
            let actual_display_max: Vec<f64> = actual_entries
                .iter()
                .map(|entry| entry.display_bpm_max)
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP hash_bpms:      {:?}\nGolden hash_bpms:    {:?}\nRSSP bpms:           {:?}\nGolden bpms:         {:?}\nRSSP bpm_min:        {:?}\nGolden bpm_min:      {:?}\nRSSP bpm_max:        {:?}\nGolden bpm_max:      {:?}\nRSSP display_bpm:    {:?}\nGolden display_bpm:  {:?}\nRSSP display_min:    {:?}\nGolden display_min:  {:?}\nRSSP display_max:    {:?}\nGolden display_max:  {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_hashes,
                expected_hashes,
                actual_bpms,
                expected_bpms,
                actual_min,
                expected_min,
                actual_max,
                expected_max,
                actual_display,
                expected_display,
                actual_display_min,
                expected_display_min,
                actual_display_max,
                expected_display_max
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

        let res = check_file(&path, &extension, &baseline_dir);
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
                "rerun: cargo test --test bpm_parity -- --exact {:?}",
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

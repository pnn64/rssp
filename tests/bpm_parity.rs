use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{analyze, AnalysisOptions};
use rssp::report::build_timing_snapshot;

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    bpms: String,
    hash_bpms: String,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartBpmInfo {
    step_type: String,
    difficulty: String,
    hash_bpms: String,
    bpms: String,
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

fn compute_chart_bpms(simfile_data: &[u8], extension: &str) -> Result<Vec<ChartBpmInfo>, String> {
    let simfile = analyze(simfile_data, extension, AnalysisOptions::default())
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();

    for chart in &simfile.charts {
        let step_type = chart.step_type_str.clone();
        let difficulty = rssp::normalize_difficulty_label(&chart.difficulty_str);
        let hash_bpms = chart
            .chart_bpms
            .clone()
            .unwrap_or_else(|| simfile.normalized_bpms.clone());
        let timing_bpms = build_timing_snapshot(chart, &simfile).bpms_formatted;

        results.push(ChartBpmInfo {
            step_type,
            difficulty,
            hash_bpms,
            bpms: timing_bpms,
        });
    }

    Ok(results)
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

            let hash_matches = expected_hash.is_some() && expected_hash == actual_hash;
            let bpms_matches = expected_bpms.is_some() && expected_bpms == actual_bpms;
            let status = if hash_matches && bpms_matches {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: hash_bpms: {} -> {} | bpms: {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_hash.unwrap_or("-"),
                actual_hash.unwrap_or("-"),
                expected_bpms.unwrap_or("-"),
                actual_bpms.unwrap_or("-"),
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries
                .iter()
                .zip(&actual_entries)
                .all(|(expected, actual)| {
                    expected.hash_bpms == actual.hash_bpms && expected.bpms == actual.bpms
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
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP hash_bpms:   {:?}\nGolden hash_bpms: {:?}\nRSSP bpms:        {:?}\nGolden bpms:      {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_hashes,
                expected_hashes,
                actual_bpms,
                expected_bpms
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

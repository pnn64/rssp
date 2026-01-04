use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{analyze, normalize_difficulty_label, AnalysisOptions};

#[derive(Debug, Deserialize)]
struct GoldenTechCounts {
    crossovers: u32,
    footswitches: u32,
    sideswitches: u32,
    jacks: u32,
    brackets: u32,
    doublesteps: u32,
}

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    tech_counts: Option<GoldenTechCounts>,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartTechCounts {
    step_type: String,
    difficulty: String,
    crossovers: u32,
    footswitches: u32,
    sideswitches: u32,
    jacks: u32,
    brackets: u32,
    doublesteps: u32,
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

fn compute_chart_tech_counts(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartTechCounts>, String> {
    let options = AnalysisOptions {
        mono_threshold: 6,
        ..AnalysisOptions::default()
    };
    let summary = analyze(simfile_data, extension, options).map_err(|e| e.to_string())?;
    let mut results = Vec::with_capacity(summary.charts.len());
    for chart in summary.charts {
        let counts = chart.tech_counts;
        results.push(ChartTechCounts {
            step_type: chart.step_type_str,
            difficulty: chart.difficulty_str,
            crossovers: counts.crossovers,
            footswitches: counts.footswitches,
            sideswitches: counts.sideswitches,
            jacks: counts.jacks,
            brackets: counts.brackets,
            doublesteps: counts.doublesteps,
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

    let rssp_charts = compute_chart_tech_counts(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let mut golden_map: HashMap<(String, String), Vec<GoldenChart>> = HashMap::new();
    for golden in golden_charts {
        let Some(key) = chart_key(&golden.step_type, &golden.difficulty) else {
            continue;
        };
        golden_map.entry(key).or_default().push(golden);
    }

    let mut rssp_map: HashMap<(String, String), Vec<ChartTechCounts>> = HashMap::new();
    for chart in rssp_charts {
        let Some(key) = chart_key(&chart.step_type, &chart.difficulty) else {
            continue;
        };
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

            let expected_counts = expected
                .and_then(|entry| entry.tech_counts.as_ref())
                .ok_or_else(|| {
                    format!(
                        "\n\nMISSING BASELINE TECH COUNTS\nFile: {}\nChart: {} {}\n",
                        path.display(),
                        step_type,
                        difficulty
                    )
                })?;

            let matches = actual.is_some()
                && expected_counts.crossovers == actual.map(|a| a.crossovers).unwrap_or(0)
                && expected_counts.footswitches == actual.map(|a| a.footswitches).unwrap_or(0)
                && expected_counts.sideswitches == actual.map(|a| a.sideswitches).unwrap_or(0)
                && expected_counts.jacks == actual.map(|a| a.jacks).unwrap_or(0)
                && expected_counts.brackets == actual.map(|a| a.brackets).unwrap_or(0)
                && expected_counts.doublesteps == actual.map(|a| a.doublesteps).unwrap_or(0);

            let status = if matches { "....ok" } else { "....MISMATCH" };

            println!(
                "  {} {} [{}]: crossovers {}->{} | footswitches {}->{} | sideswitches {}->{} | jacks {}->{} | brackets {}->{} | doublesteps {}->{} {}",
                step_type,
                difficulty,
                meter_label,
                expected_counts.crossovers,
                actual.map(|a| a.crossovers).unwrap_or(0),
                expected_counts.footswitches,
                actual.map(|a| a.footswitches).unwrap_or(0),
                expected_counts.sideswitches,
                actual.map(|a| a.sideswitches).unwrap_or(0),
                expected_counts.jacks,
                actual.map(|a| a.jacks).unwrap_or(0),
                expected_counts.brackets,
                actual.map(|a| a.brackets).unwrap_or(0),
                expected_counts.doublesteps,
                actual.map(|a| a.doublesteps).unwrap_or(0),
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries.iter().zip(&actual_entries).all(|(e, a)| {
                let Some(ref counts) = e.tech_counts else {
                    return false;
                };
                counts.crossovers == a.crossovers
                    && counts.footswitches == a.footswitches
                    && counts.sideswitches == a.sideswitches
                    && counts.jacks == a.jacks
                    && counts.brackets == a.brackets
                    && counts.doublesteps == a.doublesteps
            });
        if !matches {
            let expected_crossovers: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.crossovers))
                .collect();
            let actual_crossovers: Vec<u32> = actual_entries.iter().map(|a| a.crossovers).collect();
            let expected_footswitches: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.footswitches))
                .collect();
            let actual_footswitches: Vec<u32> = actual_entries.iter().map(|a| a.footswitches).collect();
            let expected_sideswitches: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.sideswitches))
                .collect();
            let actual_sideswitches: Vec<u32> = actual_entries.iter().map(|a| a.sideswitches).collect();
            let expected_jacks: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.jacks))
                .collect();
            let actual_jacks: Vec<u32> = actual_entries.iter().map(|a| a.jacks).collect();
            let expected_brackets: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.brackets))
                .collect();
            let actual_brackets: Vec<u32> = actual_entries.iter().map(|a| a.brackets).collect();
            let expected_doublesteps: Vec<u32> = expected_entries
                .iter()
                .filter_map(|e| e.tech_counts.as_ref().map(|c| c.doublesteps))
                .collect();
            let actual_doublesteps: Vec<u32> = actual_entries.iter().map(|a| a.doublesteps).collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP crossovers:   {:?}\nGolden crossovers: {:?}\nRSSP footswitches:  {:?}\nGolden footswitches: {:?}\nRSSP sideswitches:  {:?}\nGolden sideswitches: {:?}\nRSSP jacks:         {:?}\nGolden jacks:       {:?}\nRSSP brackets:      {:?}\nGolden brackets:    {:?}\nRSSP doublesteps:   {:?}\nGolden doublesteps: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_crossovers,
                expected_crossovers,
                actual_footswitches,
                expected_footswitches,
                actual_sideswitches,
                expected_sideswitches,
                actual_jacks,
                expected_jacks,
                actual_brackets,
                expected_brackets,
                actual_doublesteps,
                expected_doublesteps
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
                "rerun: cargo test --test tech_counts_parity -- --exact {:?}",
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

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{AnalysisOptions, analyze};

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    holds: u32,
    mines: u32,
    rolls: u32,
    notes: u32,
    lifts: u32,
    fakes: u32,
    jumps: u32,
    hands: u32,
    total_steps: u32,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartStepCounts {
    step_type: String,
    difficulty: String,
    holds: u32,
    mines: u32,
    rolls: u32,
    notes: u32,
    lifts: u32,
    fakes: u32,
    jumps: u32,
    hands: u32,
    total_steps: u32,
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

fn format_count(value: Option<u32>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn compute_chart_step_counts(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartStepCounts>, String> {
    let options = AnalysisOptions {
        compute_tech_counts: false,
        ..AnalysisOptions::default()
    };
    let summary = analyze(simfile_data, extension, options).map_err(|e| e.to_string())?;
    let mut results = Vec::new();
    for chart in summary.charts {
        results.push(ChartStepCounts {
            step_type: chart.step_type_str,
            difficulty: chart.difficulty_str,
            holds: chart.stats.holds,
            mines: chart.stats.mines,
            rolls: chart.stats.rolls,
            notes: chart.stats.total_arrows,
            lifts: chart.stats.lifts,
            fakes: chart.stats.fakes,
            jumps: chart.stats.jumps,
            hands: chart.stats.hands,
            total_steps: chart.stats.total_steps,
        });
    }
    Ok(results)
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let (raw_bytes, ext) = if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zst"))
    {
        let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
        let raw_bytes = zstd::decode_all(&compressed_bytes[..])
            .map_err(|e| format!("Failed to decompress simfile: {}", e))?;
        (raw_bytes, extension)
    } else {
        let sim = rssp::simfile::open(path).map_err(|e| format!("Failed to read file: {}", e))?;
        (sim.data, sim.extension)
    };

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

    let compressed_golden =
        fs::read(&golden_path).map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let golden_charts: Vec<GoldenChart> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_charts = compute_chart_step_counts(&raw_bytes, ext)
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

    let mut rssp_map: HashMap<(String, String), Vec<ChartStepCounts>> = HashMap::new();
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

            let holds = field("holds", expected.map(|e| e.holds), actual.map(|a| a.holds));
            let mines = field("mines", expected.map(|e| e.mines), actual.map(|a| a.mines));
            let rolls = field("rolls", expected.map(|e| e.rolls), actual.map(|a| a.rolls));
            let notes = field("notes", expected.map(|e| e.notes), actual.map(|a| a.notes));
            let lifts = field("lifts", expected.map(|e| e.lifts), actual.map(|a| a.lifts));
            let fakes = field("fakes", expected.map(|e| e.fakes), actual.map(|a| a.fakes));
            let jumps = field("jumps", expected.map(|e| e.jumps), actual.map(|a| a.jumps));
            let hands = field("hands", expected.map(|e| e.hands), actual.map(|a| a.hands));
            let total_steps = field(
                "total_steps",
                expected.map(|e| e.total_steps),
                actual.map(|a| a.total_steps),
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

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries.iter().zip(&actual_entries).all(|(e, a)| {
                e.holds == a.holds
                    && e.mines == a.mines
                    && e.rolls == a.rolls
                    && e.notes == a.notes
                    && e.lifts == a.lifts
                    && e.fakes == a.fakes
                    && e.jumps == a.jumps
                    && e.hands == a.hands
                    && e.total_steps == a.total_steps
            });
        if !matches {
            let expected_notes: Vec<u32> = expected_entries.iter().map(|e| e.notes).collect();
            let actual_notes: Vec<u32> = actual_entries.iter().map(|a| a.notes).collect();
            let expected_steps: Vec<u32> = expected_entries.iter().map(|e| e.total_steps).collect();
            let actual_steps: Vec<u32> = actual_entries.iter().map(|a| a.total_steps).collect();
            let expected_holds: Vec<u32> = expected_entries.iter().map(|e| e.holds).collect();
            let actual_holds: Vec<u32> = actual_entries.iter().map(|a| a.holds).collect();
            let expected_mines: Vec<u32> = expected_entries.iter().map(|e| e.mines).collect();
            let actual_mines: Vec<u32> = actual_entries.iter().map(|a| a.mines).collect();
            let expected_rolls: Vec<u32> = expected_entries.iter().map(|e| e.rolls).collect();
            let actual_rolls: Vec<u32> = actual_entries.iter().map(|a| a.rolls).collect();
            let expected_lifts: Vec<u32> = expected_entries.iter().map(|e| e.lifts).collect();
            let actual_lifts: Vec<u32> = actual_entries.iter().map(|a| a.lifts).collect();
            let expected_fakes: Vec<u32> = expected_entries.iter().map(|e| e.fakes).collect();
            let actual_fakes: Vec<u32> = actual_entries.iter().map(|a| a.fakes).collect();
            let expected_jumps: Vec<u32> = expected_entries.iter().map(|e| e.jumps).collect();
            let actual_jumps: Vec<u32> = actual_entries.iter().map(|a| a.jumps).collect();
            let expected_hands: Vec<u32> = expected_entries.iter().map(|e| e.hands).collect();
            let actual_hands: Vec<u32> = actual_entries.iter().map(|a| a.hands).collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP notes:      {:?}\nGolden notes:    {:?}\nRSSP total_steps: {:?}\nGolden total_steps: {:?}\nRSSP holds:      {:?}\nGolden holds:    {:?}\nRSSP mines:      {:?}\nGolden mines:    {:?}\nRSSP rolls:      {:?}\nGolden rolls:    {:?}\nRSSP lifts:      {:?}\nGolden lifts:    {:?}\nRSSP fakes:      {:?}\nGolden fakes:    {:?}\nRSSP jumps:      {:?}\nGolden jumps:    {:?}\nRSSP hands:      {:?}\nGolden hands:    {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_notes,
                expected_notes,
                actual_steps,
                expected_steps,
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
        let extension = if ext.eq_ignore_ascii_case("zst") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let inner_path = Path::new(stem);
            let inner_extension = inner_path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();

            if inner_extension != "sm" && inner_extension != "ssc" {
                continue;
            }
            inner_extension
        } else if ext.eq_ignore_ascii_case("sm") {
            "sm".to_string()
        } else if ext.eq_ignore_ascii_case("ssc") {
            "ssc".to_string()
        } else {
            continue;
        };

        let test_name = path
            .strip_prefix(&packs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        tests.push(TestCase {
            name: test_name,
            path: path.to_path_buf(),
            extension,
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
                "rerun: cargo test --test step_counts_parity -- --exact {:?}",
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

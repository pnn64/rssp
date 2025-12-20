use std::fs;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use serde::Deserialize;
use walkdir::WalkDir;
use libtest_mimic::Arguments;

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    hash: String,
    #[serde(default)]
    meter: Option<u32>,
}

fn main() {
    let args = Arguments::from_args();

    // 1. Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Assuming the data submodule is mounted at 'tests/data'
    let packs_dir = manifest_dir.join("tests/data/packs");
    let baseline_dir = manifest_dir.join("tests/data/baseline");

    if !packs_dir.exists() {
        println!("No tests/packs directory found.");
        return;
    }

    // 2. Collect all test cases
    let mut tests = Vec::new();

    for entry in WalkDir::new(&packs_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        // Check for .zst extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "zst" {
            continue;
        }

        // Check the "inner" extension (e.g. "file.sm.zst" -> "sm")
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let inner_path = Path::new(stem);
        let inner_extension = inner_path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if inner_extension != "sm" && inner_extension != "ssc" {
            continue;
        }

        // Create a pretty name: "PackName/SongName/file.ssc.zst"
        let test_name = path.strip_prefix(&packs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        tests.push(TestCase {
            name: test_name,
            path: path.to_path_buf(),
            extension: inner_extension,
        });
    }

    // Keep test discovery order stable (WalkDir / filesystem order is not guaranteed).
    tests.sort_by(|a, b| a.name.cmp(&b.name));

    // Apply CLI filters (compatible with libtest / libtest-mimic).
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
        // We don't define any ignored tests; this matches libtest semantics.
        tests.clear();
    }

    if args.list {
        for t in &tests {
            println!("{}", t.name);
        }
        return;
    }

    // 4. Run tests (serially; one simfile must fully validate before the next starts).
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

        // Make CI logs stream predictably.
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
                "rerun: cargo test --test hash_parity -- --exact {:?}",
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

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    // 1. Read Compressed Simfile
    let compressed_bytes = fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    // 2. Decompress Simfile
    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {}", e))?;
    
    // 3. Compute Hash (on raw bytes) to find Baseline JSON
    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    
    // Determine sharded subfolder (first 2 chars of hash)
    let subfolder = &file_hash[0..2];

    // Look for baseline/{xx}/{hash}.json.zst
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

    // 4. Read & Decompress Golden JSON
    let compressed_golden = fs::read(&golden_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;
    
    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let golden_charts: Vec<GoldenChart> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    // 5. Run RSSP FAST Hashing (using decompressed raw_bytes)
    let rssp_charts = rssp::compute_all_hashes(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    // 6. Compare Charts (support multiple edits per difficulty)
    let mut golden_map: HashMap<(String, String), Vec<(String, Option<u32>)>> = HashMap::new();
    for golden in golden_charts {
        let step_type_lower = golden.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let difficulty = rssp::normalize_difficulty_label(&golden.difficulty);
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        golden_map
            .entry(key)
            .or_default()
            .push((golden.hash, golden.meter));
    }

    let mut rssp_map: HashMap<(String, String), Vec<String>> = HashMap::new();
    for chart in rssp_charts {
        let step_type_lower = chart.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let key = (
            step_type_lower,
            chart.difficulty.to_ascii_lowercase(),
        );
        rssp_map.entry(key).or_default().push(chart.hash);
    }

    let mut golden_entries: Vec<_> = golden_map.into_iter().collect();
    golden_entries.sort_by(|a, b| a.0.cmp(&b.0));

    println!("File: {}", path.display());

    for ((step_type, difficulty), expected_entries) in golden_entries {
        let Some(actual_hashes) = rssp_map.remove(&(step_type.clone(), difficulty.clone())) else {
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

        let count = expected_entries.len().max(actual_hashes.len());
        for idx in 0..count {
            let expected = expected_entries.get(idx).map(|(hash, _)| hash.as_str());
            let actual = actual_hashes.get(idx).map(|s| s.as_str());
            let meter_label = expected_entries
                .get(idx)
                .and_then(|(_, meter)| *meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());
            let status = if expected.is_some() && expected == actual {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  {} {} [{}]: baseline: {} -> rssp: {} {}",
                step_type,
                difficulty,
                meter_label,
                expected.unwrap_or("-"),
                actual.unwrap_or("-"),
                status
            );
        }

        let matches = expected_entries.len() == actual_hashes.len()
            && expected_entries
                .iter()
                .zip(&actual_hashes)
                .all(|((expected_hash, _), actual_hash)| expected_hash == actual_hash);
        if !matches {
            let expected_hashes: Vec<String> = expected_entries
                .iter()
                .map(|(hash, _)| hash.clone())
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
        continue;
    }

    Ok(())
}

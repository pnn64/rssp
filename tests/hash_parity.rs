use std::fs;
use std::path::{Path, PathBuf};
use rssp; // Ensure rssp::compute_all_hashes is available in lib.rs
use serde::Deserialize;
use walkdir::WalkDir;
use libtest_mimic::{Arguments, Trial, Failed};

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    hash: String,
}

fn main() {
    let args = Arguments::from_args();

    // 1. Setup paths
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = manifest_dir.join("tests/packs");
    let golden_dir = manifest_dir.join("tests/golden");

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

        let extension = path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if extension != "sm" && extension != "ssc" {
            continue;
        }

        // Create a pretty name for the test: "PackName/SongName/file.ssc"
        let test_name = path.strip_prefix(&packs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let path_clone = path.to_path_buf();
        let golden_dir_clone = golden_dir.clone();
        let extension_clone = extension.clone();

        // 3. Create a Trial for this file
        let trial = Trial::test(test_name, move || {
            check_file(&path_clone, &extension_clone, &golden_dir_clone)
        });

        tests.push(trial);
    }

    // 4. Run tests
    libtest_mimic::run(&args, tests).exit();
}

fn check_file(path: &Path, extension: &str, golden_dir: &Path) -> Result<(), Failed> {
    // 1. Read File
    let raw_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;
    
    // 2. Compute Hash to find Golden JSON
    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let golden_path = golden_dir.join(format!("{}.json", file_hash));

    if !golden_path.exists() {
        // Return Ok to skip silently.
        // If you want to see skips, use: println!("Skipping {}", path.display());
        return Ok(()); 
    }

    // 3. Parse Golden Data
    let json_content = fs::read_to_string(&golden_path)
        .map_err(|e| format!("Failed to read golden JSON: {}", e))?;
    
    let golden_charts: Vec<GoldenChart> = serde_json::from_str(&json_content)
        .map_err(|e| format!("Failed to parse golden JSON: {}", e))?;

    // 4. Run RSSP FAST Hashing
    // This calls the lightweight function we added to lib.rs
    let rssp_charts = rssp::compute_all_hashes(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    // 5. Compare Charts
    for golden in golden_charts {
        // --- FILTER: Only dance-single ---
        if !golden.step_type.eq_ignore_ascii_case("dance-single") {
            continue;
        }

        // --- FILTER: No Edits ---
        if golden.difficulty.eq_ignore_ascii_case("edit") {
            continue;
        }

        let match_opt = rssp_charts.iter().find(|c| 
            c.difficulty.eq_ignore_ascii_case(&golden.difficulty) &&
            c.step_type.eq_ignore_ascii_case(&golden.step_type)
        );

        if let Some(chart) = match_opt {
            if chart.hash != golden.hash {
                return Err(Failed::from(format!(
                    "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP Hash:   {}\nGolden Hash: {}\n",
                    path.display(),
                    golden.step_type,
                    golden.difficulty,
                    chart.hash,
                    golden.hash
                )));
            }
        } else {
            return Err(Failed::from(format!(
                "\n\nMISSING CHART DETECTED\nFile: {}\nExpected: {} {}\n",
                path.display(),
                golden.step_type,
                golden.difficulty
            )));
        }
    }

    Ok(())
}
//! Per-row StepParity annotation parity against ITGmania.
//!
//! This is the row-level companion to `tech_counts_parity`. Where that test
//! proves the *aggregate* crossover/footswitch/etc. counts match ITGmania, this
//! one proves the *per-row* annotation matches: for every annotated row it
//! checks the beat, the foot-bearing columns (ITGmania's `footPlacement` keys),
//! the per-column foot identity (`feet`), the foot count, and the full per-row
//! tech (`tech_counts`). The `feet` / `tech_counts` checks engage only when the
//! baseline carries them (older baselines stay green).
//!
//! Ground truth comes from the `itgmania-reference-harness`, which calls
//! `Steps:GetNoteAnnotations()` on the real engine and writes a
//! `note_annotations` array into each chart's baseline JSON. rssp produces the
//! same data via `analyze` with `compute_note_annotations` enabled
//! (`ChartSummary::note_annotations`).
//!
//! Forward compatible: golden charts without a `note_annotations` field are
//! skipped, so this test stays green until the harness baselines are
//! regenerated with annotation data.
//!
//! Corpus + baseline locations default to `tests/data/{packs,baseline}` but can
//! be overridden so you can point at any song library, e.g.:
//!   RSSP_PARITY_PACKS_DIR=D:/github/deadsync-0/target/local/songs \
//!   RSSP_PARITY_BASELINE_DIR=D:/path/to/baselines \
//!   cargo test --test note_annotations_parity

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{AnalysisOptions, RowAnnotation, TechCounts, analyze, normalize_difficulty_label};

#[derive(Debug, Default, Deserialize, PartialEq)]
struct GoldenTechCounts {
    #[serde(default)]
    crossovers: u32,
    #[serde(default)]
    half_crossovers: u32,
    #[serde(default)]
    full_crossovers: u32,
    #[serde(default)]
    footswitches: u32,
    #[serde(default)]
    up_footswitches: u32,
    #[serde(default)]
    down_footswitches: u32,
    #[serde(default)]
    sideswitches: u32,
    #[serde(default)]
    jacks: u32,
    #[serde(default)]
    brackets: u32,
    #[serde(default)]
    doublesteps: u32,
}

#[derive(Debug, Deserialize)]
struct GoldenNoteAnnotation {
    beat: f32,
    /// Foot-bearing columns, 0-indexed (ITGmania `footPlacement` keys minus 1).
    columns: Vec<u8>,
    /// Foot id assigned to each column, parallel to `columns` (absent on older
    /// baselines -> the foot-identity check is skipped for that chart).
    #[serde(default)]
    feet: Option<Vec<u8>>,
    #[serde(default)]
    note_count: Option<u8>,
    /// Full per-row tech counts (absent on older baselines -> tech check skipped).
    #[serde(default)]
    tech_counts: Option<GoldenTechCounts>,
}

#[derive(Debug, Deserialize)]
struct GoldenChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    #[serde(default)]
    meter: Option<u32>,
    /// Absent on older baselines (generated before the harness emitted
    /// annotations) -> chart is skipped rather than failed.
    #[serde(default)]
    note_annotations: Option<Vec<GoldenNoteAnnotation>>,
}

#[derive(Debug, Clone)]
struct ChartAnnotations {
    step_type: String,
    difficulty: String,
    annotations: Vec<RowAnnotation>,
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

const BEAT_EPSILON: f32 = 1.0e-3;

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

/// Foot-bearing columns of a row as a sorted 0-indexed list (the rssp analog of
/// ITGmania's `footPlacement` keys).
fn columns_of(annotation: &RowAnnotation) -> Vec<u8> {
    let mut cols = Vec::with_capacity(annotation.column_mask.count_ones() as usize);
    let mut mask = annotation.column_mask;
    while mask != 0 {
        let c = mask.trailing_zeros() as u8;
        cols.push(c);
        mask &= mask - 1;
    }
    cols
}

/// Foot id assigned to each foot-bearing column, parallel to [`columns_of`]
/// (the rssp analog of ITGmania's `footPlacement` values).
fn feet_of(annotation: &RowAnnotation) -> Vec<u8> {
    columns_of(annotation)
        .into_iter()
        .map(|c| annotation.foot(c as usize) as u8)
        .collect()
}

/// True when the golden per-row tech counts equal rssp's.
fn tech_matches(g: &GoldenTechCounts, t: &TechCounts) -> bool {
    g.crossovers == t.crossovers
        && g.half_crossovers == t.half_crossovers
        && g.full_crossovers == t.full_crossovers
        && g.footswitches == t.footswitches
        && g.up_footswitches == t.up_footswitches
        && g.down_footswitches == t.down_footswitches
        && g.sideswitches == t.sideswitches
        && g.jacks == t.jacks
        && g.brackets == t.brackets
        && g.doublesteps == t.doublesteps
}

fn compute_chart_annotations(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartAnnotations>, String> {
    let options = AnalysisOptions {
        mono_threshold: 6,
        compute_note_annotations: true,
        ..AnalysisOptions::default()
    };
    let summary = analyze(simfile_data, extension, &options)?;
    let mut results = Vec::with_capacity(summary.charts.len());
    for chart in summary.charts {
        results.push(ChartAnnotations {
            step_type: chart.step_type_str,
            difficulty: chart.difficulty_str,
            annotations: chart.note_annotations.unwrap_or_default(),
        });
    }
    Ok(results)
}

/// Compare one chart's worth of annotations. Returns a human-readable mismatch
/// description on failure, `None` on success.
fn diff_annotations(
    label: &str,
    expected: &[GoldenNoteAnnotation],
    actual: &[RowAnnotation],
) -> Option<String> {
    if expected.len() != actual.len() {
        return Some(format!(
            "{label}: row count differs (golden {} rows, rssp {} rows)",
            expected.len(),
            actual.len()
        ));
    }

    for (idx, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
        if (e.beat - a.beat).abs() > BEAT_EPSILON {
            return Some(format!(
                "{label}: row {idx} beat differs (golden {:.4}, rssp {:.4})",
                e.beat, a.beat
            ));
        }
        let actual_cols = columns_of(a);
        let mut expected_cols = e.columns.clone();
        expected_cols.sort_unstable();
        if expected_cols != actual_cols {
            return Some(format!(
                "{label}: row {idx} (beat {:.4}) columns differ (golden {:?}, rssp {:?})",
                e.beat, expected_cols, actual_cols
            ));
        }
        if let Some(expected_count) = e.note_count {
            if u32::from(expected_count) != a.foot_count() {
                return Some(format!(
                    "{label}: row {idx} (beat {:.4}) note_count differs (golden {}, rssp {})",
                    e.beat,
                    expected_count,
                    a.foot_count()
                ));
            }
        }
        if let Some(expected_feet) = &e.feet {
            if expected_feet.len() == e.columns.len() {
                // Align golden feet to sorted columns, matching feet_of's order.
                let mut pairs: Vec<(u8, u8)> = e
                    .columns
                    .iter()
                    .copied()
                    .zip(expected_feet.iter().copied())
                    .collect();
                pairs.sort_by_key(|p| p.0);
                let expected_feet_sorted: Vec<u8> = pairs.iter().map(|p| p.1).collect();
                let actual_feet = feet_of(a);
                if expected_feet_sorted != actual_feet {
                    return Some(format!(
                        "{label}: row {idx} (beat {:.4}) feet differ (golden {:?}, rssp {:?})",
                        e.beat, expected_feet_sorted, actual_feet
                    ));
                }
            }
        }
        if let Some(expected_tech) = &e.tech_counts {
            if !tech_matches(expected_tech, &a.row_tech) {
                return Some(format!(
                    "{label}: row {idx} (beat {:.4}) tech_counts differ (golden {:?}, rssp {:?})",
                    e.beat, expected_tech, a.row_tech
                ));
            }
        }
    }

    None
}

fn read_simfile_bytes(path: &Path, extension: &str) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("Failed to read file: {e}"))?;
    if extension == "zst" {
        zstd::decode_all(&bytes[..]).map_err(|e| format!("Failed to decompress simfile: {e}"))
    } else {
        Ok(bytes)
    }
}

/// `extension` is the outer file extension; returns the simfile-format extension
/// (`sm`/`ssc`) used by `analyze`.
fn inner_extension(path: &Path, extension: &str) -> String {
    if extension == "zst" {
        Path::new(path.file_stem().and_then(|s| s.to_str()).unwrap_or(""))
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default()
    } else {
        extension.to_ascii_lowercase()
    }
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let raw_bytes = read_simfile_bytes(path, extension)?;
    let format_ext = inner_extension(path, extension);

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];
    let golden_path = baseline_dir
        .join(subfolder)
        .join(format!("{file_hash}.json.zst"));

    if !golden_path.exists() {
        // No baseline for this simfile -> nothing to assert against. Skip
        // rather than fail so partial corpora work.
        return Ok(());
    }

    let compressed_golden =
        fs::read(&golden_path).map_err(|e| format!("Failed to read baseline file: {e}"))?;
    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {e}"))?;
    let golden_charts: Vec<GoldenChart> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {e}"))?;

    // If no golden chart carries annotation data, this baseline predates the
    // harness change -> skip silently.
    let any_annotations = golden_charts
        .iter()
        .any(|c| c.note_annotations.as_ref().is_some_and(|a| !a.is_empty()));
    if !any_annotations {
        return Ok(());
    }

    let rssp_charts = compute_chart_annotations(&raw_bytes, &format_ext)
        .map_err(|e| format!("RSSP Parsing Error: {e}"))?;

    let mut rssp_map: HashMap<(String, String), Vec<ChartAnnotations>> = HashMap::new();
    for chart in rssp_charts {
        let Some(key) = chart_key(&chart.step_type, &chart.difficulty) else {
            continue;
        };
        rssp_map.entry(key).or_default().push(chart);
    }

    println!("File: {}", path.display());

    for golden in &golden_charts {
        let Some(expected) = golden.note_annotations.as_ref() else {
            continue;
        };
        if expected.is_empty() {
            continue;
        }
        let Some(key) = chart_key(&golden.step_type, &golden.difficulty) else {
            continue;
        };
        let (step_type, difficulty) = key.clone();
        let meter_label = golden
            .meter
            .map_or_else(|| "?".to_string(), |m| m.to_string());

        let Some(candidates) = rssp_map.get(&key) else {
            return Err(format!(
                "\n\nMISSING CHART\nFile: {}\nExpected: {} {} [{}]\n",
                path.display(),
                step_type,
                difficulty,
                meter_label
            ));
        };

        // A simfile can hold several charts at the same (type, difficulty)
        // (e.g. Edit slots); accept if any rssp chart matches row-for-row.
        let mut last_mismatch = None;
        let matched = candidates.iter().any(|cand| {
            match diff_annotations(
                &format!("{step_type} {difficulty} [{meter_label}]"),
                expected,
                &cand.annotations,
            ) {
                None => true,
                Some(msg) => {
                    last_mismatch = Some(msg);
                    false
                }
            }
        });

        if matched {
            println!("  {step_type} {difficulty} [{meter_label}]: {} rows ....ok", expected.len());
        } else {
            println!("  {step_type} {difficulty} [{meter_label}] ....MISMATCH");
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
                path.display(),
                last_mismatch.unwrap_or_else(|| "no rssp chart for key".to_string())
            ));
        }
    }

    Ok(())
}

fn resolve_dir(env_key: &str, default: PathBuf) -> PathBuf {
    std::env::var_os(env_key).map_or(default, PathBuf::from)
}

fn main() {
    let args = Arguments::from_args();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = resolve_dir("RSSP_PARITY_PACKS_DIR", manifest_dir.join("tests/data/packs"));
    let baseline_dir = resolve_dir(
        "RSSP_PARITY_BASELINE_DIR",
        manifest_dir.join("tests/data/baseline"),
    );

    if !packs_dir.exists() {
        println!("No packs directory found at {}.", packs_dir.display());
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

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .unwrap_or_default();

        // Accept compressed (.zst wrapping .sm/.ssc) or raw simfiles.
        let outer_ext = if ext == "zst" {
            let inner = inner_extension(path, "zst");
            if inner != "sm" && inner != "ssc" {
                continue;
            }
            "zst".to_string()
        } else if ext == "sm" || ext == "ssc" {
            ext
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
            extension: outer_ext,
        });
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));

    let tests: Vec<_> = tests
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

        match check_file(&path, &extension, &baseline_dir) {
            Ok(()) => {
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
                "rerun: cargo test --test note_annotations_parity -- --exact {:?}",
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

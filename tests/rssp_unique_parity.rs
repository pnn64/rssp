use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::patterns::{BoxCounts, PatternVariant, compute_box_counts, count_pattern};
use rssp::report::format_json_float;
use rssp::{AnalysisOptions, ChartSummary, analyze};

const DEFAULT_MONO_THRESHOLD: usize = 6;

#[derive(Debug, Deserialize)]
struct GoldenFile {
    charts: Vec<GoldenChart>,
}

#[derive(Debug, Deserialize)]
struct GoldenChart {
    chart_info: GoldenChartInfo,
    breakdown: Breakdown,
    mono_candle_stats: GoldenMonoCandleStats,
    pattern_counts: PatternCounts,
}

#[derive(Debug, Deserialize)]
struct GoldenChartInfo {
    step_type: String,
    difficulty: String,
    rating: String,
    matrix_rating: f64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct Breakdown {
    #[serde(alias = "detailed_breakdown")]
    sn_detailed_breakdown: String,
    #[serde(alias = "partial_breakdown")]
    sn_partial_breakdown: String,
    #[serde(alias = "simple_breakdown")]
    sn_simple_breakdown: String,
}

#[derive(Debug, Deserialize)]
struct GoldenMonoCandleStats {
    total_candles: u32,
    left_foot_candles: u32,
    right_foot_candles: u32,
    candles_percent: f64,
    total_mono: u32,
    left_face_mono: u32,
    right_face_mono: u32,
    mono_percent: f64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct PatternCounts {
    boxes: BoxesCounts,
    anchors: AnchorsCounts,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct BoxesCounts {
    total_boxes: u32,
    lr_boxes: u32,
    ud_boxes: u32,
    corner_boxes: u32,
    ld_boxes: u32,
    lu_boxes: u32,
    rd_boxes: u32,
    ru_boxes: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct AnchorsCounts {
    total_anchors: u32,
    left_anchors: u32,
    down_anchors: u32,
    up_anchors: u32,
    right_anchors: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct MonoCandleStats {
    total_candles: u32,
    left_foot_candles: u32,
    right_foot_candles: u32,
    candles_percent: String,
    total_mono: u32,
    left_face_mono: u32,
    right_face_mono: u32,
    mono_percent: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ChartUniqueValues {
    matrix_rating: String,
    breakdown: Breakdown,
    mono_candle_stats: MonoCandleStats,
    pattern_counts: PatternCounts,
}

#[derive(Debug, Clone)]
struct ChartSnapshot {
    rating: String,
    values: ChartUniqueValues,
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

fn format_candles(stats: Option<&MonoCandleStats>) -> String {
    stats
        .map(|s| {
            format!(
                "{} (L {} R {}) {}%",
                s.total_candles, s.left_foot_candles, s.right_foot_candles, s.candles_percent
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_mono(stats: Option<&MonoCandleStats>) -> String {
    stats
        .map(|s| {
            format!(
                "{} (L {} R {}) {}%",
                s.total_mono, s.left_face_mono, s.right_face_mono, s.mono_percent
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_boxes(patterns: Option<&PatternCounts>) -> String {
    patterns
        .map(|p| {
            let b = &p.boxes;
            format!(
                "{} (LR {} UD {} LD {} LU {} RD {} RU {})",
                b.total_boxes,
                b.lr_boxes,
                b.ud_boxes,
                b.ld_boxes,
                b.lu_boxes,
                b.rd_boxes,
                b.ru_boxes
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn format_anchors(patterns: Option<&PatternCounts>) -> String {
    patterns
        .map(|p| {
            let a = &p.anchors;
            format!(
                "{} (L {} D {} U {} R {})",
                a.total_anchors, a.left_anchors, a.down_anchors, a.up_anchors, a.right_anchors
            )
        })
        .unwrap_or_else(|| "-".to_string())
}

fn chart_values_from_summary(chart: &ChartSummary) -> ChartUniqueValues {
    let patterns = &chart.detected_patterns;
    let left_foot_candles = count_pattern(patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count_pattern(patterns, PatternVariant::CandleRight);
    let box_counts: BoxCounts = compute_box_counts(patterns);

    ChartUniqueValues {
        matrix_rating: format_json_float(chart.matrix_rating),
        breakdown: Breakdown {
            sn_detailed_breakdown: chart.sn_detailed_breakdown.clone(),
            sn_partial_breakdown: chart.sn_partial_breakdown.clone(),
            sn_simple_breakdown: chart.sn_simple_breakdown.clone(),
        },
        mono_candle_stats: MonoCandleStats {
            total_candles: left_foot_candles + right_foot_candles,
            left_foot_candles,
            right_foot_candles,
            candles_percent: format_json_float(chart.candle_percent),
            total_mono: chart.mono_total,
            left_face_mono: chart.facing_left,
            right_face_mono: chart.facing_right,
            mono_percent: format_json_float(chart.mono_percent),
        },
        pattern_counts: PatternCounts {
            boxes: BoxesCounts {
                total_boxes: box_counts.total_boxes,
                lr_boxes: box_counts.lr_boxes,
                ud_boxes: box_counts.ud_boxes,
                corner_boxes: box_counts.corner_boxes,
                ld_boxes: box_counts.ld_boxes,
                lu_boxes: box_counts.lu_boxes,
                rd_boxes: box_counts.rd_boxes,
                ru_boxes: box_counts.ru_boxes,
            },
            anchors: AnchorsCounts {
                total_anchors: chart.anchor_left
                    + chart.anchor_down
                    + chart.anchor_up
                    + chart.anchor_right,
                left_anchors: chart.anchor_left,
                down_anchors: chart.anchor_down,
                up_anchors: chart.anchor_up,
                right_anchors: chart.anchor_right,
            },
        },
    }
}

fn chart_values_from_golden(chart: &GoldenChart) -> ChartUniqueValues {
    ChartUniqueValues {
        matrix_rating: format_json_float(chart.chart_info.matrix_rating),
        breakdown: chart.breakdown.clone(),
        mono_candle_stats: MonoCandleStats {
            total_candles: chart.mono_candle_stats.total_candles,
            left_foot_candles: chart.mono_candle_stats.left_foot_candles,
            right_foot_candles: chart.mono_candle_stats.right_foot_candles,
            candles_percent: format_json_float(chart.mono_candle_stats.candles_percent),
            total_mono: chart.mono_candle_stats.total_mono,
            left_face_mono: chart.mono_candle_stats.left_face_mono,
            right_face_mono: chart.mono_candle_stats.right_face_mono,
            mono_percent: format_json_float(chart.mono_candle_stats.mono_percent),
        },
        pattern_counts: chart.pattern_counts.clone(),
    }
}

fn compute_chart_values(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<(String, String, ChartSnapshot)>, String> {
    let options = AnalysisOptions {
        strip_tags: false,
        mono_threshold: DEFAULT_MONO_THRESHOLD,
        custom_patterns: Vec::new(),
        compute_tech_counts: true,
        compute_pattern_counts: true,
        translate_markers: false,
    };

    let summary = analyze(simfile_data, extension, options).map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    for chart in &summary.charts {
        let step_type = chart.step_type_str.clone();
        if step_type == "lights-cabinet" {
            continue;
        }
        let difficulty = chart.difficulty_str.clone();
        results.push((
            step_type,
            difficulty,
            ChartSnapshot {
                rating: chart.rating_str.clone(),
                values: chart_values_from_summary(chart),
            },
        ));
    }

    Ok(results)
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let compressed_bytes = fs::read(path).map_err(|e| format!("Failed to read file: {}", e))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {}", e))?;

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];

    let golden_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.rssp.json.zst", file_hash));

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

    let golden_file: GoldenFile = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_charts = compute_chart_values(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let mut golden_map: HashMap<(String, String), Vec<ChartSnapshot>> = HashMap::new();
    for golden in golden_file.charts {
        let step_type = golden.chart_info.step_type.clone();
        let difficulty = rssp::normalize_difficulty_label(&golden.chart_info.difficulty);
        let step_type_lower = step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        golden_map.entry(key).or_default().push(ChartSnapshot {
            rating: golden.chart_info.rating.clone(),
            values: chart_values_from_golden(&golden),
        });
    }

    let mut rssp_map: HashMap<(String, String), Vec<ChartSnapshot>> = HashMap::new();
    for (step_type, difficulty, snapshot) in rssp_charts {
        let step_type_lower = step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        rssp_map.entry(key).or_default().push(snapshot);
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
                .map(|entry| entry.rating.as_str())
                .filter(|label| !label.is_empty())
                .or_else(|| {
                    actual
                        .map(|entry| entry.rating.as_str())
                        .filter(|label| !label.is_empty())
                })
                .map(|label| label.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());

            let expected_values = expected.map(|entry| &entry.values);
            let actual_values = actual.map(|entry| &entry.values);
            let matches = expected_values.is_some()
                && actual_values.is_some()
                && expected_values == actual_values;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_matrix = expected_values
                .map(|v| v.matrix_rating.as_str())
                .unwrap_or("-");
            let actual_matrix = actual_values
                .map(|v| v.matrix_rating.as_str())
                .unwrap_or("-");
            let expected_detail = expected_values
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual_values
                .map(|v| v.breakdown.sn_detailed_breakdown.as_str())
                .unwrap_or("-");
            let expected_partial = expected_values
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let actual_partial = actual_values
                .map(|v| v.breakdown.sn_partial_breakdown.as_str())
                .unwrap_or("-");
            let expected_simple = expected_values
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let actual_simple = actual_values
                .map(|v| v.breakdown.sn_simple_breakdown.as_str())
                .unwrap_or("-");
            let expected_candles = format_candles(expected_values.map(|v| &v.mono_candle_stats));
            let actual_candles = format_candles(actual_values.map(|v| &v.mono_candle_stats));
            let expected_mono = format_mono(expected_values.map(|v| &v.mono_candle_stats));
            let actual_mono = format_mono(actual_values.map(|v| &v.mono_candle_stats));
            let expected_boxes = format_boxes(expected_values.map(|v| &v.pattern_counts));
            let actual_boxes = format_boxes(actual_values.map(|v| &v.pattern_counts));
            let expected_anchors = format_anchors(expected_values.map(|v| &v.pattern_counts));
            let actual_anchors = format_anchors(actual_values.map(|v| &v.pattern_counts));

            println!(
                "  {} {} [{}]: matrix_rating {} -> {} | sn_detailed {} -> {} | sn_partial {} -> {} | sn_simple {} -> {} | candles {} -> {} | mono {} -> {} | boxes {} -> {} | anchors {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_matrix,
                actual_matrix,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_candles,
                actual_candles,
                expected_mono,
                actual_mono,
                expected_boxes,
                actual_boxes,
                expected_anchors,
                actual_anchors,
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries
                .iter()
                .zip(&actual_entries)
                .all(|(expected, actual)| expected.values == actual.values);
        if !matches {
            let expected_values: Vec<ChartUniqueValues> = expected_entries
                .iter()
                .map(|entry| entry.values.clone())
                .collect();
            let actual_values: Vec<ChartUniqueValues> = actual_entries
                .iter()
                .map(|entry| entry.values.clone())
                .collect();
            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP values:   {:?}\nGolden values: {:?}\n",
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
                "rerun: cargo test --test rssp_unique_parity -- --exact {:?}",
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

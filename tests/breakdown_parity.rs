use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{AnalysisOptions, analyze};

#[derive(Debug, Deserialize)]
struct HarnessChart {
    difficulty: String,
    #[serde(rename = "steps_type")]
    step_type: String,
    streams_breakdown: String,
    streams_breakdown_level1: String,
    streams_breakdown_level2: String,
    total_stream_measures: u32,
    total_break_measures: u32,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RsspGoldenFile {
    charts: Vec<RsspGoldenChart>,
}

#[derive(Debug, Deserialize)]
struct RsspGoldenChart {
    chart_info: RsspChartInfo,
    breakdown: SnBreakdown,
    stream_info: RsspStreamInfo,
}

#[derive(Debug, Deserialize)]
struct RsspChartInfo {
    step_type: String,
    difficulty: String,
    rating: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct SnBreakdown {
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
}

#[derive(Debug, Deserialize)]
struct RsspStreamInfo {
    sn_breaks: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct BreakdownSet {
    detailed: String,
    partial: String,
    simple: String,
}

#[derive(Debug, Clone)]
struct ChartBreakdowns {
    step_type: String,
    difficulty: String,
    rating: String,
    streams: BreakdownSet,
    sn: BreakdownSet,
    total_streams: u32,
    total_breaks: u32,
    sn_breaks: u32,
}

#[derive(Debug, Clone)]
struct SnSnapshot {
    rating: String,
    breakdown: BreakdownSet,
    sn_breaks: u32,
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

fn compute_chart_breakdowns(
    simfile_data: &[u8],
    extension: &str,
) -> Result<Vec<ChartBreakdowns>, String> {
    let options = AnalysisOptions {
        compute_tech_counts: false,
        ..AnalysisOptions::default()
    };
    let summary = analyze(simfile_data, extension, options)
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for chart in summary.charts {
        results.push(ChartBreakdowns {
            step_type: chart.step_type_str,
            difficulty: chart.difficulty_str,
            rating: chart.rating_str,
            streams: BreakdownSet {
                detailed: chart.detailed_breakdown,
                partial: chart.partial_breakdown,
                simple: chart.simple_breakdown,
            },
            sn: BreakdownSet {
                detailed: chart.sn_detailed_breakdown,
                partial: chart.sn_partial_breakdown,
                simple: chart.sn_simple_breakdown,
            },
            total_streams: chart.total_streams,
            total_breaks: chart.stream_counts.total_breaks,
            sn_breaks: chart.stream_counts.sn_breaks,
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

    let harness_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.json.zst", file_hash));

    if !harness_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            harness_path.display()
        ));
    }

    let rssp_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.rssp.json.zst", file_hash));

    if !rssp_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            rssp_path.display()
        ));
    }

    let compressed_harness = fs::read(&harness_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let harness_json = zstd::decode_all(&compressed_harness[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let harness_charts: Vec<HarnessChart> = serde_json::from_slice(&harness_json)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let compressed_rssp = fs::read(&rssp_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let rssp_json = zstd::decode_all(&compressed_rssp[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let rssp_file: RsspGoldenFile = serde_json::from_slice(&rssp_json)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_charts = compute_chart_breakdowns(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let mut harness_map: HashMap<(String, String), Vec<HarnessChart>> = HashMap::new();
    for chart in harness_charts {
        let step_type_lower = chart.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let difficulty = rssp::normalize_difficulty_label(&chart.difficulty);
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        harness_map.entry(key).or_default().push(chart);
    }

    let mut sn_map: HashMap<(String, String), Vec<SnSnapshot>> = HashMap::new();
    for chart in rssp_file.charts {
        let step_type = chart.chart_info.step_type;
        let step_type_lower = step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let difficulty = rssp::normalize_difficulty_label(&chart.chart_info.difficulty);
        let key = (step_type_lower, difficulty.to_ascii_lowercase());
        sn_map.entry(key).or_default().push(SnSnapshot {
            rating: chart.chart_info.rating,
            breakdown: BreakdownSet {
                detailed: chart.breakdown.sn_detailed_breakdown,
                partial: chart.breakdown.sn_partial_breakdown,
                simple: chart.breakdown.sn_simple_breakdown,
            },
            sn_breaks: chart.stream_info.sn_breaks,
        });
    }

    let mut rssp_map: HashMap<(String, String), Vec<ChartBreakdowns>> = HashMap::new();
    for chart in rssp_charts {
        let step_type_lower = chart.step_type.to_ascii_lowercase();
        if step_type_lower != "dance-single" && step_type_lower != "dance-double" {
            continue;
        }
        let key = (step_type_lower, chart.difficulty.to_ascii_lowercase());
        rssp_map.entry(key).or_default().push(chart);
    }

    let mut harness_entries: Vec<_> = harness_map.into_iter().collect();
    harness_entries.sort_by(|a, b| a.0.cmp(&b.0));

    println!("File: {}", path.display());

    for ((step_type, difficulty), expected_entries) in harness_entries {
        let Some(actual_entries) = rssp_map.get(&(step_type.clone(), difficulty.clone())) else {
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

            let expected_detail = expected
                .map(|v| v.streams_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual
                .map(|v| v.streams.detailed.as_str())
                .unwrap_or("-");
            let expected_partial = expected
                .map(|v| v.streams_breakdown_level1.as_str())
                .unwrap_or("-");
            let actual_partial = actual
                .map(|v| v.streams.partial.as_str())
                .unwrap_or("-");
            let expected_simple = expected
                .map(|v| v.streams_breakdown_level2.as_str())
                .unwrap_or("-");
            let actual_simple = actual
                .map(|v| v.streams.simple.as_str())
                .unwrap_or("-");
            let expected_total_streams = expected.map(|v| v.total_stream_measures);
            let actual_total_streams = actual.map(|v| v.total_streams);
            let expected_total_breaks = expected.map(|v| v.total_break_measures);
            let actual_total_breaks = actual.map(|v| v.total_breaks);

            let matches = expected.is_some()
                && actual.is_some()
                && expected_detail == actual_detail
                && expected_partial == actual_partial
                && expected_simple == actual_simple
                && expected_total_streams == actual_total_streams
                && expected_total_breaks == actual_total_breaks;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_total_streams = expected_total_streams
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_total_streams = actual_total_streams
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let expected_total_breaks = expected_total_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_total_breaks = actual_total_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());

            println!(
                "  {} {} [{}]: detailed {} -> {} | partial {} -> {} | simple {} -> {} | total_streams {} -> {} | total_breaks {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_total_streams,
                actual_total_streams,
                expected_total_breaks,
                actual_total_breaks,
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries.iter().zip(actual_entries).all(|(e, a)| {
                e.streams_breakdown == a.streams.detailed
                    && e.streams_breakdown_level1 == a.streams.partial
                    && e.streams_breakdown_level2 == a.streams.simple
                    && e.total_stream_measures == a.total_streams
                    && e.total_break_measures == a.total_breaks
            });
        if !matches {
            let expected_detail: Vec<String> =
                expected_entries.iter().map(|e| e.streams_breakdown.clone()).collect();
            let actual_detail: Vec<String> =
                actual_entries.iter().map(|a| a.streams.detailed.clone()).collect();
            let expected_partial: Vec<String> = expected_entries
                .iter()
                .map(|e| e.streams_breakdown_level1.clone())
                .collect();
            let actual_partial: Vec<String> =
                actual_entries.iter().map(|a| a.streams.partial.clone()).collect();
            let expected_simple: Vec<String> = expected_entries
                .iter()
                .map(|e| e.streams_breakdown_level2.clone())
                .collect();
            let actual_simple: Vec<String> =
                actual_entries.iter().map(|a| a.streams.simple.clone()).collect();
            let expected_total_streams: Vec<u32> =
                expected_entries.iter().map(|e| e.total_stream_measures).collect();
            let actual_total_streams: Vec<u32> =
                actual_entries.iter().map(|a| a.total_streams).collect();
            let expected_total_breaks: Vec<u32> =
                expected_entries.iter().map(|e| e.total_break_measures).collect();
            let actual_total_breaks: Vec<u32> =
                actual_entries.iter().map(|a| a.total_breaks).collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP detailed: {:?}\nGolden detailed: {:?}\nRSSP partial: {:?}\nGolden partial: {:?}\nRSSP simple: {:?}\nGolden simple: {:?}\nRSSP total_streams: {:?}\nGolden total_streams: {:?}\nRSSP total_breaks: {:?}\nGolden total_breaks: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_detail,
                expected_detail,
                actual_partial,
                expected_partial,
                actual_simple,
                expected_simple,
                actual_total_streams,
                expected_total_streams,
                actual_total_breaks,
                expected_total_breaks
            ));
        }
    }

    let mut sn_entries: Vec<_> = sn_map.into_iter().collect();
    sn_entries.sort_by(|a, b| a.0.cmp(&b.0));

    for ((step_type, difficulty), expected_entries) in sn_entries {
        let Some(actual_entries) = rssp_map.get(&(step_type.clone(), difficulty.clone())) else {
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

            let expected_detail = expected
                .map(|v| v.breakdown.detailed.as_str())
                .unwrap_or("-");
            let actual_detail = actual
                .map(|v| v.sn.detailed.as_str())
                .unwrap_or("-");
            let expected_partial = expected
                .map(|v| v.breakdown.partial.as_str())
                .unwrap_or("-");
            let actual_partial = actual
                .map(|v| v.sn.partial.as_str())
                .unwrap_or("-");
            let expected_simple = expected
                .map(|v| v.breakdown.simple.as_str())
                .unwrap_or("-");
            let actual_simple = actual
                .map(|v| v.sn.simple.as_str())
                .unwrap_or("-");
            let expected_sn_breaks = expected.map(|v| v.sn_breaks);
            let actual_sn_breaks = actual.map(|v| v.sn_breaks);

            let matches = expected.is_some()
                && actual.is_some()
                && expected_detail == actual_detail
                && expected_partial == actual_partial
                && expected_simple == actual_simple
                && expected_sn_breaks == actual_sn_breaks;
            let status = if matches { "....ok" } else { "....MISMATCH" };

            let expected_sn_breaks = expected_sn_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());
            let actual_sn_breaks = actual_sn_breaks
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".to_string());

            println!(
                "  {} {} [{}]: sn_detailed {} -> {} | sn_partial {} -> {} | sn_simple {} -> {} | sn_breaks {} -> {} {}",
                step_type,
                difficulty,
                meter_label,
                expected_detail,
                actual_detail,
                expected_partial,
                actual_partial,
                expected_simple,
                actual_simple,
                expected_sn_breaks,
                actual_sn_breaks,
                status
            );
        }

        let matches = expected_entries.len() == actual_entries.len()
            && expected_entries.iter().zip(actual_entries).all(|(e, a)| {
                e.breakdown == a.sn && e.sn_breaks == a.sn_breaks
            });
        if !matches {
            let expected_detail: Vec<String> =
                expected_entries.iter().map(|e| e.breakdown.detailed.clone()).collect();
            let actual_detail: Vec<String> =
                actual_entries.iter().map(|a| a.sn.detailed.clone()).collect();
            let expected_partial: Vec<String> =
                expected_entries.iter().map(|e| e.breakdown.partial.clone()).collect();
            let actual_partial: Vec<String> =
                actual_entries.iter().map(|a| a.sn.partial.clone()).collect();
            let expected_simple: Vec<String> =
                expected_entries.iter().map(|e| e.breakdown.simple.clone()).collect();
            let actual_simple: Vec<String> =
                actual_entries.iter().map(|a| a.sn.simple.clone()).collect();
            let expected_sn_breaks: Vec<u32> =
                expected_entries.iter().map(|e| e.sn_breaks).collect();
            let actual_sn_breaks: Vec<u32> =
                actual_entries.iter().map(|a| a.sn_breaks).collect();

            return Err(format!(
                "\n\nMISMATCH DETECTED\nFile: {}\nChart: {} {}\nRSSP sn_detailed: {:?}\nGolden sn_detailed: {:?}\nRSSP sn_partial: {:?}\nGolden sn_partial: {:?}\nRSSP sn_simple: {:?}\nGolden sn_simple: {:?}\nRSSP sn_breaks: {:?}\nGolden sn_breaks: {:?}\n",
                path.display(),
                step_type,
                difficulty,
                actual_detail,
                expected_detail,
                actual_partial,
                expected_partial,
                actual_simple,
                expected_simple,
                actual_sn_breaks,
                expected_sn_breaks
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
                "rerun: cargo test --test breakdown_parity -- --exact {:?}",
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

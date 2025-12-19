use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::{analyze, AnalysisOptions, ChartSummary};
use rssp::patterns::PatternVariant;

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
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
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
    towers: TowersCounts,
    triangles: TrianglesCounts,
    staircases: StaircasesCounts,
    sweeps: SweepsCounts,
    candle_sweeps: CandleSweepsCounts,
    copters: CoptersCounts,
    spirals: SpiralsCounts,
    turbo_candles: TurboCandlesCounts,
    hip_breakers: HipBreakersCounts,
    doritos: DoritosCounts,
    luchis: LuchisCounts,
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

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct TowersCounts {
    total_towers: u32,
    lr_towers: u32,
    ud_towers: u32,
    corner_towers: u32,
    ld_towers: u32,
    lu_towers: u32,
    rd_towers: u32,
    ru_towers: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct TrianglesCounts {
    total_triangles: u32,
    ldl_triangles: u32,
    lul_triangles: u32,
    rdr_triangles: u32,
    rur_triangles: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct StaircasesCounts {
    total_staircases: u32,
    left_staircases: u32,
    right_staircases: u32,
    left_inv_staircases: u32,
    right_inv_staircases: u32,
    total_alt_staircases: u32,
    left_alt_staircases: u32,
    right_alt_staircases: u32,
    left_inv_alt_staircases: u32,
    right_inv_alt_staircases: u32,
    total_double_staircases: u32,
    left_double_staircases: u32,
    right_double_staircases: u32,
    left_inv_double_staircases: u32,
    right_inv_double_staircases: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct SweepsCounts {
    total_sweeps: u32,
    left_sweeps: u32,
    right_sweeps: u32,
    left_inv_sweeps: u32,
    right_inv_sweeps: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct CandleSweepsCounts {
    total_candle_sweeps: u32,
    left_candle_sweeps: u32,
    right_candle_sweeps: u32,
    left_inv_candle_sweeps: u32,
    right_inv_candle_sweeps: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct CoptersCounts {
    total_copters: u32,
    left_copters: u32,
    right_copters: u32,
    left_inv_copters: u32,
    right_inv_copters: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct SpiralsCounts {
    total_spirals: u32,
    left_spirals: u32,
    right_spirals: u32,
    left_inv_spirals: u32,
    right_inv_spirals: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct TurboCandlesCounts {
    total_turbo_candles: u32,
    left_turbo_candles: u32,
    right_turbo_candles: u32,
    left_inv_turbo_candles: u32,
    right_inv_turbo_candles: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct HipBreakersCounts {
    total_hip_breakers: u32,
    left_hip_breakers: u32,
    right_hip_breakers: u32,
    left_inv_hip_breakers: u32,
    right_inv_hip_breakers: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct DoritosCounts {
    total_doritos: u32,
    left_doritos: u32,
    right_doritos: u32,
    left_inv_doritos: u32,
    right_inv_doritos: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct LuchisCounts {
    total_luchis: u32,
    left_du_luchis: u32,
    left_ud_luchis: u32,
    right_du_luchis: u32,
    right_ud_luchis: u32,
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

fn format_json_float(value: f64) -> String {
    format!("{:.2}", value)
}

fn count_pattern(map: &HashMap<PatternVariant, u32>, variant: PatternVariant) -> u32 {
    *map.get(&variant).unwrap_or(&0)
}

fn compute_boxes(map: &HashMap<PatternVariant, u32>) -> BoxesCounts {
    let lr = count_pattern(map, PatternVariant::BoxLR);
    let ud = count_pattern(map, PatternVariant::BoxUD);
    let ld = count_pattern(map, PatternVariant::BoxCornerLD);
    let lu = count_pattern(map, PatternVariant::BoxCornerLU);
    let rd = count_pattern(map, PatternVariant::BoxCornerRD);
    let ru = count_pattern(map, PatternVariant::BoxCornerRU);
    let corner = ld + lu + rd + ru;
    let total = lr + ud + corner;

    BoxesCounts {
        total_boxes: total,
        lr_boxes: lr,
        ud_boxes: ud,
        corner_boxes: corner,
        ld_boxes: ld,
        lu_boxes: lu,
        rd_boxes: rd,
        ru_boxes: ru,
    }
}

fn compute_towers(map: &HashMap<PatternVariant, u32>) -> TowersCounts {
    let lr = count_pattern(map, PatternVariant::TowerLR);
    let ud = count_pattern(map, PatternVariant::TowerUD);
    let ld = count_pattern(map, PatternVariant::TowerCornerLD);
    let lu = count_pattern(map, PatternVariant::TowerCornerLU);
    let rd = count_pattern(map, PatternVariant::TowerCornerRD);
    let ru = count_pattern(map, PatternVariant::TowerCornerRU);
    let corner = ld + lu + rd + ru;
    let total = lr + ud + corner;

    TowersCounts {
        total_towers: total,
        lr_towers: lr,
        ud_towers: ud,
        corner_towers: corner,
        ld_towers: ld,
        lu_towers: lu,
        rd_towers: rd,
        ru_towers: ru,
    }
}

fn compute_triangles(map: &HashMap<PatternVariant, u32>) -> TrianglesCounts {
    let ldl = count_pattern(map, PatternVariant::TriangleLDL);
    let lul = count_pattern(map, PatternVariant::TriangleLUL);
    let rdr = count_pattern(map, PatternVariant::TriangleRDR);
    let rur = count_pattern(map, PatternVariant::TriangleRUR);
    let total = ldl + lul + rdr + rur;

    TrianglesCounts {
        total_triangles: total,
        ldl_triangles: ldl,
        lul_triangles: lul,
        rdr_triangles: rdr,
        rur_triangles: rur,
    }
}

fn compute_staircases(map: &HashMap<PatternVariant, u32>) -> StaircasesCounts {
    let left = count_pattern(map, PatternVariant::StaircaseLeft);
    let right = count_pattern(map, PatternVariant::StaircaseRight);
    let left_inv = count_pattern(map, PatternVariant::StaircaseInvLeft);
    let right_inv = count_pattern(map, PatternVariant::StaircaseInvRight);
    let total = left + right + left_inv + right_inv;

    let alt_left = count_pattern(map, PatternVariant::AltStaircasesLeft);
    let alt_right = count_pattern(map, PatternVariant::AltStaircasesRight);
    let alt_left_inv = count_pattern(map, PatternVariant::AltStaircasesInvLeft);
    let alt_right_inv = count_pattern(map, PatternVariant::AltStaircasesInvRight);
    let total_alt = alt_left + alt_right + alt_left_inv + alt_right_inv;

    let double_left = count_pattern(map, PatternVariant::DStaircaseLeft);
    let double_right = count_pattern(map, PatternVariant::DStaircaseRight);
    let double_left_inv = count_pattern(map, PatternVariant::DStaircaseInvLeft);
    let double_right_inv = count_pattern(map, PatternVariant::DStaircaseInvRight);
    let total_double = double_left + double_right + double_left_inv + double_right_inv;

    StaircasesCounts {
        total_staircases: total,
        left_staircases: left,
        right_staircases: right,
        left_inv_staircases: left_inv,
        right_inv_staircases: right_inv,
        total_alt_staircases: total_alt,
        left_alt_staircases: alt_left,
        right_alt_staircases: alt_right,
        left_inv_alt_staircases: alt_left_inv,
        right_inv_alt_staircases: alt_right_inv,
        total_double_staircases: total_double,
        left_double_staircases: double_left,
        right_double_staircases: double_right,
        left_inv_double_staircases: double_left_inv,
        right_inv_double_staircases: double_right_inv,
    }
}

fn compute_sweeps(map: &HashMap<PatternVariant, u32>) -> SweepsCounts {
    let left = count_pattern(map, PatternVariant::SweepLeft);
    let right = count_pattern(map, PatternVariant::SweepRight);
    let left_inv = count_pattern(map, PatternVariant::SweepInvLeft);
    let right_inv = count_pattern(map, PatternVariant::SweepInvRight);
    let total = left + right + left_inv + right_inv;

    SweepsCounts {
        total_sweeps: total,
        left_sweeps: left,
        right_sweeps: right,
        left_inv_sweeps: left_inv,
        right_inv_sweeps: right_inv,
    }
}

fn compute_candle_sweeps(map: &HashMap<PatternVariant, u32>) -> CandleSweepsCounts {
    let left = count_pattern(map, PatternVariant::SweepCandleLeft);
    let right = count_pattern(map, PatternVariant::SweepCandleRight);
    let left_inv = count_pattern(map, PatternVariant::SweepCandleInvLeft);
    let right_inv = count_pattern(map, PatternVariant::SweepCandleInvRight);
    let total = left + right + left_inv + right_inv;

    CandleSweepsCounts {
        total_candle_sweeps: total,
        left_candle_sweeps: left,
        right_candle_sweeps: right,
        left_inv_candle_sweeps: left_inv,
        right_inv_candle_sweeps: right_inv,
    }
}

fn compute_copters(map: &HashMap<PatternVariant, u32>) -> CoptersCounts {
    let left = count_pattern(map, PatternVariant::CopterLeft);
    let right = count_pattern(map, PatternVariant::CopterRight);
    let left_inv = count_pattern(map, PatternVariant::CopterInvLeft);
    let right_inv = count_pattern(map, PatternVariant::CopterInvRight);
    let total = left + right + left_inv + right_inv;

    CoptersCounts {
        total_copters: total,
        left_copters: left,
        right_copters: right,
        left_inv_copters: left_inv,
        right_inv_copters: right_inv,
    }
}

fn compute_spirals(map: &HashMap<PatternVariant, u32>) -> SpiralsCounts {
    let left = count_pattern(map, PatternVariant::SpiralLeft);
    let right = count_pattern(map, PatternVariant::SpiralRight);
    let left_inv = count_pattern(map, PatternVariant::SpiralInvLeft);
    let right_inv = count_pattern(map, PatternVariant::SpiralInvRight);
    let total = left + right + left_inv + right_inv;

    SpiralsCounts {
        total_spirals: total,
        left_spirals: left,
        right_spirals: right,
        left_inv_spirals: left_inv,
        right_inv_spirals: right_inv,
    }
}

fn compute_turbo_candles(map: &HashMap<PatternVariant, u32>) -> TurboCandlesCounts {
    let left = count_pattern(map, PatternVariant::TurboCandleLeft);
    let right = count_pattern(map, PatternVariant::TurboCandleRight);
    let left_inv = count_pattern(map, PatternVariant::TurboCandleInvLeft);
    let right_inv = count_pattern(map, PatternVariant::TurboCandleInvRight);
    let total = left + right + left_inv + right_inv;

    TurboCandlesCounts {
        total_turbo_candles: total,
        left_turbo_candles: left,
        right_turbo_candles: right,
        left_inv_turbo_candles: left_inv,
        right_inv_turbo_candles: right_inv,
    }
}

fn compute_hip_breakers(map: &HashMap<PatternVariant, u32>) -> HipBreakersCounts {
    let left = count_pattern(map, PatternVariant::HipBreakerLeft);
    let right = count_pattern(map, PatternVariant::HipBreakerRight);
    let left_inv = count_pattern(map, PatternVariant::HipBreakerInvLeft);
    let right_inv = count_pattern(map, PatternVariant::HipBreakerInvRight);
    let total = left + right + left_inv + right_inv;

    HipBreakersCounts {
        total_hip_breakers: total,
        left_hip_breakers: left,
        right_hip_breakers: right,
        left_inv_hip_breakers: left_inv,
        right_inv_hip_breakers: right_inv,
    }
}

fn compute_doritos(map: &HashMap<PatternVariant, u32>) -> DoritosCounts {
    let left = count_pattern(map, PatternVariant::DoritoLeft);
    let right = count_pattern(map, PatternVariant::DoritoRight);
    let left_inv = count_pattern(map, PatternVariant::DoritoInvLeft);
    let right_inv = count_pattern(map, PatternVariant::DoritoInvRight);
    let total = left + right + left_inv + right_inv;

    DoritosCounts {
        total_doritos: total,
        left_doritos: left,
        right_doritos: right,
        left_inv_doritos: left_inv,
        right_inv_doritos: right_inv,
    }
}

fn compute_luchis(map: &HashMap<PatternVariant, u32>) -> LuchisCounts {
    let left_du = count_pattern(map, PatternVariant::LuchiLeftDU);
    let left_ud = count_pattern(map, PatternVariant::LuchiLeftUD);
    let right_du = count_pattern(map, PatternVariant::LuchiRightDU);
    let right_ud = count_pattern(map, PatternVariant::LuchiRightUD);
    let total = left_du + left_ud + right_du + right_ud;

    LuchisCounts {
        total_luchis: total,
        left_du_luchis: left_du,
        left_ud_luchis: left_ud,
        right_du_luchis: right_du,
        right_ud_luchis: right_ud,
    }
}

fn chart_values_from_summary(chart: &ChartSummary) -> ChartUniqueValues {
    let patterns = &chart.detected_patterns;
    let left_foot_candles = count_pattern(patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count_pattern(patterns, PatternVariant::CandleRight);

    ChartUniqueValues {
        matrix_rating: format_json_float(chart.matrix_rating),
        breakdown: Breakdown {
            detailed_breakdown: chart.detailed.clone(),
            partial_breakdown: chart.partial.clone(),
            simple_breakdown: chart.simple.clone(),
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
            boxes: compute_boxes(patterns),
            anchors: AnchorsCounts {
                total_anchors: chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right,
                left_anchors: chart.anchor_left,
                down_anchors: chart.anchor_down,
                up_anchors: chart.anchor_up,
                right_anchors: chart.anchor_right,
            },
            towers: compute_towers(patterns),
            triangles: compute_triangles(patterns),
            staircases: compute_staircases(patterns),
            sweeps: compute_sweeps(patterns),
            candle_sweeps: compute_candle_sweeps(patterns),
            copters: compute_copters(patterns),
            spirals: compute_spirals(patterns),
            turbo_candles: compute_turbo_candles(patterns),
            hip_breakers: compute_hip_breakers(patterns),
            doritos: compute_doritos(patterns),
            luchis: compute_luchis(patterns),
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
    let compressed_bytes = fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

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

    let compressed_golden = fs::read(&golden_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let golden_file: GoldenFile = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let rssp_charts = compute_chart_values(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let mut golden_map: HashMap<(String, String), Vec<ChartSnapshot>> = HashMap::new();
    for golden in golden_file.charts {
        let step_type = golden.chart_info.step_type.clone();
        let difficulty = golden.chart_info.difficulty.clone();
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

            let expected_matrix = expected_values.map(|v| v.matrix_rating.as_str()).unwrap_or("-");
            let actual_matrix = actual_values.map(|v| v.matrix_rating.as_str()).unwrap_or("-");
            let expected_detail = expected_values
                .map(|v| v.breakdown.detailed_breakdown.as_str())
                .unwrap_or("-");
            let actual_detail = actual_values
                .map(|v| v.breakdown.detailed_breakdown.as_str())
                .unwrap_or("-");
            let expected_partial = expected_values
                .map(|v| v.breakdown.partial_breakdown.as_str())
                .unwrap_or("-");
            let actual_partial = actual_values
                .map(|v| v.breakdown.partial_breakdown.as_str())
                .unwrap_or("-");
            let expected_simple = expected_values
                .map(|v| v.breakdown.simple_breakdown.as_str())
                .unwrap_or("-");
            let actual_simple = actual_values
                .map(|v| v.breakdown.simple_breakdown.as_str())
                .unwrap_or("-");

            println!(
                "  {} {} [{}]: matrix_rating {} -> {} | detailed {} -> {} | partial {} -> {} | simple {} -> {} {}",
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

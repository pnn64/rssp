use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Duration;

use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::patterns::{CustomPatternSummary, PatternVariant};
use crate::stats::{ArrowStats, StreamCounts};
use crate::step_parity::TechCounts;

// Make the struct and its fields public
#[derive(Debug)]
pub struct ChartSummary {
    pub step_type_str:     String,
    pub step_artist_str:   Vec<String>,
    pub difficulty_str:    String,
    pub rating_str:        String,
    pub matrix_rating:     f64,
    pub tech_notation_str: String,
    pub tier_bpm:          f64,
    pub stats:             ArrowStats,
    pub stream_counts:     StreamCounts,
    pub total_measures:    usize,
    pub total_streams:     u32,
    pub detailed:          String,
    pub partial:           String,
    pub simple:            String,
    pub max_nps:           f64,
    pub median_nps:        f64,
    pub detected_patterns: HashMap<PatternVariant, u32>,
    pub anchor_left:       u32,
    pub anchor_down:       u32,
    pub anchor_up:         u32,
    pub anchor_right:      u32,
    pub facing_left:       u32,
    pub facing_right:      u32,
    pub mono_total:        u32,
    pub mono_percent:      f64,
    pub candle_total:      u32,
    pub candle_percent:    f64,
    pub tech_counts:       TechCounts,
    pub custom_patterns:   Vec<CustomPatternSummary>,
    pub short_hash:        String,
    pub bpm_neutral_hash:  String,
    pub elapsed:           Duration,
    pub measure_densities: Vec<usize>,
    pub measure_nps_vec:   Vec<f64>,
    pub minimized_note_data: Vec<u8>,
    pub chart_stops:       Option<String>,
    pub chart_speeds:      Option<String>,
    pub chart_scrolls:     Option<String>,
    pub chart_bpms:        Option<String>,
    pub chart_delays:      Option<String>,
    pub chart_warps:       Option<String>,
}

// Make the struct and its fields public
#[derive(Debug)] // Add Debug for easier use in the engine
pub struct SimfileSummary {
    pub title_str:            String,
    pub subtitle_str:         String,
    pub artist_str:           String,
    pub titletranslit_str:    String,
    pub subtitletranslit_str: String,
    pub artisttranslit_str:   String,
    pub offset:               f64,
    pub normalized_bpms:      String,
    pub normalized_stops:     String,
    pub normalized_delays:    String,
    pub normalized_speeds:    String,
    pub normalized_scrolls:   String,
    pub banner_path:          String,
    pub background_path:      String,
    pub music_path:           String,
    pub display_bpm_str:      String,
    pub sample_start:         f64,
    pub sample_length:        f64,
    pub min_bpm:              f64,
    pub max_bpm:              f64,
    pub normalized_warps:     String,
    pub median_bpm:           f64,
    pub average_bpm:          f64,
    pub total_length:         i32,
    pub charts:               Vec<ChartSummary>,
    pub total_elapsed:        Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Full,
    Pretty,
    JSON,
    CSV,
}

pub fn print_reports(simfile: &SimfileSummary, mode: OutputMode) {
    match mode {
        OutputMode::Full   => print_full_all(simfile),
        OutputMode::Pretty => print_pretty_all(simfile),
        OutputMode::JSON   => print_json_all(simfile),
        OutputMode::CSV    => print_csv_all(simfile),
    }
}

fn format_duration(seconds: i32) -> String {
    let minutes = seconds / 60;
    let seconds = seconds % 60;
    format!("{}m {:02}s", minutes, seconds)
}

fn count(map: &HashMap<PatternVariant, u32>, variant: PatternVariant) -> u32 {
    *map.get(&variant).unwrap_or(&0)
}

fn print_pretty_all(simfile: &SimfileSummary) {
    println!("--- Song Details ---");
    println!("Title: {}{} by {}",
        simfile.title_str,
        if simfile.subtitle_str.is_empty() {
            String::new()
        } else {
            format!(" {}", simfile.subtitle_str)
        },
        simfile.artist_str
    );
    println!("Length: {}", format_duration(simfile.total_length));
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        println!("BPM: {:.0}", simfile.min_bpm);
    } else {
        println!("BPM: {:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm);
        println!("Median BPM: {:.0}", simfile.median_bpm);
        println!("Average BPM: {:.0}", simfile.average_bpm);
    }

    for chart in &simfile.charts {
        print_pretty_chart(chart);
    }
}

fn print_pretty_chart(chart: &ChartSummary) {
    let header = format!("{} {} : {}", chart.difficulty_str, chart.rating_str, chart.step_artist_str.join(", "));
    println!("\n{}", header);
    println!("{}", "-".repeat(header.len()));

    if (chart.median_nps - chart.max_nps).abs() < f64::EPSILON {
        println!("NPS: {:.2} Median/Peak", chart.median_nps);
    } else {
        println!("NPS: {:.2} Median, {:.2} Peak", chart.median_nps, chart.max_nps);
    }

    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let adjusted_stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else { 0.0 };

    let stream_percent = if total_measures > 0 {
        (total_stream as f64 / total_measures as f64) * 100.0
    } else { 0.0 };

    println!("Total Stream: {} ({:.2}%/{:.2}% Adj.)", total_stream, stream_percent, adjusted_stream_percent);
    println!("Total Break: {} ({:.2}%)", total_break, 100.0 - adjusted_stream_percent);

    println!("\n--- Chart Info ---");
    println!("Steps: {} ({} arrows)", chart.stats.total_steps, chart.stats.total_arrows);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Hands: {}", chart.stats.hands);
    println!("Holds: {}", chart.stats.holds);
    println!("Rolls: {}", chart.stats.rolls);
    println!("Mines: {}", chart.stats.mines);

    if chart.stats.lifts > 0 {
        println!("Lifts: {}", chart.stats.lifts);
    }

    if chart.stats.fakes > 0 {
        println!("Fakes: {}", chart.stats.fakes);
    }

    println!("\n--- Pattern Analysis ---");
    let candle_left = chart.detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = chart.detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    println!("Candles: {} ({} left, {} right)",
        candle_left + candle_right, candle_left, candle_right);
    println!("Candle%: {:.2}%", chart.candle_percent);
    println!("Mono: {} ({} left-facing, {} right-facing)", chart.mono_total, chart.facing_left, chart.facing_right);
    println!("Mono%: {:.2}%", chart.mono_percent);

    let box_lr = chart.detected_patterns.get(&PatternVariant::BoxLR).unwrap_or(&0);
    let box_ud = chart.detected_patterns.get(&PatternVariant::BoxUD).unwrap_or(&0);
    let box_corners =
        chart.detected_patterns.get(&PatternVariant::BoxCornerLD).unwrap_or(&0) +
        chart.detected_patterns.get(&PatternVariant::BoxCornerLU).unwrap_or(&0) +
        chart.detected_patterns.get(&PatternVariant::BoxCornerRD).unwrap_or(&0) +
        chart.detected_patterns.get(&PatternVariant::BoxCornerRU).unwrap_or(&0);
    println!("Boxes: {} ({} LRLR, {} UDUD, {} corner)",
        box_lr + box_ud + box_corners, box_lr, box_ud, box_corners);

    let anchor_total = chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    println!("Anchors: {} ({} left, {} down, {} up, {} right)",
        anchor_total, chart.anchor_left, chart.anchor_down, chart.anchor_up, chart.anchor_right);

    println!("\n--- Step Parity Analysis ---");
    println!("Crossovers: {}", chart.tech_counts.crossovers);
    println!("Footswitches: {} ({} up, {} down)", chart.tech_counts.footswitches, chart.tech_counts.up_footswitches, chart.tech_counts.down_footswitches);
    println!("Sideswitches: {}", chart.tech_counts.sideswitches);
    println!("Jacks: {}", chart.tech_counts.jacks);
    println!("Brackets: {}", chart.tech_counts.brackets);
    println!("Doublesteps: {}", chart.tech_counts.doublesteps);

    if !chart.custom_patterns.is_empty() {
        println!("\n--- Custom Patterns ---");
        for cp in &chart.custom_patterns {
            println!("{}: {}", cp.pattern, cp.count);
        }
    }

    if !chart.detailed.is_empty() {
        println!("\n--- Detailed Breakdown ---");
        println!("{}", chart.detailed);
        println!("--- Partially Simplified ---");
        println!("{}", chart.partial);
        println!("--- Simplified Breakdown ---");
        println!("{}", chart.simple);
    }
}

fn print_full_all(simfile: &SimfileSummary) {
    println!("--- Song Details ---");
    println!("Title: {}", simfile.title_str);
    if !simfile.subtitle_str.is_empty() {
        println!("Subtitle: {}", simfile.subtitle_str);
    }
    println!("Artist: {}", simfile.artist_str);
    if !simfile.titletranslit_str.is_empty() {
        println!("Title trans: {}", simfile.titletranslit_str);
    }
    if !simfile.subtitletranslit_str.is_empty() {
        println!("Subtitle trans: {}", simfile.subtitletranslit_str);
    }
    if !simfile.artisttranslit_str.is_empty() {
        println!("Artist trans: {}", simfile.artisttranslit_str);
    }
    
    println!("Length: {}", format_duration(simfile.total_length));
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        println!("BPM: {:.0}", simfile.min_bpm);
    } else {
        println!("BPM: {:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm);
    }
    println!("Average BPM: {:.2}", simfile.average_bpm);
    println!("Median BPM: {:.2}", simfile.median_bpm);
    println!("BPM Data: {}", simfile.normalized_bpms);
    println!("Offset: {:.3}", simfile.offset);

    for chart in &simfile.charts {
        print_full_chart(chart);
    }
    println!("\nElapsed Time: {:?}", simfile.total_elapsed);
}

fn print_full_chart(chart: &ChartSummary) {
    let header = format!("{} {} : {}", chart.difficulty_str, chart.rating_str, chart.step_artist_str.join(", "));
    println!("\n{}", header);
    println!("{}", "-".repeat(header.len()));

    println!("Step Type: {}", chart.step_type_str);
    println!("Matrix Rating: {:.4}", chart.matrix_rating);
    println!("Tier BPM: {}", chart.tier_bpm);
    if !chart.tech_notation_str.is_empty() {
        println!("Tech Notations: {}", chart.tech_notation_str);
    }
    println!("SHA1 Hash: {}", chart.short_hash);
    println!("BPM Neutral SHA1 Hash: {}\n", chart.bpm_neutral_hash);

    if (chart.median_nps - chart.max_nps).abs() < f64::EPSILON {
        println!("NPS: {:.2} Median/Peak", chart.median_nps);
    } else {
        println!("NPS: {:.2} Median, {:.2} Peak", chart.median_nps, chart.max_nps);
    }
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let adjusted_stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else { 0.0 };

    let stream_percent = if total_measures > 0 {
        (total_stream as f64 / total_measures as f64) * 100.0
    } else { 0.0 };
    println!("Total Stream: {} ({:.2}%/{:.2}% Adj.)", total_stream, stream_percent, adjusted_stream_percent);
    println!("    16th_streams: {}", chart.stream_counts.run16_streams);
    println!("    20th_streams: {}", chart.stream_counts.run20_streams);
    println!("    24th_streams: {}", chart.stream_counts.run24_streams);
    println!("    32nd_streams: {}", chart.stream_counts.run32_streams);
    println!("Total Break: {} ({:.2}%)", total_break, 100.0 - adjusted_stream_percent);

    println!("\n--- Chart Info ---");
    println!("Steps: {} ({} arrows) [{} left, {} down, {} up, {} right]", chart.stats.total_steps, chart.stats.total_arrows,chart.stats.left, chart.stats.down, chart.stats.up, chart.stats.right);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Hands: {}", chart.stats.hands);
    println!("Holds: {}", chart.stats.holds);
    println!("Rolls: {}", chart.stats.rolls);
    println!("Mines: {}", chart.stats.mines);
    println!("Lifts: {}", chart.stats.lifts);
    println!("Fakes: {}", chart.stats.fakes);

    println!("\n--- Pattern Analysis ---");
    let candle_left = chart.detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = chart.detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    println!("Candles: {} ({} left, {} right)",
        candle_left + candle_right, candle_left, candle_right);
    println!("Candle%: {:.2}%", chart.candle_percent);
    println!("Mono: {} ({} left-facing, {} right-facing)", chart.mono_total, chart.facing_left, chart.facing_right);
    println!("Mono%: {:.2}%", chart.mono_percent);

    let box_lr = chart.detected_patterns.get(&PatternVariant::BoxLR).unwrap_or(&0);
    let box_ud = chart.detected_patterns.get(&PatternVariant::BoxUD).unwrap_or(&0);
    let box_ld = chart.detected_patterns.get(&PatternVariant::BoxCornerLD).unwrap_or(&0);
    let box_lu = chart.detected_patterns.get(&PatternVariant::BoxCornerLU).unwrap_or(&0);
    let box_rd = chart.detected_patterns.get(&PatternVariant::BoxCornerRD).unwrap_or(&0);
    let box_ru = chart.detected_patterns.get(&PatternVariant::BoxCornerRU).unwrap_or(&0);
    let box_corners = box_lr + box_ud + box_ld + box_lu + box_rd + box_ru;
    println!("Boxes: {} ({} LRLR, {} UDUD, {} LDLD, {} LULU, {} RDRD, {} RURU)",
        box_lr + box_ud + box_corners, box_lr, box_ud, box_ld, box_lu, box_rd, box_ru);

    let anchor_total = chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    println!("Anchors: {} ({} left, {} down, {} up, {} right)",
        anchor_total, chart.anchor_left, chart.anchor_down, chart.anchor_up, chart.anchor_right);

    println!("\n--- Step Parity Analysis ---");
    println!("Crossovers: {}", chart.tech_counts.crossovers);
    println!("Footswitches: {} ({} up, {} down)", chart.tech_counts.footswitches, chart.tech_counts.up_footswitches, chart.tech_counts.down_footswitches);
    println!("Sideswitches: {}", chart.tech_counts.sideswitches);
    println!("Jacks: {}", chart.tech_counts.jacks);
    println!("Brackets: {}", chart.tech_counts.brackets);
    println!("Doublesteps: {}", chart.tech_counts.doublesteps);

    if !chart.detailed.is_empty() {
        println!("\n--- Detailed Breakdown ---");
        println!("{}", chart.detailed);
        println!("--- Partially Simplified ---");
        println!("{}", chart.partial);
        println!("--- Simplified Breakdown ---");
        println!("{}", chart.simple);
    }

    println!("\n--- Other Patterns ---");
    let lr_towers = count(&chart.detected_patterns, PatternVariant::TowerLR);
    let ud_towers = count(&chart.detected_patterns, PatternVariant::TowerUD);
    let ld_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLD);
    let lu_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLU);
    let rd_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRD);
    let ru_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRU);
    let corner_towers = ld_towers + lu_towers + rd_towers + ru_towers;
    let total_towers = lr_towers + ud_towers + corner_towers;
    println!("Total Towers: {} ({} LR, {} UD, {} LD, {} LU, {} RD, {} RU)", total_towers, lr_towers, ud_towers, ld_towers, lu_towers, rd_towers, ru_towers);

    // Triangles
    let ldl_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLDL);
    let lul_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLUL);
    let rdr_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRDR);
    let rur_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRUR);
    let total_triangles = ldl_triangles + lul_triangles + rdr_triangles + rur_triangles;
    println!("Total Triangles: {} ({} LDL, {} LUL, {} RDR, {} RUR)",
        total_triangles, ldl_triangles, lul_triangles, rdr_triangles, rur_triangles);

    // Staircases
    let left_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseLeft);
    let right_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseRight);
    let left_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvLeft);
    let right_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvRight);
    let total_staircases = left_staircases + right_staircases + left_inv_staircases + right_inv_staircases;
    println!("Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_staircases, left_staircases, right_staircases, left_inv_staircases, right_inv_staircases);

    // Alternate Staircases
    let alt_left = count(&chart.detected_patterns, PatternVariant::AltStaircasesLeft);
    let alt_right = count(&chart.detected_patterns, PatternVariant::AltStaircasesRight);
    let alt_left_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvLeft);
    let alt_right_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvRight);
    let total_alt = alt_left + alt_right + alt_left_inv + alt_right_inv;
    println!("Alt Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_alt, alt_left, alt_right, alt_left_inv, alt_right_inv);

    // Double Staircases
    let d_left = count(&chart.detected_patterns, PatternVariant::DStaircaseLeft);
    let d_right = count(&chart.detected_patterns, PatternVariant::DStaircaseRight);
    let d_left_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvLeft);
    let d_right_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvRight);
    let total_double = d_left + d_right + d_left_inv + d_right_inv;
    println!("Double Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_double, d_left, d_right, d_left_inv, d_right_inv);

    // Sweeps
    let left_sweeps = count(&chart.detected_patterns, PatternVariant::SweepLeft);
    let right_sweeps = count(&chart.detected_patterns, PatternVariant::SweepRight);
    let left_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvLeft);
    let right_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvRight);
    let total_sweeps = left_sweeps + right_sweeps + left_inv_sweeps + right_inv_sweeps;
    println!("Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_sweeps, left_sweeps, right_sweeps, left_inv_sweeps, right_inv_sweeps);

    // Candle Sweeps
    let left_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleLeft);
    let right_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleRight);
    let left_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvLeft);
    let right_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvRight);
    let total_candle_sweeps = left_candle_sweeps + right_candle_sweeps + left_inv_candle_sweeps + right_inv_candle_sweeps;
    println!("Candle Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_candle_sweeps, left_candle_sweeps, right_candle_sweeps, left_inv_candle_sweeps, right_inv_candle_sweeps);

    // Copters
    let left_copters = count(&chart.detected_patterns, PatternVariant::CopterLeft);
    let right_copters = count(&chart.detected_patterns, PatternVariant::CopterRight);
    let left_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvLeft);
    let right_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvRight);
    let total_copters = left_copters + right_copters + left_inv_copters + right_inv_copters;
    println!("Copters: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_copters, left_copters, right_copters, left_inv_copters, right_inv_copters);

    // Spirals
    let left_spirals = count(&chart.detected_patterns, PatternVariant::SpiralLeft);
    let right_spirals = count(&chart.detected_patterns, PatternVariant::SpiralRight);
    let left_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvLeft);
    let right_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvRight);
    let total_spirals = left_spirals + right_spirals + left_inv_spirals + right_inv_spirals;
    println!("Spirals: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_spirals, left_spirals, right_spirals, left_inv_spirals, right_inv_spirals);

    // Turbo Candles
    let left_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleLeft);
    let right_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleRight);
    let left_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvLeft);
    let right_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvRight);
    let total_turbo_candles = left_turbo_candles + right_turbo_candles + left_inv_turbo_candles + right_inv_turbo_candles;
    println!("Turbo Candles: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_turbo_candles, left_turbo_candles, right_turbo_candles, left_inv_turbo_candles, right_inv_turbo_candles);

    // Hip Breakers
    let left_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerLeft);
    let right_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerRight);
    let left_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvLeft);
    let right_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvRight);
    let total_hip_breakers = left_hip_breakers + right_hip_breakers + left_inv_hip_breakers + right_inv_hip_breakers;
    println!("Hip Breakers: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_hip_breakers, left_hip_breakers, right_hip_breakers, left_inv_hip_breakers, right_inv_hip_breakers);

    // Doritos
    let left_doritos = count(&chart.detected_patterns, PatternVariant::DoritoLeft);
    let right_doritos = count(&chart.detected_patterns, PatternVariant::DoritoRight);
    let left_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvLeft);
    let right_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvRight);
    let total_doritos = left_doritos + right_doritos + left_inv_doritos + right_inv_doritos;
    println!("Doritos: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_doritos, left_doritos, right_doritos, left_inv_doritos, right_inv_doritos);

    // Luchis
    let left_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftDU);
    let left_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftUD);
    let right_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightDU);
    let right_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightUD);
    let total_luchis = left_du_luchis + left_ud_luchis + right_du_luchis + right_ud_luchis;
    println!("Luchis: {} ({} Left DU, {} Left UD, {} Right DU, {} Right UD)",
        total_luchis, left_du_luchis, left_ud_luchis, right_du_luchis, right_ud_luchis);

    if !chart.custom_patterns.is_empty() {
        println!("\n--- Custom Patterns ---");
        for cp in &chart.custom_patterns {
            println!("{}: {}", cp.pattern, cp.count);
        }
    }
}

fn json_chart_info(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "step_type": chart.step_type_str,
        "difficulty": chart.difficulty_str,
        "tier_bpm": chart.tier_bpm,
        "rating": chart.rating_str,
        "matrix_rating": chart.matrix_rating,
        "step_artists": chart.step_artist_str,
        "tech_notation": chart.tech_notation_str,
        "sha1": chart.short_hash,
        "bpm_neutral_sha1": chart.bpm_neutral_hash,
    })
}

fn json_arrow_stats(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "total_arrows": chart.stats.total_arrows,
        "left_arrows": chart.stats.left,
        "down_arrows": chart.stats.down,
        "up_arrows": chart.stats.up,
        "right_arrows": chart.stats.right,
        "total_steps": chart.stats.total_steps,
        "jumps": chart.stats.jumps,
        "hands": chart.stats.hands,
        "holds": chart.stats.holds,
        "rolls": chart.stats.rolls,
        "mines": chart.stats.mines,
        "lifts": chart.stats.lifts,
        "fakes": chart.stats.fakes,
    })
}

fn json_stream_info(chart: &ChartSummary) -> JsonValue {
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let adj_stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else {
        0.0
    };

    let stream_percent = if total_measures > 0 {
        (total_stream as f64 / total_measures as f64) * 100.0
    } else {
        0.0
    };

    serde_json::json!({
        "total_streams": total_stream,
        "16th_streams": chart.stream_counts.run16_streams,
        "20th_streams": chart.stream_counts.run20_streams,
        "24th_streams": chart.stream_counts.run24_streams,
        "32nd_streams": chart.stream_counts.run32_streams,
        "total_breaks": total_break,
        "stream_percent": stream_percent,
        "adj_stream_percent": adj_stream_percent,
        "break_percent": 100.0 - adj_stream_percent,
    })
}

fn json_nps(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "max_nps": chart.max_nps,
        "median_nps": chart.median_nps,
    })
}

fn json_breakdown(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "detailed_breakdown": chart.detailed,
        "partial_breakdown": chart.partial,
        "simple_breakdown": chart.simple,
    })
}

fn json_mono_candle_stats(chart: &ChartSummary) -> JsonValue {
    let left_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleRight);
    let total_candles = left_foot_candles + right_foot_candles;

    serde_json::json!({
        "total_candles": total_candles,
        "left_foot_candles": left_foot_candles,
        "right_foot_candles": right_foot_candles,
        "candles_percent": chart.candle_percent,
        "total_mono": chart.mono_total,
        "left_face_mono": chart.facing_left,
        "right_face_mono": chart.facing_right,
        "mono_percent": chart.mono_percent,
    })
}

fn json_pattern_counts(chart: &ChartSummary) -> JsonValue {
    let mut obj = JsonMap::new();

    // Boxes
    let lr_boxes = count(&chart.detected_patterns, PatternVariant::BoxLR);
    let ud_boxes = count(&chart.detected_patterns, PatternVariant::BoxUD);
    let ld_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLD);
    let lu_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLU);
    let rd_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRD);
    let ru_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRU);
    let corner_boxes = ld_boxes + lu_boxes + rd_boxes + ru_boxes;
    let total_boxes = lr_boxes + ud_boxes + corner_boxes;
    obj.insert(
        "boxes".to_string(),
        serde_json::json!({
            "total_boxes": total_boxes,
            "lr_boxes": lr_boxes,
            "ud_boxes": ud_boxes,
            "corner_boxes": corner_boxes,
            "ld_boxes": ld_boxes,
            "lu_boxes": lu_boxes,
            "rd_boxes": rd_boxes,
            "ru_boxes": ru_boxes,
        }),
    );

    // Anchors
    let total_anchors =
        chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    obj.insert(
        "anchors".to_string(),
        serde_json::json!({
            "total_anchors": total_anchors,
            "left_anchors": chart.anchor_left,
            "down_anchors": chart.anchor_down,
            "up_anchors": chart.anchor_up,
            "right_anchors": chart.anchor_right,
        }),
    );

    // Towers
    let lr_towers = count(&chart.detected_patterns, PatternVariant::TowerLR);
    let ud_towers = count(&chart.detected_patterns, PatternVariant::TowerUD);
    let ld_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLD);
    let lu_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLU);
    let rd_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRD);
    let ru_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRU);
    let corner_towers = ld_towers + lu_towers + rd_towers + ru_towers;
    let total_towers = lr_towers + ud_towers + corner_towers;
    obj.insert(
        "towers".to_string(),
        serde_json::json!({
            "total_towers": total_towers,
            "lr_towers": lr_towers,
            "ud_towers": ud_towers,
            "corner_towers": corner_towers,
            "ld_towers": ld_towers,
            "lu_towers": lu_towers,
            "rd_towers": rd_towers,
            "ru_towers": ru_towers,
        }),
    );

    // Triangles
    let ldl_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLDL);
    let lul_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLUL);
    let rdr_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRDR);
    let rur_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRUR);
    let total_triangles =
        ldl_triangles + lul_triangles + rdr_triangles + rur_triangles;
    obj.insert(
        "triangles".to_string(),
        serde_json::json!({
            "total_triangles": total_triangles,
            "ldl_triangles": ldl_triangles,
            "lul_triangles": lul_triangles,
            "rdr_triangles": rdr_triangles,
            "rur_triangles": rur_triangles,
        }),
    );

    // Staircases
    let left_staircases =
        count(&chart.detected_patterns, PatternVariant::StaircaseLeft);
    let right_staircases =
        count(&chart.detected_patterns, PatternVariant::StaircaseRight);
    let left_inv_staircases =
        count(&chart.detected_patterns, PatternVariant::StaircaseInvLeft);
    let right_inv_staircases =
        count(&chart.detected_patterns, PatternVariant::StaircaseInvRight);
    let total_staircases = left_staircases
        + right_staircases
        + left_inv_staircases
        + right_inv_staircases;
    let alt_left =
        count(&chart.detected_patterns, PatternVariant::AltStaircasesLeft);
    let alt_right =
        count(&chart.detected_patterns, PatternVariant::AltStaircasesRight);
    let alt_left_inv =
        count(&chart.detected_patterns, PatternVariant::AltStaircasesInvLeft);
    let alt_right_inv =
        count(&chart.detected_patterns, PatternVariant::AltStaircasesInvRight);
    let total_alt = alt_left + alt_right + alt_left_inv + alt_right_inv;
    let d_left = count(&chart.detected_patterns, PatternVariant::DStaircaseLeft);
    let d_right = count(&chart.detected_patterns, PatternVariant::DStaircaseRight);
    let d_left_inv =
        count(&chart.detected_patterns, PatternVariant::DStaircaseInvLeft);
    let d_right_inv =
        count(&chart.detected_patterns, PatternVariant::DStaircaseInvRight);
    let total_double = d_left + d_right + d_left_inv + d_right_inv;
    obj.insert(
        "staircases".to_string(),
        serde_json::json!({
            "total_staircases": total_staircases,
            "left_staircases": left_staircases,
            "right_staircases": right_staircases,
            "left_inv_staircases": left_inv_staircases,
            "right_inv_staircases": right_inv_staircases,
            "total_alt_staircases": total_alt,
            "left_alt_staircases": alt_left,
            "right_alt_staircases": alt_right,
            "left_inv_alt_staircases": alt_left_inv,
            "right_inv_alt_staircases": alt_right_inv,
            "total_double_staircases": total_double,
            "left_double_staircases": d_left,
            "right_double_staircases": d_right,
            "left_inv_double_staircases": d_left_inv,
            "right_inv_double_staircases": d_right_inv,
        }),
    );

    // Sweeps
    let left_sweeps = count(&chart.detected_patterns, PatternVariant::SweepLeft);
    let right_sweeps = count(&chart.detected_patterns, PatternVariant::SweepRight);
    let left_inv_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepInvLeft);
    let right_inv_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepInvRight);
    let total_sweeps =
        left_sweeps + right_sweeps + left_inv_sweeps + right_inv_sweeps;
    obj.insert(
        "sweeps".to_string(),
        serde_json::json!({
            "total_sweeps": total_sweeps,
            "left_sweeps": left_sweeps,
            "right_sweeps": right_sweeps,
            "left_inv_sweeps": left_inv_sweeps,
            "right_inv_sweeps": right_inv_sweeps,
        }),
    );

    // Candle Sweeps
    let left_candle_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepCandleLeft);
    let right_candle_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepCandleRight);
    let left_inv_candle_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepCandleInvLeft);
    let right_inv_candle_sweeps =
        count(&chart.detected_patterns, PatternVariant::SweepCandleInvRight);
    let total_candle_sweeps = left_candle_sweeps
        + right_candle_sweeps
        + left_inv_candle_sweeps
        + right_inv_candle_sweeps;
    obj.insert(
        "candle_sweeps".to_string(),
        serde_json::json!({
            "total_candle_sweeps": total_candle_sweeps,
            "left_candle_sweeps": left_candle_sweeps,
            "right_candle_sweeps": right_candle_sweeps,
            "left_inv_candle_sweeps": left_inv_candle_sweeps,
            "right_inv_candle_sweeps": right_inv_candle_sweeps,
        }),
    );

    // Copters
    let left_copters =
        count(&chart.detected_patterns, PatternVariant::CopterLeft);
    let right_copters =
        count(&chart.detected_patterns, PatternVariant::CopterRight);
    let left_inv_copters =
        count(&chart.detected_patterns, PatternVariant::CopterInvLeft);
    let right_inv_copters =
        count(&chart.detected_patterns, PatternVariant::CopterInvRight);
    let total_copters =
        left_copters + right_copters + left_inv_copters + right_inv_copters;
    obj.insert(
        "copters".to_string(),
        serde_json::json!({
            "total_copters": total_copters,
            "left_copters": left_copters,
            "right_copters": right_copters,
            "left_inv_copters": left_inv_copters,
            "right_inv_copters": right_inv_copters,
        }),
    );

    // Spirals
    let left_spirals =
        count(&chart.detected_patterns, PatternVariant::SpiralLeft);
    let right_spirals =
        count(&chart.detected_patterns, PatternVariant::SpiralRight);
    let left_inv_spirals =
        count(&chart.detected_patterns, PatternVariant::SpiralInvLeft);
    let right_inv_spirals =
        count(&chart.detected_patterns, PatternVariant::SpiralInvRight);
    let total_spirals =
        left_spirals + right_spirals + left_inv_spirals + right_inv_spirals;
    obj.insert(
        "spirals".to_string(),
        serde_json::json!({
            "total_spirals": total_spirals,
            "left_spirals": left_spirals,
            "right_spirals": right_spirals,
            "left_inv_spirals": left_inv_spirals,
            "right_inv_spirals": right_inv_spirals,
        }),
    );

    // Turbo Candles
    let left_turbo_candles =
        count(&chart.detected_patterns, PatternVariant::TurboCandleLeft);
    let right_turbo_candles =
        count(&chart.detected_patterns, PatternVariant::TurboCandleRight);
    let left_inv_turbo_candles = count(
        &chart.detected_patterns,
        PatternVariant::TurboCandleInvLeft,
    );
    let right_inv_turbo_candles = count(
        &chart.detected_patterns,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles = left_turbo_candles
        + right_turbo_candles
        + left_inv_turbo_candles
        + right_inv_turbo_candles;
    obj.insert(
        "turbo_candles".to_string(),
        serde_json::json!({
            "total_turbo_candles": total_turbo_candles,
            "left_turbo_candles": left_turbo_candles,
            "right_turbo_candles": right_turbo_candles,
            "left_inv_turbo_candles": left_inv_turbo_candles,
            "right_inv_turbo_candles": right_inv_turbo_candles,
        }),
    );

    // Hip Breakers
    let left_hip_breakers =
        count(&chart.detected_patterns, PatternVariant::HipBreakerLeft);
    let right_hip_breakers =
        count(&chart.detected_patterns, PatternVariant::HipBreakerRight);
    let left_inv_hip_breakers = count(
        &chart.detected_patterns,
        PatternVariant::HipBreakerInvLeft,
    );
    let right_inv_hip_breakers = count(
        &chart.detected_patterns,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers = left_hip_breakers
        + right_hip_breakers
        + left_inv_hip_breakers
        + right_inv_hip_breakers;
    obj.insert(
        "hip_breakers".to_string(),
        serde_json::json!({
            "total_hip_breakers": total_hip_breakers,
            "left_hip_breakers": left_hip_breakers,
            "right_hip_breakers": right_hip_breakers,
            "left_inv_hip_breakers": left_inv_hip_breakers,
            "right_inv_hip_breakers": right_inv_hip_breakers,
        }),
    );

    // Doritos
    let left_doritos =
        count(&chart.detected_patterns, PatternVariant::DoritoLeft);
    let right_doritos =
        count(&chart.detected_patterns, PatternVariant::DoritoRight);
    let left_inv_doritos =
        count(&chart.detected_patterns, PatternVariant::DoritoInvLeft);
    let right_inv_doritos =
        count(&chart.detected_patterns, PatternVariant::DoritoInvRight);
    let total_doritos =
        left_doritos + right_doritos + left_inv_doritos + right_inv_doritos;
    obj.insert(
        "doritos".to_string(),
        serde_json::json!({
            "total_doritos": total_doritos,
            "left_doritos": left_doritos,
            "right_doritos": right_doritos,
            "left_inv_doritos": left_inv_doritos,
            "right_inv_doritos": right_inv_doritos,
        }),
    );

    // Luchis
    let left_du_luchis =
        count(&chart.detected_patterns, PatternVariant::LuchiLeftDU);
    let left_ud_luchis =
        count(&chart.detected_patterns, PatternVariant::LuchiLeftUD);
    let right_du_luchis =
        count(&chart.detected_patterns, PatternVariant::LuchiRightDU);
    let right_ud_luchis =
        count(&chart.detected_patterns, PatternVariant::LuchiRightUD);
    let total_luchis =
        left_du_luchis + left_ud_luchis + right_du_luchis + right_ud_luchis;
    obj.insert(
        "luchis".to_string(),
        serde_json::json!({
            "total_luchis": total_luchis,
            "left_du_luchis": left_du_luchis,
            "left_ud_luchis": left_ud_luchis,
            "right_du_luchis": right_du_luchis,
            "right_ud_luchis": right_ud_luchis,
        }),
    );

    // Custom patterns
    let mut custom = JsonMap::new();
    for cp in &chart.custom_patterns {
        custom.insert(cp.pattern.clone(), JsonValue::from(cp.count));
    }
    obj.insert("custom_patterns".to_string(), JsonValue::Object(custom));

    JsonValue::Object(obj)
}

fn json_tech_counts(chart: &ChartSummary) -> JsonValue {
    serde_json::json!({
        "crossovers": chart.tech_counts.crossovers,
        "footswitches": chart.tech_counts.footswitches,
        "up_footswitches": chart.tech_counts.up_footswitches,
        "down_footswitches": chart.tech_counts.down_footswitches,
        "sideswitches": chart.tech_counts.sideswitches,
        "jacks": chart.tech_counts.jacks,
        "brackets": chart.tech_counts.brackets,
        "doublesteps": chart.tech_counts.doublesteps,
    })
}

fn write_indent<W: Write>(writer: &mut W, indent: usize) -> io::Result<()> {
    for _ in 0..indent {
        writer.write_all(b" ")?;
    }
    Ok(())
}

fn write_json_string<W: Write>(writer: &mut W, s: &str) -> io::Result<()> {
    let encoded = serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string());
    writer.write_all(encoded.as_bytes())
}

fn write_json_number_for_key<W: Write>(
    writer: &mut W,
    key: Option<&str>,
    number: &JsonNumber,
) -> io::Result<()> {
    if let Some(i) = number.as_i64() {
        write!(writer, "{}", i)
    } else if let Some(u) = number.as_u64() {
        write!(writer, "{}", u)
    } else if let Some(f) = number.as_f64() {
        match key {
            Some("offset") => write!(writer, "{:.3}", f),
            Some("bpm") => write!(writer, "{}", f),
            _ => write!(writer, "{:.2}", f),
        }
    } else {
        write!(writer, "0")
    }
}

fn write_json_value_with_key<W: Write>(
    writer: &mut W,
    key: Option<&str>,
    value: &JsonValue,
    indent: usize,
) -> io::Result<()> {
    match value {
        JsonValue::Null => writer.write_all(b"null"),
        JsonValue::Bool(b) => {
            if *b {
                writer.write_all(b"true")
            } else {
                writer.write_all(b"false")
            }
        }
        JsonValue::Number(n) => write_json_number_for_key(writer, key, n),
        JsonValue::String(s) => write_json_string(writer, s),
        JsonValue::Array(arr) => write_json_array(writer, arr, indent),
        JsonValue::Object(obj) => write_json_object(writer, obj, indent),
    }
}

fn write_json_array<W: Write>(
    writer: &mut W,
    arr: &[JsonValue],
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"[\n")?;
    let mut first = true;
    for value in arr {
        if !first {
            writer.write_all(b",\n")?;
        }
        first = false;
        write_indent(writer, indent + 2)?;
        write_json_value_with_key(writer, None, value, indent + 2)?;
    }
    writer.write_all(b"\n")?;
    write_indent(writer, indent)?;
    writer.write_all(b"]")
}

fn write_json_object<W: Write>(
    writer: &mut W,
    obj: &JsonMap<String, JsonValue>,
    indent: usize,
) -> io::Result<()> {
    writer.write_all(b"{\n")?;
    let mut first = true;
    for (key, value) in obj {
        if !first {
            writer.write_all(b",\n")?;
        }
        first = false;
        write_indent(writer, indent + 2)?;
        write_json_string(writer, key)?;
        writer.write_all(b": ")?;
        write_json_value_with_key(writer, Some(key.as_str()), value, indent + 2)?;
    }
    if !obj.is_empty() {
        writer.write_all(b"\n")?;
    }
    write_indent(writer, indent)?;
    writer.write_all(b"}")
}

pub fn print_json_all(simfile: &SimfileSummary) {
    let bpm_value = if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        JsonValue::from(simfile.min_bpm)
    } else {
        JsonValue::from(format!("{:.0}-{:.0}", simfile.min_bpm, simfile.max_bpm))
    };

    let charts: Vec<JsonValue> = simfile
        .charts
        .iter()
        .map(|chart| {
            serde_json::json!({
                "chart_info": json_chart_info(chart),
                "arrow_stats": json_arrow_stats(chart),
                "stream_info": json_stream_info(chart),
                "nps": json_nps(chart),
                "breakdown": json_breakdown(chart),
                "mono_candle_stats": json_mono_candle_stats(chart),
                "pattern_counts": json_pattern_counts(chart),
                "tech_counts": json_tech_counts(chart),
            })
        })
        .collect();

    let mut root_obj = JsonMap::new();
    root_obj.insert("title".to_string(), JsonValue::from(simfile.title_str.clone()));
    root_obj.insert("subtitle".to_string(), JsonValue::from(simfile.subtitle_str.clone()));
    root_obj.insert("artist".to_string(), JsonValue::from(simfile.artist_str.clone()));
    root_obj.insert("title_trans".to_string(), JsonValue::from(simfile.titletranslit_str.clone()));
    root_obj.insert("subtitle_trans".to_string(), JsonValue::from(simfile.subtitletranslit_str.clone()));
    root_obj.insert("artist_trans".to_string(), JsonValue::from(simfile.artisttranslit_str.clone()));
    root_obj.insert("length".to_string(), JsonValue::from(simfile.total_length.to_string()));
    root_obj.insert("bpm".to_string(), bpm_value);
    root_obj.insert("min_bpm".to_string(), JsonValue::from(simfile.min_bpm));
    root_obj.insert("max_bpm".to_string(), JsonValue::from(simfile.max_bpm));
    root_obj.insert("average_bpm".to_string(), JsonValue::from(simfile.average_bpm));
    root_obj.insert("median_bpm".to_string(), JsonValue::from(simfile.median_bpm));
    root_obj.insert("bpm_data".to_string(), JsonValue::from(simfile.normalized_bpms.clone()));
    root_obj.insert("offset".to_string(), JsonValue::from(simfile.offset));
    root_obj.insert("charts".to_string(), JsonValue::from(charts));

    let root = JsonValue::Object(root_obj);

    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    if write_json_value_with_key(&mut handle, None, &root, 0).is_ok() {
        let _ = writeln!(handle);
    }
}

fn print_csv_all(simfile: &SimfileSummary) {
    let mut header = String::from(
        "Title,Subtitle,Artist,Title trans,Subtitle trans,Artist trans,Length,BPM,BPM Tier,min_bpm,max_bpm,average_bpm,median bpm,BPM-data,offset,file_md5_hash,\
step_type,difficulty,rating,step_artist,tech_notation,sha1_hash,bpm_neutral_hash,\
total_arrows,left_arrows,down_arrows,up_arrows,right_arrows,\
total_steps,jumps,hands,holds,rolls,mines,lifts,fakes,\
total_streams,16th_streams,20th_streams,24th_streams,32nd_streams,total_breaks,stream_percent,adj_stream_percent,max_nps,median_nps,matrix_rating,mono_total,\
total_candles,left_foot_candles,right_foot_candles,candles_percent,\
total_mono,left_face_mono,right_face_mono,mono_percent,\
total_boxes,lr_boxes,ud_boxes,corner_boxes,ld_boxes,lu_boxes,rd_boxes,ru_boxes,\
total_anchors,left_anchors,down_anchors,up_anchors,right_anchors,\
detailed_breakdown,partial_breakdown,simple_breakdown,\
total_towers,lr_towers,ud_towers,corner_towers,ld_towers,lu_towers,rd_towers,ru_towers,\
total_triangles,ldl_triangles,lul_triangles,rdr_triangles,rur_triangles,\
crossovers,half_crossovers,full_crossovers,footswitches,up_footswitches,down_footswitches,sideswitches,jacks,brackets,doublesteps,\
total staircases,left_staircases,right_staircases,left_inv_staircases,right_inv_staircases,\
total_alt_staircases,left_alt_staircases,right_alt_staircases,left_inv_alt_staircases,right_inv_alt_staircases,\
total_double_staircases,left_double_staircases,right_double_staircases,left_inv_double_staircases,right_inv_double_staircases,\
total_sweeps,left_sweeps,right_sweeps,left_inv_sweeps,right_inv_sweeps,\
total_candle_sweeps,left_candle_sweeps,right_candle_sweeps,left_inv_candle_sweeps,right_inv_candle_sweeps,\
total copters,left_copters,right_copters,left_inv_copters,right_inv_copters,\
total_spirals,left_spirals,right_spirals,left_inv_spirals,right_inv_spirals,\
total_turbo_candles,left_turbo_candles,right_turbo_candles,left_inv_turbo_candles,right_inv_turbo_candles,\
total_hip_breakers,left_hip_breakers,right_hip_breakers,left_inv_hip_breakers,right_inv_hip_breakers,\
total_doritos,left_doritos,right_doritos,left_inv_doritos,right_inv_doritos,\
total_luchis,left_du_luchis,left_ud_luchis,right_du_luchis,right_ud_luchis"
    );

    if let Some(first_chart) = simfile.charts.first() {
        for cp in &first_chart.custom_patterns {
            header.push(',');
            header.push_str("custom_pattern_");
            header.push_str(&cp.pattern);
        }
    }

    println!("{}", header);

    for chart in &simfile.charts {
        print_csv_row(simfile, chart);
    }
}

fn print_csv_row(simfile: &SimfileSummary, chart: &ChartSummary) {
    fn esc_csv(s: &str) -> String {
        if s.contains('"') || s.contains(',') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }

    print!("{},{},{},{},{},{},{},",
        esc_csv(&simfile.title_str),
        esc_csv(&simfile.subtitle_str),
        esc_csv(&simfile.artist_str),
        esc_csv(&simfile.titletranslit_str),
        esc_csv(&simfile.subtitletranslit_str),
        esc_csv(&simfile.artisttranslit_str),
        format_duration(simfile.total_length),
    );
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        print!("{},", simfile.min_bpm);
    } else {
        print!("{}-{},", simfile.min_bpm, simfile.max_bpm);
    }
    print!("{},{},{},{},{},{},",
        simfile.min_bpm,
        simfile.max_bpm,
        simfile.average_bpm,
        simfile.median_bpm,
        esc_csv(&simfile.normalized_bpms),
        simfile.offset,
    );
    print!(",");

    print!("{},{},{},{},{},{},{},",
        esc_csv(&chart.step_type_str),
        esc_csv(&chart.difficulty_str),
        esc_csv(&chart.rating_str),
        esc_csv(&chart.step_artist_str.join(", ")),
        esc_csv(&chart.tech_notation_str),
        esc_csv(&chart.short_hash),
        esc_csv(&chart.bpm_neutral_hash),
    );

    print!("{},{},{},{},{},",
        chart.stats.total_arrows,
        chart.stats.left,
        chart.stats.down,
        chart.stats.up,
        chart.stats.right,
    );

    print!("{},{},{},{},{},{},{},{},",
        chart.stats.total_steps,
        chart.stats.jumps,
        chart.stats.hands,
        chart.stats.holds,
        chart.stats.rolls,
        chart.stats.mines,
        chart.stats.lifts,
        chart.stats.fakes,
    );

    let total_streams = chart.total_streams;
    let total_breaks = chart.stream_counts.total_breaks;
    let stream_percent = if total_streams + total_breaks > 0 {
        (total_streams as f64 / (total_streams + total_breaks) as f64) * 100.0
    } else { 0.0 };
    print!("{},{},{},{},{},{},{},",
        total_streams,
        chart.stream_counts.run16_streams,
        chart.stream_counts.run20_streams,
        chart.stream_counts.run24_streams,
        chart.stream_counts.run32_streams,
        total_breaks,
        stream_percent,
    );
    print!(",");

    print!("{},{},{},{},",
        chart.max_nps,
        chart.median_nps,
        chart.matrix_rating,
        chart.mono_total,
    );

    let left_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleRight);
    let total_candles = left_foot_candles + right_foot_candles;
    print!("{},{},{},{},",
        total_candles,
        left_foot_candles,
        right_foot_candles,
        chart.candle_percent,
    );

    print!("{},{},{},{},",
        chart.mono_total,
        chart.facing_left,
        chart.facing_right,
        chart.mono_percent,
    );

    let lr_boxes = count(&chart.detected_patterns, PatternVariant::BoxLR);
    let ud_boxes = count(&chart.detected_patterns, PatternVariant::BoxUD);
    let ld_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLD);
    let lu_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLU);
    let rd_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRD);
    let ru_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRU);
    let corner_boxes = ld_boxes + lu_boxes + rd_boxes + ru_boxes;
    let total_boxes = lr_boxes + ud_boxes + corner_boxes;
    print!("{},{},{},{},{},{},{},{},",
        total_boxes,
        lr_boxes,
        ud_boxes,
        corner_boxes,
        ld_boxes,
        lu_boxes,
        rd_boxes,
        ru_boxes,
    );

    let total_anchors = chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    print!("{},{},{},{},{},",
        total_anchors,
        chart.anchor_left,
        chart.anchor_down,
        chart.anchor_up,
        chart.anchor_right,
    );

    print!("{},{},{},",
        esc_csv(&chart.detailed),
        esc_csv(&chart.partial),
        esc_csv(&chart.simple),
    );

    let lr_towers = count(&chart.detected_patterns, PatternVariant::TowerLR);
    let ud_towers = count(&chart.detected_patterns, PatternVariant::TowerUD);
    let ld_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLD);
    let lu_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLU);
    let rd_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRD);
    let ru_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRU);
    let corner_towers = ld_towers + lu_towers + rd_towers + ru_towers;
    let total_towers = lr_towers + ud_towers + corner_towers;
    print!("{},{},{},{},{},{},{},{},",
        total_towers,
        lr_towers,
        ud_towers,
        corner_towers,
        ld_towers,
        lu_towers,
        rd_towers,
        ru_towers,
    );

    let ldl_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLDL);
    let lul_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLUL);
    let rdr_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRDR);
    let rur_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRUR);
    let total_triangles = ldl_triangles + lul_triangles + rdr_triangles + rur_triangles;
    print!("{},{},{},{},{},",
        total_triangles,
        ldl_triangles,
        lul_triangles,
        rdr_triangles,
        rur_triangles,
    );

    print!("{},{},{},{},{},{},{},{},",
        chart.tech_counts.crossovers,
        chart.tech_counts.footswitches,
        chart.tech_counts.up_footswitches,
        chart.tech_counts.down_footswitches,
        chart.tech_counts.sideswitches,
        chart.tech_counts.jacks,
        chart.tech_counts.brackets,
        chart.tech_counts.doublesteps,
    );

    let left_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseLeft);
    let right_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseRight);
    let left_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvLeft);
    let right_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvRight);
    let total_staircases = left_staircases + right_staircases + left_inv_staircases + right_inv_staircases;
    print!("{},{},{},{},{},",
        total_staircases,
        left_staircases,
        right_staircases,
        left_inv_staircases,
        right_inv_staircases,
    );

    let alt_left = count(&chart.detected_patterns, PatternVariant::AltStaircasesLeft);
    let alt_right = count(&chart.detected_patterns, PatternVariant::AltStaircasesRight);
    let alt_left_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvLeft);
    let alt_right_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvRight);
    let total_alt = alt_left + alt_right + alt_left_inv + alt_right_inv;

    let d_left = count(&chart.detected_patterns, PatternVariant::DStaircaseLeft);
    let d_right = count(&chart.detected_patterns, PatternVariant::DStaircaseRight);
    let d_left_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvLeft);
    let d_right_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvRight);
    let total_double = d_left + d_right + d_left_inv + d_right_inv;

    print!("{},{},{},{},{},{},{},{},{},{},",
        total_alt,
        alt_left,
        alt_right,
        alt_left_inv,
        alt_right_inv,
        total_double,
        d_left,
        d_right,
        d_left_inv,
        d_right_inv,
    );

    let left_sweeps = count(&chart.detected_patterns, PatternVariant::SweepLeft);
    let right_sweeps = count(&chart.detected_patterns, PatternVariant::SweepRight);
    let left_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvLeft);
    let right_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvRight);
    let total_sweeps = left_sweeps + right_sweeps + left_inv_sweeps + right_inv_sweeps;
    print!("{},{},{},{},{},",
        total_sweeps,
        left_sweeps,
        right_sweeps,
        left_inv_sweeps,
        right_inv_sweeps,
    );

    let left_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleLeft);
    let right_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleRight);
    let left_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvLeft);
    let right_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvRight);
    let total_candle_sweeps = left_candle_sweeps + right_candle_sweeps + left_inv_candle_sweeps + right_inv_candle_sweeps;
    print!("{},{},{},{},{},",
        total_candle_sweeps,
        left_candle_sweeps,
        right_candle_sweeps,
        left_inv_candle_sweeps,
        right_inv_candle_sweeps,
    );

    let left_copters = count(&chart.detected_patterns, PatternVariant::CopterLeft);
    let right_copters = count(&chart.detected_patterns, PatternVariant::CopterRight);
    let left_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvLeft);
    let right_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvRight);
    let total_copters = left_copters + right_copters + left_inv_copters + right_inv_copters;
    print!("{},{},{},{},{},",
        total_copters,
        left_copters,
        right_copters,
        left_inv_copters,
        right_inv_copters,
    );

    let left_spirals = count(&chart.detected_patterns, PatternVariant::SpiralLeft);
    let right_spirals = count(&chart.detected_patterns, PatternVariant::SpiralRight);
    let left_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvLeft);
    let right_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvRight);
    let total_spirals = left_spirals + right_spirals + left_inv_spirals + right_inv_spirals;
    print!("{},{},{},{},{},",
        total_spirals,
        left_spirals,
        right_spirals,
        left_inv_spirals,
        right_inv_spirals,
    );

    let left_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleLeft);
    let right_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleRight);
    let left_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvLeft);
    let right_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvRight);
    let total_turbo_candles = left_turbo_candles + right_turbo_candles + left_inv_turbo_candles + right_inv_turbo_candles;
    print!("{},{},{},{},{},",
        total_turbo_candles,
        left_turbo_candles,
        right_turbo_candles,
        left_inv_turbo_candles,
        right_inv_turbo_candles,
    );

    let left_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerLeft);
    let right_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerRight);
    let left_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvLeft);
    let right_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvRight);
    let total_hip_breakers = left_hip_breakers + right_hip_breakers + left_inv_hip_breakers + right_inv_hip_breakers;
    print!("{},{},{},{},{},",
        total_hip_breakers,
        left_hip_breakers,
        right_hip_breakers,
        left_inv_hip_breakers,
        right_inv_hip_breakers,
    );

    let left_doritos = count(&chart.detected_patterns, PatternVariant::DoritoLeft);
    let right_doritos = count(&chart.detected_patterns, PatternVariant::DoritoRight);
    let left_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvLeft);
    let right_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvRight);
    let total_doritos = left_doritos + right_doritos + left_inv_doritos + right_inv_doritos;
    print!("{},{},{},{},{},",
        total_doritos,
        left_doritos,
        right_doritos,
        left_inv_doritos,
        right_inv_doritos,
    );

    let left_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftDU);
    let left_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftUD);
    let right_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightDU);
    let right_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightUD);
    let total_luchis = left_du_luchis + left_ud_luchis + right_du_luchis + right_ud_luchis;
    print!("{},{},{},{},{}",
        total_luchis,
        left_du_luchis,
        left_ud_luchis,
        right_du_luchis,
        right_ud_luchis,
    );

    for cp in &chart.custom_patterns {
        print!(",{}", cp.count);
    }

    println!();
}

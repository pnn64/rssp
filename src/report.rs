use std::collections::HashMap;
use std::time::Duration;

use crate::patterns::PatternVariant;
use crate::stats::{ArrowStats, StreamCounts};

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
    pub short_hash:        String,
    pub bpm_neutral_hash:  String,
    pub elapsed:           Duration,
    pub measure_densities: Vec<usize>,
    pub measure_nps_vec:   Vec<f64>,
}

pub struct SimfileSummary {
    pub title_str:            String,
    pub subtitle_str:         String,
    pub artist_str:           String,
    pub titletranslit_str:    String,
    pub subtitletranslit_str: String,
    pub artisttranslit_str:   String,
    pub offset:               f64,
    pub normalized_bpms:      String,
    pub min_bpm:              f64,
    pub max_bpm:              f64,
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

// Helper function to escape strings for JSON
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

// Helper function to print an indented line
fn print_indented(line: &str, indent: usize) {
    println!("{}{}", " ".repeat(indent), line);
}

// Helper functions to print key-value pairs with/without trailing commas
fn print_kv_str(key: &str, value: &str, indent: usize) {
    print_indented(&format!("\"{}\": \"{}\",", key, esc(value)), indent);
}

fn print_kv_str_last(key: &str, value: &str, indent: usize) {
    print_indented(&format!("\"{}\": \"{}\"", key, esc(value)), indent);
}

fn print_kv_int(key: &str, value: u32, indent: usize) {
    print_indented(&format!("\"{}\": {},", key, value), indent);
}

fn print_kv_int_last(key: &str, value: u32, indent: usize) {
    print_indented(&format!("\"{}\": {}", key, value), indent);
}

fn print_kv_float(key: &str, value: f64, indent: usize) {
    print_indented(&format!("\"{}\": {:.2},", key, value), indent);
}

fn print_kv_float_last(key: &str, value: f64, indent: usize) {
    print_indented(&format!("\"{}\": {:.2}", key, value), indent);
}

fn print_kv_array(key: &str, values: &[&str], indent: usize) {
    let mut line = format!("\"{}\": [", key);
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            line.push_str(", ");
        }
        line.push_str(&format!("\"{}\"", esc(val)));
    }
    line.push_str("],");
    print_indented(&line, indent);
}

// Helper function to count pattern occurrences
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
    // TODO: BPM Tier
    println!("Average BPM: {:.2}", simfile.average_bpm);
    println!("Median BPM: {:.2}", simfile.median_bpm);
    println!("BPM Data: {}", simfile.normalized_bpms);
    println!("Offset: {:.3}", simfile.offset);
    // TODO: file_md5_hash

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
}

fn print_chart_info_fields(chart: &ChartSummary, indent: usize) {
    print_kv_str("step_type", &chart.step_type_str, indent);
    print_kv_str("difficulty", &chart.difficulty_str, indent);
    print_kv_float("tier_bpm", chart.tier_bpm, indent);
    print_kv_str("rating", &chart.rating_str, indent);
    print_kv_float("matrix_rating", chart.matrix_rating, indent);
    let step_artists_refs: Vec<&str> = chart.step_artist_str.iter().map(|s| s.as_str()).collect();
    print_kv_array("step_artists", &step_artists_refs, indent);
    print_kv_str("tech_notation", &chart.tech_notation_str, indent);
    print_kv_str("sha1", &chart.short_hash, indent);
    print_kv_str_last("bpm_neutral_sha1", &chart.bpm_neutral_hash, indent);
}


fn print_arrow_stats_fields(chart: &ChartSummary, indent: usize) {
    print_kv_int("total_arrows", chart.stats.total_arrows, indent);
    print_kv_int("left_arrows", chart.stats.left, indent);
    print_kv_int("down_arrows", chart.stats.down, indent);
    print_kv_int("up_arrows", chart.stats.up, indent);
    print_kv_int("right_arrows", chart.stats.right, indent);
    print_kv_int("total_steps", chart.stats.total_steps, indent);
    print_kv_int("jumps", chart.stats.jumps, indent);
    print_kv_int("hands", chart.stats.hands, indent);
    print_kv_int("holds", chart.stats.holds, indent);
    print_kv_int("rolls", chart.stats.rolls, indent);
    print_kv_int("mines", chart.stats.mines, indent);
    print_kv_int("lifts", chart.stats.lifts, indent);
    print_kv_int_last("fakes", chart.stats.fakes, indent);
}

fn print_stream_info_fields(chart: &ChartSummary, indent: usize) {
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let adj_stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else { 0.0 };

    let stream_percent = if total_measures > 0 {
        (total_stream as f64 / total_measures as f64) * 100.0
    } else { 0.0 };
    print_kv_int("total_streams", total_stream, indent);
    print_kv_int("16th_streams", chart.stream_counts.run16_streams, indent);
    print_kv_int("20th_streams", chart.stream_counts.run20_streams, indent);
    print_kv_int("24th_streams", chart.stream_counts.run24_streams, indent);
    print_kv_int("32nd_streams", chart.stream_counts.run32_streams, indent);
    print_kv_int("total_breaks", total_break, indent);
    print_kv_float("stream_percent", stream_percent, indent);
    print_kv_float("adj_stream_percent", adj_stream_percent, indent);
    print_kv_float_last("break_percent", 100.0 - adj_stream_percent, indent);
}

fn print_nps_fields(chart: &ChartSummary, indent: usize) {
    print_kv_float("max_nps", chart.max_nps, indent);
    print_kv_float_last("median_nps", chart.median_nps, indent);
}

fn print_mono_candle_stats_fields(chart: &ChartSummary, indent: usize) {
    let left_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleLeft);
    let right_foot_candles = count(&chart.detected_patterns, PatternVariant::CandleRight);
    let total_candles = left_foot_candles + right_foot_candles;
    print_kv_int("total_candles", total_candles, indent);
    print_kv_int("left_foot_candles", left_foot_candles, indent);
    print_kv_int("right_foot_candles", right_foot_candles, indent);
    print_kv_float("candles_percent", chart.candle_percent, indent);
    print_kv_int("total_mono", chart.mono_total, indent);
    print_kv_int("left_face_mono", chart.facing_left, indent);
    print_kv_int("right_face_mono", chart.facing_right, indent);
    print_kv_float_last("mono_percent", chart.mono_percent, indent);
}

fn print_pattern_counts_fields(chart: &ChartSummary, indent: usize) {
    // Boxes
    print_indented("\"boxes\": {", indent);
    let lr_boxes = count(&chart.detected_patterns, PatternVariant::BoxLR);
    let ud_boxes = count(&chart.detected_patterns, PatternVariant::BoxUD);
    let ld_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLD);
    let lu_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerLU);
    let rd_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRD);
    let ru_boxes = count(&chart.detected_patterns, PatternVariant::BoxCornerRU);
    let corner_boxes = ld_boxes + lu_boxes + rd_boxes + ru_boxes;
    let total_boxes = lr_boxes + ud_boxes + corner_boxes;
    print_kv_int("total_boxes", total_boxes, indent + 2);
    print_kv_int("lr_boxes", lr_boxes, indent + 2);
    print_kv_int("ud_boxes", ud_boxes, indent + 2);
    print_kv_int("corner_boxes", corner_boxes, indent + 2);
    print_kv_int("ld_boxes", ld_boxes, indent + 2);
    print_kv_int("lu_boxes", lu_boxes, indent + 2);
    print_kv_int("rd_boxes", rd_boxes, indent + 2);
    print_kv_int_last("ru_boxes", ru_boxes, indent + 2);
    print_indented("},", indent);

    // Anchors
    print_indented("\"anchors\": {", indent);
    let total_anchors = chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right;
    print_kv_int("total_anchors", total_anchors, indent + 2);
    print_kv_int("left_anchors", chart.anchor_left, indent + 2);
    print_kv_int("down_anchors", chart.anchor_down, indent + 2);
    print_kv_int("up_anchors", chart.anchor_up, indent + 2);
    print_kv_int_last("right_anchors", chart.anchor_right, indent + 2);
    print_indented("},", indent);

    // Towers
    print_indented("\"towers\": {", indent);
    let lr_towers = count(&chart.detected_patterns, PatternVariant::TowerLR);
    let ud_towers = count(&chart.detected_patterns, PatternVariant::TowerUD);
    let ld_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLD);
    let lu_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerLU);
    let rd_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRD);
    let ru_towers = count(&chart.detected_patterns, PatternVariant::TowerCornerRU);
    let corner_towers = ld_towers + lu_towers + rd_towers + ru_towers;
    let total_towers = lr_towers + ud_towers + corner_towers;
    print_kv_int("total_towers", total_towers, indent + 2);
    print_kv_int("lr_towers", lr_towers, indent + 2);
    print_kv_int("ud_towers", ud_towers, indent + 2);
    print_kv_int("corner_towers", corner_towers, indent + 2);
    print_kv_int("ld_towers", ld_towers, indent + 2);
    print_kv_int("lu_towers", lu_towers, indent + 2);
    print_kv_int("rd_towers", rd_towers, indent + 2);
    print_kv_int_last("ru_towers", ru_towers, indent + 2);
    print_indented("},", indent);

    // Triangles
    print_indented("\"triangles\": {", indent);
    let ldl_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLDL);
    let lul_triangles = count(&chart.detected_patterns, PatternVariant::TriangleLUL);
    let rdr_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRDR);
    let rur_triangles = count(&chart.detected_patterns, PatternVariant::TriangleRUR);
    let total_triangles = ldl_triangles + lul_triangles + rdr_triangles + rur_triangles;
    print_kv_int("total_triangles", total_triangles, indent + 2);
    print_kv_int("ldl_triangles", ldl_triangles, indent + 2);
    print_kv_int("lul_triangles", lul_triangles, indent + 2);
    print_kv_int("rdr_triangles", rdr_triangles, indent + 2);
    print_kv_int_last("rur_triangles", rur_triangles, indent + 2);
    print_indented("},", indent);

    // Staircases
    print_indented("\"staircases\": {", indent);
    let left_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseLeft);
    let right_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseRight);
    let left_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvLeft);
    let right_inv_staircases = count(&chart.detected_patterns, PatternVariant::StaircaseInvRight);
    let total_staircases = left_staircases + right_staircases + left_inv_staircases + right_inv_staircases;
    print_kv_int("total_staircases", total_staircases, indent + 2);
    print_kv_int("left_staircases", left_staircases, indent + 2);
    print_kv_int("right_staircases", right_staircases, indent + 2);
    print_kv_int("left_inv_staircases", left_inv_staircases, indent + 2);
    print_kv_int("right_inv_staircases", right_inv_staircases, indent + 2);
    let alt_left = count(&chart.detected_patterns, PatternVariant::AltStaircasesLeft);
    let alt_right = count(&chart.detected_patterns, PatternVariant::AltStaircasesRight);
    let alt_left_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvLeft);
    let alt_right_inv = count(&chart.detected_patterns, PatternVariant::AltStaircasesInvRight);
    let total_alt = alt_left + alt_right + alt_left_inv + alt_right_inv;
    print_kv_int("total_alt_staircases", total_alt, indent + 2);
    print_kv_int("left_alt_staircases", alt_left, indent + 2);
    print_kv_int("right_alt_staircases", alt_right, indent + 2);
    print_kv_int("left_inv_alt_staircases", alt_left_inv, indent + 2);
    print_kv_int("right_inv_alt_staircases", alt_right_inv, indent + 2);

    let d_left = count(&chart.detected_patterns, PatternVariant::DStaircaseLeft);
    let d_right = count(&chart.detected_patterns, PatternVariant::DStaircaseRight);
    let d_left_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvLeft);
    let d_right_inv = count(&chart.detected_patterns, PatternVariant::DStaircaseInvRight);
    let total_double = d_left + d_right + d_left_inv + d_right_inv;
    print_kv_int("total_double_staircases", total_double, indent + 2);
    print_kv_int("left_double_staircases", d_left, indent + 2);
    print_kv_int("right_double_staircases", d_right, indent + 2);
    print_kv_int("left_inv_double_staircases", d_left_inv, indent + 2);
    print_kv_int_last("right_inv_double_staircases", d_right_inv, indent + 2);
    print_indented("},", indent);

    // Sweeps
    print_indented("\"sweeps\": {", indent);
    let left_sweeps = count(&chart.detected_patterns, PatternVariant::SweepLeft);
    let right_sweeps = count(&chart.detected_patterns, PatternVariant::SweepRight);
    let left_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvLeft);
    let right_inv_sweeps = count(&chart.detected_patterns, PatternVariant::SweepInvRight);
    let total_sweeps = left_sweeps + right_sweeps + left_inv_sweeps + right_inv_sweeps;
    print_kv_int("total_sweeps", total_sweeps, indent + 2);
    print_kv_int("left_sweeps", left_sweeps, indent + 2);
    print_kv_int("right_sweeps", right_sweeps, indent + 2);
    print_kv_int("left_inv_sweeps", left_inv_sweeps, indent + 2);
    print_kv_int_last("right_inv_sweeps", right_inv_sweeps, indent + 2);
    print_indented("},", indent);

    // Candle Sweeps
    print_indented("\"candle_sweeps\": {", indent);
    let left_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleLeft);
    let right_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleRight);
    let left_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvLeft);
    let right_inv_candle_sweeps = count(&chart.detected_patterns, PatternVariant::SweepCandleInvRight);
    let total_candle_sweeps = left_candle_sweeps + right_candle_sweeps + left_inv_candle_sweeps + right_inv_candle_sweeps;
    print_kv_int("total_candle_sweeps", total_candle_sweeps, indent + 2);
    print_kv_int("left_candle_sweeps", left_candle_sweeps, indent + 2);
    print_kv_int("right_candle_sweeps", right_candle_sweeps, indent + 2);
    print_kv_int("left_inv_candle_sweeps", left_inv_candle_sweeps, indent + 2);
    print_kv_int_last("right_inv_candle_sweeps", right_inv_candle_sweeps, indent + 2);
    print_indented("},", indent);

    // Copters
    print_indented("\"copters\": {", indent);
    let left_copters = count(&chart.detected_patterns, PatternVariant::CopterLeft);
    let right_copters = count(&chart.detected_patterns, PatternVariant::CopterRight);
    let left_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvLeft);
    let right_inv_copters = count(&chart.detected_patterns, PatternVariant::CopterInvRight);
    let total_copters = left_copters + right_copters + left_inv_copters + right_inv_copters;
    print_kv_int("total_copters", total_copters, indent + 2);
    print_kv_int("left_copters", left_copters, indent + 2);
    print_kv_int("right_copters", right_copters, indent + 2);
    print_kv_int("left_inv_copters", left_inv_copters, indent + 2);
    print_kv_int_last("right_inv_copters", right_inv_copters, indent + 2);
    print_indented("},", indent);

    // Spirals
    print_indented("\"spirals\": {", indent);
    let left_spirals = count(&chart.detected_patterns, PatternVariant::SpiralLeft);
    let right_spirals = count(&chart.detected_patterns, PatternVariant::SpiralRight);
    let left_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvLeft);
    let right_inv_spirals = count(&chart.detected_patterns, PatternVariant::SpiralInvRight);
    let total_spirals = left_spirals + right_spirals + left_inv_spirals + right_inv_spirals;
    print_kv_int("total_spirals", total_spirals, indent + 2);
    print_kv_int("left_spirals", left_spirals, indent + 2);
    print_kv_int("right_spirals", right_spirals, indent + 2);
    print_kv_int("left_inv_spirals", left_inv_spirals, indent + 2);
    print_kv_int_last("right_inv_spirals", right_inv_spirals, indent + 2);
    print_indented("},", indent);

    // Turbo Candles
    print_indented("\"turbo_candles\": {", indent);
    let left_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleLeft);
    let right_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleRight);
    let left_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvLeft);
    let right_inv_turbo_candles = count(&chart.detected_patterns, PatternVariant::TurboCandleInvRight);
    let total_turbo_candles = left_turbo_candles + right_turbo_candles + left_inv_turbo_candles + right_inv_turbo_candles;
    print_kv_int("total_turbo_candles", total_turbo_candles, indent + 2);
    print_kv_int("left_turbo_candles", left_turbo_candles, indent + 2);
    print_kv_int("right_turbo_candles", right_turbo_candles, indent + 2);
    print_kv_int("left_inv_turbo_candles", left_inv_turbo_candles, indent + 2);
    print_kv_int_last("right_inv_turbo_candles", right_inv_turbo_candles, indent + 2);
    print_indented("},", indent);

    // Hip Breakers
    print_indented("\"hip_breakers\": {", indent);
    let left_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerLeft);
    let right_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerRight);
    let left_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvLeft);
    let right_inv_hip_breakers = count(&chart.detected_patterns, PatternVariant::HipBreakerInvRight);
    let total_hip_breakers = left_hip_breakers + right_hip_breakers + left_inv_hip_breakers + right_inv_hip_breakers;
    print_kv_int("total_hip_breakers", total_hip_breakers, indent + 2);
    print_kv_int("left_hip_breakers", left_hip_breakers, indent + 2);
    print_kv_int("right_hip_breakers", right_hip_breakers, indent + 2);
    print_kv_int("left_inv_hip_breakers", left_inv_hip_breakers, indent + 2);
    print_kv_int_last("right_inv_hip_breakers", right_inv_hip_breakers, indent + 2);
    print_indented("},", indent);

    // Doritos
    print_indented("\"doritos\": {", indent);
    let left_doritos = count(&chart.detected_patterns, PatternVariant::DoritoLeft);
    let right_doritos = count(&chart.detected_patterns, PatternVariant::DoritoRight);
    let left_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvLeft);
    let right_inv_doritos = count(&chart.detected_patterns, PatternVariant::DoritoInvRight);
    let total_doritos = left_doritos + right_doritos + left_inv_doritos + right_inv_doritos;
    print_kv_int("total_doritos", total_doritos, indent + 2);
    print_kv_int("left_doritos", left_doritos, indent + 2);
    print_kv_int("right_doritos", right_doritos, indent + 2);
    print_kv_int("left_inv_doritos", left_inv_doritos, indent + 2);
    print_kv_int_last("right_inv_doritos", right_inv_doritos, indent + 2);
    print_indented("},", indent);

    // Luchis
    print_indented("\"luchis\": {", indent);
    let left_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftDU);
    let left_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiLeftUD);
    let right_du_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightDU);
    let right_ud_luchis = count(&chart.detected_patterns, PatternVariant::LuchiRightUD);
    let total_luchis = left_du_luchis + left_ud_luchis + right_du_luchis + right_ud_luchis;
    print_kv_int("total_luchis", total_luchis, indent + 2);
    print_kv_int("left_du_luchis", left_du_luchis, indent + 2);
    print_kv_int("left_ud_luchis", left_ud_luchis, indent + 2);
    print_kv_int("right_du_luchis", right_du_luchis, indent + 2);
    print_kv_int_last("right_ud_luchis", right_ud_luchis, indent + 2);
    print_indented("}", indent);
}

fn print_breakdown_fields(chart: &ChartSummary, indent: usize) {
    print_kv_str("detailed_breakdown", &chart.detailed, indent);
    print_kv_str("partial_breakdown", &chart.partial, indent);
    print_kv_str_last("simple_breakdown", &chart.simple, indent);
}

// Main function to print a chart
fn print_json_chart(chart: &ChartSummary, is_last: bool) {
    print_indented("{", 4); // Start of chart object
    print_indented("\"chart_info\": {", 6);
    print_chart_info_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"arrow_stats\": {", 6);
    print_arrow_stats_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"stream_info\": {", 6);
    print_stream_info_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"nps\": {", 6);
    print_nps_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"breakdown\": {", 6);
    print_breakdown_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"mono_candle_stats\": {", 6);
    print_mono_candle_stats_fields(chart, 8);
    print_indented("},", 6);

    print_indented("\"pattern_counts\": {", 6);
    print_pattern_counts_fields(chart, 8);
    print_indented("}", 6);

    if is_last {
        print_indented("}", 4);
    } else {
        println!("{}{},", " ".repeat(4), "}");
    }
}

// Function to print the entire simfile report in JSON format
pub fn print_json_all(simfile: &SimfileSummary) {
    println!("{{");
    print_kv_str("title", &simfile.title_str, 2);
    print_kv_str("subtitle", &simfile.subtitle_str, 2);
    print_kv_str("artist", &simfile.artist_str, 2);
    print_kv_str("title_trans", &simfile.titletranslit_str, 2);
    print_kv_str("subtitle_trans", &simfile.subtitletranslit_str, 2);
    print_kv_str("artist_trans", &simfile.artisttranslit_str, 2);
    print_indented(&format!("\"length\": \"{}\",", simfile.total_length), 2);
    if (simfile.min_bpm - simfile.max_bpm).abs() < f64::EPSILON {
        print_indented(&format!("\"bpm\": {},", simfile.min_bpm), 2);
    } else {
        print_indented(&format!("\"bpm\": \"{:.0}-{:.0}\",", simfile.min_bpm, simfile.max_bpm), 2);
    }
    print_kv_float("min_bpm", simfile.min_bpm, 2);
    print_kv_float("max_bpm", simfile.max_bpm, 2);
    print_kv_float("average_bpm", simfile.average_bpm, 2);
    print_kv_float("median_bpm", simfile.median_bpm, 2);
    print_kv_str("bpm_data", &simfile.normalized_bpms, 2);
    print_indented(&format!("\"offset\": {:.3},", simfile.offset), 2);
    print_indented("\"charts\": [", 2);
    for (i, chart) in simfile.charts.iter().enumerate() {
        print_json_chart(chart, i + 1 == simfile.charts.len());
    }
    print_indented("]", 2);
    println!("}}");
}

fn print_csv_all(simfile: &SimfileSummary) {
    println!(
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
    // TODO: Tier BPM
    print!("{},{},{},{},{},{},",
        simfile.min_bpm,
        simfile.max_bpm,
        simfile.average_bpm,
        simfile.median_bpm,
        esc_csv(&simfile.normalized_bpms),
        simfile.offset,
    );
    // TODO: file_md5_hash
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
    // TODO: adj_stream_percent
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
    print!("{},{},{},{},{},",
        total_luchis,
        left_du_luchis,
        left_ud_luchis,
        right_du_luchis,
        right_ud_luchis,
    );

    println!();
}

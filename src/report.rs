use std::collections::HashMap;
use std::cmp::Ordering;
use std::io::{self, Write};
use std::time::Duration;

use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::patterns::{CustomPatternSummary, PatternVariant};
use crate::stats::{ArrowStats, StreamCounts};
use crate::step_parity::TechCounts;
use crate::timing::{SpeedUnit, TimingData, TimingSegments};

#[inline(always)]
fn compute_stream_percentages(
    total_streams: u32,
    total_breaks: u32,
    total_measures: usize,
) -> (f64, f64, f64) {
    let adj_stream_percent = if total_streams + total_breaks > 0 {
        (total_streams as f64 / (total_streams + total_breaks) as f64) * 100.0
    } else {
        0.0
    };

    let stream_percent = if total_measures > 0 {
        (total_streams as f64 / total_measures as f64) * 100.0
    } else {
        0.0
    };

    let break_percent = 100.0 - adj_stream_percent;

    (stream_percent, adj_stream_percent, break_percent)
}

#[derive(Clone, Copy)]
struct BoxParts {
    lr: u32,
    ud: u32,
    ld: u32,
    lu: u32,
    rd: u32,
    ru: u32,
}

#[inline(always)]
fn compute_box_parts(patterns: &HashMap<PatternVariant, u32>) -> BoxParts {
    BoxParts {
        lr: *patterns.get(&PatternVariant::BoxLR).unwrap_or(&0),
        ud: *patterns.get(&PatternVariant::BoxUD).unwrap_or(&0),
        ld: *patterns.get(&PatternVariant::BoxCornerLD).unwrap_or(&0),
        lu: *patterns.get(&PatternVariant::BoxCornerLU).unwrap_or(&0),
        rd: *patterns.get(&PatternVariant::BoxCornerRD).unwrap_or(&0),
        ru: *patterns.get(&PatternVariant::BoxCornerRU).unwrap_or(&0),
    }
}

#[derive(Clone, Copy)]
struct StairParts {
    left: u32,
    right: u32,
    left_inv: u32,
    right_inv: u32,
}

#[inline(always)]
fn compute_stair_parts(
    patterns: &HashMap<PatternVariant, u32>,
    left: PatternVariant,
    right: PatternVariant,
    left_inv: PatternVariant,
    right_inv: PatternVariant,
) -> StairParts {
    StairParts {
        left: *patterns.get(&left).unwrap_or(&0),
        right: *patterns.get(&right).unwrap_or(&0),
        left_inv: *patterns.get(&left_inv).unwrap_or(&0),
        right_inv: *patterns.get(&right_inv).unwrap_or(&0),
    }
}

#[derive(Clone, Copy)]
struct SweepParts {
    left: u32,
    right: u32,
    left_inv: u32,
    right_inv: u32,
}

#[inline(always)]
fn compute_sweep_parts(
    patterns: &HashMap<PatternVariant, u32>,
    left: PatternVariant,
    right: PatternVariant,
    left_inv: PatternVariant,
    right_inv: PatternVariant,
) -> SweepParts {
    SweepParts {
        left: *patterns.get(&left).unwrap_or(&0),
        right: *patterns.get(&right).unwrap_or(&0),
        left_inv: *patterns.get(&left_inv).unwrap_or(&0),
        right_inv: *patterns.get(&right_inv).unwrap_or(&0),
    }
}

#[derive(Clone, Copy)]
struct TowerParts {
    lr: u32,
    ud: u32,
    ld: u32,
    lu: u32,
    rd: u32,
    ru: u32,
}

#[inline(always)]
fn compute_tower_parts(patterns: &HashMap<PatternVariant, u32>) -> TowerParts {
    TowerParts {
        lr: *patterns.get(&PatternVariant::TowerLR).unwrap_or(&0),
        ud: *patterns.get(&PatternVariant::TowerUD).unwrap_or(&0),
        ld: *patterns.get(&PatternVariant::TowerCornerLD).unwrap_or(&0),
        lu: *patterns.get(&PatternVariant::TowerCornerLU).unwrap_or(&0),
        rd: *patterns.get(&PatternVariant::TowerCornerRD).unwrap_or(&0),
        ru: *patterns.get(&PatternVariant::TowerCornerRU).unwrap_or(&0),
    }
}

#[derive(Clone, Copy)]
struct TriangleParts {
    ldl: u32,
    lul: u32,
    rdr: u32,
    rur: u32,
}

#[inline(always)]
fn compute_triangle_parts(patterns: &HashMap<PatternVariant, u32>) -> TriangleParts {
    TriangleParts {
        ldl: *patterns.get(&PatternVariant::TriangleLDL).unwrap_or(&0),
        lul: *patterns.get(&PatternVariant::TriangleLUL).unwrap_or(&0),
        rdr: *patterns.get(&PatternVariant::TriangleRDR).unwrap_or(&0),
        rur: *patterns.get(&PatternVariant::TriangleRUR).unwrap_or(&0),
    }
}

#[derive(Clone, Copy)]
struct SimpleQuadParts {
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

#[inline(always)]
fn compute_simple_quad_parts(
    patterns: &HashMap<PatternVariant, u32>,
    a: PatternVariant,
    b: PatternVariant,
    c: PatternVariant,
    d: PatternVariant,
) -> SimpleQuadParts {
    SimpleQuadParts {
        a: *patterns.get(&a).unwrap_or(&0),
        b: *patterns.get(&b).unwrap_or(&0),
        c: *patterns.get(&c).unwrap_or(&0),
        d: *patterns.get(&d).unwrap_or(&0),
    }
}

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
    /// Mines that are actually judgable (not inside warps or #FAKES).
    pub mines_nonfake:     u32,
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
    pub row_to_beat:       Vec<f32>,
    pub timing_segments:   TimingSegments,
    pub minimized_note_data: Vec<u8>,
    pub chart_stops:       Option<String>,
    pub chart_speeds:      Option<String>,
    pub chart_scrolls:     Option<String>,
    pub chart_bpms:        Option<String>,
    pub chart_delays:      Option<String>,
    pub chart_warps:       Option<String>,
    pub chart_fakes:       Option<String>,
    pub chart_time_signatures: Option<String>,
    pub chart_labels:      Option<String>,
    pub chart_tickcounts:  Option<String>,
    pub chart_combos:      Option<String>,
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
    pub normalized_fakes:     String,
    pub normalized_time_signatures: String,
    pub normalized_labels:    String,
    pub normalized_tickcounts: String,
    pub normalized_combos:    String,
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

fn chart_or_global<'a>(chart_value: &'a Option<String>, global_value: &'a str) -> Option<&'a str> {
    if let Some(s) = chart_value {
        if !s.is_empty() {
            return Some(s.as_str());
        }
    }
    if !global_value.is_empty() {
        Some(global_value)
    } else {
        None
    }
}

fn has_zero_beat(beat: f64) -> bool {
    beat.abs() <= 1e-6
}

fn parse_time_signatures(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let mut out = Vec::new();
    let Some(s) = opt else {
        out.push((0.0, 4, 4));
        return out;
    };

    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else { continue };
        let Some(num_str) = parts.next() else { continue };
        let Some(den_str) = parts.next() else { continue };
        let Ok(beat) = beat_str.trim().parse::<f64>() else { continue };
        let Ok(num) = num_str.trim().parse::<i32>() else { continue };
        let Ok(den) = den_str.trim().parse::<i32>() else { continue };
        out.push((beat, num, den));
    }

    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    if out.is_empty() {
        out.push((0.0, 4, 4));
    } else if !out.iter().any(|(beat, _, _)| has_zero_beat(*beat)) {
        out.insert(0, (0.0, 4, 4));
    }
    out
}

fn parse_tickcounts(opt: Option<&str>) -> Vec<(f64, i32)> {
    let mut out = Vec::new();
    let Some(s) = opt else {
        out.push((0.0, 4));
        return out;
    };

    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else { continue };
        let Some(count_str) = parts.next() else { continue };
        let Ok(beat) = beat_str.trim().parse::<f64>() else { continue };
        let Ok(count) = count_str.trim().parse::<i32>() else { continue };
        out.push((beat, count));
    }

    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    if out.is_empty() {
        out.push((0.0, 4));
    } else if !out.iter().any(|(beat, _)| has_zero_beat(*beat)) {
        out.insert(0, (0.0, 4));
    }
    out
}

fn parse_combos(opt: Option<&str>) -> Vec<(f64, i32, i32)> {
    let mut out = Vec::new();
    let Some(s) = opt else {
        out.push((0.0, 1, 1));
        return out;
    };

    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut parts = segment.split('=');
        let Some(beat_str) = parts.next() else { continue };
        let Some(combo_str) = parts.next() else { continue };
        let Some(miss_str) = parts.next() else { continue };
        let Ok(beat) = beat_str.trim().parse::<f64>() else { continue };
        let Ok(combo) = combo_str.trim().parse::<i32>() else { continue };
        let Ok(miss) = miss_str.trim().parse::<i32>() else { continue };
        out.push((beat, combo, miss));
    }

    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    if out.is_empty() {
        out.push((0.0, 1, 1));
    } else if !out.iter().any(|(beat, _, _)| has_zero_beat(*beat)) {
        out.insert(0, (0.0, 1, 1));
    }
    out
}

fn parse_labels(opt: Option<&str>) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    let Some(s) = opt else {
        out.push((0.0, "Song Start".to_string()));
        return out;
    };

    for segment in s.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let Some((beat_str, label_raw)) = segment.split_once('=') else {
            continue;
        };
        let Ok(beat) = beat_str.trim().parse::<f64>() else {
            continue;
        };
        let label = label_raw.trim().to_string();
        if label.is_empty() {
            continue;
        }
        out.push((beat, label));
    }

    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    if out.is_empty() {
        out.push((0.0, "Song Start".to_string()));
    } else if !out.iter().any(|(beat, _)| has_zero_beat(*beat)) {
        out.insert(0, (0.0, "Song Start".to_string()));
    }
    out
}

fn count_timing_segments_from_str(s: &str) -> u32 {
    s.split(',')
        .filter(|part| !part.trim().is_empty())
        .count() as u32
}

fn count_timing_segments(opt: Option<&str>) -> u32 {
    opt.map_or(0, count_timing_segments_from_str)
}

fn count_gimmick_speed_segments(opt: Option<&str>) -> u32 {
    let Some(s) = opt else {
        return 0;
    };

    s.split(',')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let mut parts = segment.split('=');
            let _beat = parts.next();
            let factor_str = parts.next()?;
            let factor = factor_str.trim().parse::<f64>().ok()?;

            if (factor - 1.0).abs() > 1e-6 {
                Some(())
            } else {
                None
            }
        })
        .count() as u32
}

fn count_gimmick_scroll_segments(opt: Option<&str>) -> u32 {
    let Some(s) = opt else {
        return 0;
    };

    s.split(',')
        .filter_map(|segment| {
            let segment = segment.trim();
            if segment.is_empty() {
                return None;
            }

            let mut parts = segment.split('=');
            let _beat = parts.next();
            let value_str = parts.next()?;
            let value = value_str.trim().parse::<f64>().ok()?;

            if (value - 1.0).abs() > 1e-6 {
                Some(())
            } else {
                None
            }
        })
        .count() as u32
}

fn print_gimmicks(chart: &ChartSummary, simfile: &SimfileSummary) {
    let has_lifts = chart.stats.lifts > 0;
    let has_fakes = chart.stats.fakes > 0;
    let stops = chart_or_global(&chart.chart_stops, &simfile.normalized_stops);
    let delays = chart_or_global(&chart.chart_delays, &simfile.normalized_delays);
    let warps = chart_or_global(&chart.chart_warps, &simfile.normalized_warps);
    let speeds = chart_or_global(&chart.chart_speeds, &simfile.normalized_speeds);
    let scrolls = chart_or_global(&chart.chart_scrolls, &simfile.normalized_scrolls);

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    if !has_lifts
        && !has_fakes
        && stop_count == 0
        && delay_count == 0
        && warp_count == 0
        && speed_count == 0
        && scroll_count == 0
    {
        return;
    }

    println!("\n--- Gimmicks ---");
    if has_lifts {
        println!("Lifts: {}", chart.stats.lifts);
    }
    if has_fakes {
        println!("Fakes: {}", chart.stats.fakes);
    }
    if stop_count > 0 {
        println!("Stops/Freezes: {}", stop_count);
    }
    if speed_count > 0 {
        println!("Speeds: {}", speed_count);
    }
    if scroll_count > 0 {
        println!("Scrolls: {}", scroll_count);
    }
    if delay_count > 0 {
        println!("Delays: {}", delay_count);
    }
    if warp_count > 0 {
        println!("Warps: {}", warp_count);
    }
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
        print_pretty_chart(chart, simfile);
    }
}

fn print_pretty_chart(chart: &ChartSummary, simfile: &SimfileSummary) {
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
    let (stream_percent, adjusted_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    println!(
        "Total Stream: {} ({:.2}%/{:.2}% Adj.)",
        total_stream, stream_percent, adjusted_stream_percent
    );
    println!("Total Break: {} ({:.2}%)", total_break, break_percent);

    println!("\n--- Chart Info ---");
    println!("Steps: {} ({} arrows)", chart.stats.total_steps, chart.stats.total_arrows);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Hands: {}", chart.stats.hands);
    println!("Holds: {}", chart.stats.holds);
    println!("Rolls: {}", chart.stats.rolls);
    println!("Mines: {}", chart.stats.mines);

    print_gimmicks(chart, simfile);

    println!("\n--- Pattern Analysis ---");
    let candle_left = chart.detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = chart.detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    println!("Candles: {} ({} left, {} right)",
        candle_left + candle_right, candle_left, candle_right);
    println!("Candle%: {:.2}%", chart.candle_percent);
    println!("Mono: {} ({} left-facing, {} right-facing)", chart.mono_total, chart.facing_left, chart.facing_right);
    println!("Mono%: {:.2}%", chart.mono_percent);

    let box_parts = compute_box_parts(&chart.detected_patterns);
    let box_corners = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
    println!("Boxes: {} ({} LRLR, {} UDUD, {} corner)",
        box_parts.lr + box_parts.ud + box_corners, box_parts.lr, box_parts.ud, box_corners);

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
        print_full_chart(chart, simfile);
    }
    println!("\nElapsed Time: {:?}", simfile.total_elapsed);
}

fn print_full_chart(chart: &ChartSummary, simfile: &SimfileSummary) {
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
    let (stream_percent, adjusted_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    println!(
        "Total Stream: {} ({:.2}%/{:.2}% Adj.)",
        total_stream, stream_percent, adjusted_stream_percent
    );
    println!("    16th_streams: {}", chart.stream_counts.run16_streams);
    println!("    20th_streams: {}", chart.stream_counts.run20_streams);
    println!("    24th_streams: {}", chart.stream_counts.run24_streams);
    println!("    32nd_streams: {}", chart.stream_counts.run32_streams);
    println!("Total Break: {} ({:.2}%)", total_break, break_percent);

    println!("\n--- Chart Info ---");
    println!("Steps: {} ({} arrows) [{} left, {} down, {} up, {} right]", chart.stats.total_steps, chart.stats.total_arrows,chart.stats.left, chart.stats.down, chart.stats.up, chart.stats.right);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Hands: {}", chart.stats.hands);
    println!("Holds: {}", chart.stats.holds);
    println!("Rolls: {}", chart.stats.rolls);
    println!("Mines: {}", chart.stats.mines);

    print_gimmicks(chart, simfile);

    println!("\n--- Pattern Analysis ---");
    let candle_left = chart.detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = chart.detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    println!("Candles: {} ({} left, {} right)",
        candle_left + candle_right, candle_left, candle_right);
    println!("Candle%: {:.2}%", chart.candle_percent);
    println!("Mono: {} ({} left-facing, {} right-facing)", chart.mono_total, chart.facing_left, chart.facing_right);
    println!("Mono%: {:.2}%", chart.mono_percent);

    let box_parts = compute_box_parts(&chart.detected_patterns);
    let box_corners = box_parts.lr
        + box_parts.ud
        + box_parts.ld
        + box_parts.lu
        + box_parts.rd
        + box_parts.ru;
    println!("Boxes: {} ({} LRLR, {} UDUD, {} LDLD, {} LULU, {} RDRD, {} RURU)",
        box_parts.lr + box_parts.ud + box_corners,
        box_parts.lr,
        box_parts.ud,
        box_parts.ld,
        box_parts.lu,
        box_parts.rd,
        box_parts.ru);

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
    let tower_parts = compute_tower_parts(&chart.detected_patterns);
    let corner_towers =
        tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
    let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
    println!(
        "Total Towers: {} ({} LR, {} UD, {} LD, {} LU, {} RD, {} RU)",
        total_towers,
        tower_parts.lr,
        tower_parts.ud,
        tower_parts.ld,
        tower_parts.lu,
        tower_parts.rd,
        tower_parts.ru
    );

    // Triangles
    let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
    let total_triangles =
        triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
    println!(
        "Total Triangles: {} ({} LDL, {} LUL, {} RDR, {} RUR)",
        total_triangles, triangle_parts.ldl, triangle_parts.lul, triangle_parts.rdr,
        triangle_parts.rur
    );

    // Staircases
    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let total_staircases =
        stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
    println!(
        "Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_staircases, stairs.left, stairs.right, stairs.left_inv, stairs.right_inv
    );

    // Alternate Staircases
    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let total_alt = alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;
    println!(
        "Alt Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_alt, alt_stairs.left, alt_stairs.right, alt_stairs.left_inv, alt_stairs.right_inv
    );

    // Double Staircases
    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    let total_double = double_stairs.left
        + double_stairs.right
        + double_stairs.left_inv
        + double_stairs.right_inv;
    println!(
        "Double Staircases: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_double,
        double_stairs.left,
        double_stairs.right,
        double_stairs.left_inv,
        double_stairs.right_inv
    );

    // Sweeps
    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    let total_sweeps =
        sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
    println!(
        "Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_sweeps, sweeps.left, sweeps.right, sweeps.left_inv, sweeps.right_inv
    );

    // Candle Sweeps
    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    let total_candle_sweeps = candle_sweeps.left
        + candle_sweeps.right
        + candle_sweeps.left_inv
        + candle_sweeps.right_inv;
    println!(
        "Candle Sweeps: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_candle_sweeps,
        candle_sweeps.left,
        candle_sweeps.right,
        candle_sweeps.left_inv,
        candle_sweeps.right_inv
    );

    // Copters
    let copters = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::CopterLeft,
        PatternVariant::CopterRight,
        PatternVariant::CopterInvLeft,
        PatternVariant::CopterInvRight,
    );
    let total_copters = copters.a + copters.b + copters.c + copters.d;
    println!(
        "Copters: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_copters, copters.a, copters.b, copters.c, copters.d
    );

    // Spirals
    let spirals = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::SpiralLeft,
        PatternVariant::SpiralRight,
        PatternVariant::SpiralInvLeft,
        PatternVariant::SpiralInvRight,
    );
    let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
    println!(
        "Spirals: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_spirals, spirals.a, spirals.b, spirals.c, spirals.d
    );

    // Turbo Candles
    let turbo_candles = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::TurboCandleLeft,
        PatternVariant::TurboCandleRight,
        PatternVariant::TurboCandleInvLeft,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles =
        turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
    println!(
        "Turbo Candles: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_turbo_candles,
        turbo_candles.a,
        turbo_candles.b,
        turbo_candles.c,
        turbo_candles.d
    );

    // Hip Breakers
    let hip_breakers = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::HipBreakerLeft,
        PatternVariant::HipBreakerRight,
        PatternVariant::HipBreakerInvLeft,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers =
        hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
    println!(
        "Hip Breakers: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_hip_breakers, hip_breakers.a, hip_breakers.b, hip_breakers.c, hip_breakers.d
    );

    // Doritos
    let doritos = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::DoritoLeft,
        PatternVariant::DoritoRight,
        PatternVariant::DoritoInvLeft,
        PatternVariant::DoritoInvRight,
    );
    let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
    println!(
        "Doritos: {} ({} Left, {} Right, {} Left Inv, {} Right Inv)",
        total_doritos, doritos.a, doritos.b, doritos.c, doritos.d
    );

    // Luchis
    let luchis = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::LuchiLeftDU,
        PatternVariant::LuchiLeftUD,
        PatternVariant::LuchiRightDU,
        PatternVariant::LuchiRightUD,
    );
    let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
    println!(
        "Luchis: {} ({} Left DU, {} Left UD, {} Right DU, {} Right UD)",
        total_luchis, luchis.a, luchis.b, luchis.c, luchis.d
    );

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
    })
}

fn json_stream_info(chart: &ChartSummary) -> JsonValue {
    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let total_measures = chart.total_measures;

    let (stream_percent, adj_stream_percent, break_percent) =
        compute_stream_percentages(total_stream, total_break, total_measures);

    serde_json::json!({
        "total_streams": total_stream,
        "16th_streams": chart.stream_counts.run16_streams,
        "20th_streams": chart.stream_counts.run20_streams,
        "24th_streams": chart.stream_counts.run24_streams,
        "32nd_streams": chart.stream_counts.run32_streams,
        "total_breaks": total_break,
        "stream_percent": stream_percent,
        "adj_stream_percent": adj_stream_percent,
        "break_percent": break_percent,
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

fn json_gimmicks(chart: &ChartSummary, simfile: &SimfileSummary) -> JsonValue {
    let lifts = chart.stats.lifts;
    let fakes = chart.stats.fakes;
    let stops = chart_or_global(&chart.chart_stops, &simfile.normalized_stops);
    let delays = chart_or_global(&chart.chart_delays, &simfile.normalized_delays);
    let warps = chart_or_global(&chart.chart_warps, &simfile.normalized_warps);
    let speeds = chart_or_global(&chart.chart_speeds, &simfile.normalized_speeds);
    let scrolls = chart_or_global(&chart.chart_scrolls, &simfile.normalized_scrolls);

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    let mut obj = JsonMap::new();

    obj.insert("lifts".to_string(), JsonValue::from(lifts));
    obj.insert("fakes".to_string(), JsonValue::from(fakes));
    obj.insert("stops_freezes".to_string(), JsonValue::from(stop_count));
    obj.insert("speeds".to_string(), JsonValue::from(speed_count));
    obj.insert("scrolls".to_string(), JsonValue::from(scroll_count));
    obj.insert("delays".to_string(), JsonValue::from(delay_count));
    obj.insert("warps".to_string(), JsonValue::from(warp_count));

    JsonValue::Object(obj)
}

fn json_timing(chart: &ChartSummary, simfile: &SimfileSummary) -> JsonValue {
    let timing = TimingData::from_chart_data(
        simfile.offset,
        0.0,
        chart.chart_bpms.as_deref(),
        &simfile.normalized_bpms,
        chart.chart_stops.as_deref(),
        &simfile.normalized_stops,
        chart.chart_delays.as_deref(),
        &simfile.normalized_delays,
        chart.chart_warps.as_deref(),
        &simfile.normalized_warps,
        chart.chart_speeds.as_deref(),
        &simfile.normalized_speeds,
        chart.chart_scrolls.as_deref(),
        &simfile.normalized_scrolls,
        chart.chart_fakes.as_deref(),
        &simfile.normalized_fakes,
    );

    let bpms: Vec<JsonValue> = timing
        .bpm_segments()
        .into_iter()
        .map(|(beat, bpm)| serde_json::json!([beat, bpm]))
        .collect();
    let stops: Vec<JsonValue> = timing
        .stops()
        .iter()
        .map(|seg| serde_json::json!([seg.beat, seg.duration]))
        .collect();
    let delays: Vec<JsonValue> = timing
        .delays()
        .iter()
        .map(|seg| serde_json::json!([seg.beat, seg.duration]))
        .collect();
    let warps: Vec<JsonValue> = timing
        .warps()
        .iter()
        .map(|seg| serde_json::json!([seg.beat, seg.length]))
        .collect();
    let speeds: Vec<JsonValue> = timing
        .speeds()
        .iter()
        .map(|seg| {
            let unit = if seg.unit == SpeedUnit::Seconds { 1 } else { 0 };
            serde_json::json!([seg.beat, seg.ratio, seg.delay, unit])
        })
        .collect();
    let scrolls: Vec<JsonValue> = timing
        .scrolls()
        .iter()
        .map(|seg| serde_json::json!([seg.beat, seg.ratio]))
        .collect();
    let fakes: Vec<JsonValue> = timing
        .fakes()
        .iter()
        .map(|seg| serde_json::json!([seg.beat, seg.length]))
        .collect();

    let time_signatures = parse_time_signatures(chart_or_global(
        &chart.chart_time_signatures,
        &simfile.normalized_time_signatures,
    ));
    let labels = parse_labels(chart_or_global(
        &chart.chart_labels,
        &simfile.normalized_labels,
    ));
    let tickcounts = parse_tickcounts(chart_or_global(
        &chart.chart_tickcounts,
        &simfile.normalized_tickcounts,
    ));
    let combos = parse_combos(chart_or_global(
        &chart.chart_combos,
        &simfile.normalized_combos,
    ));

    serde_json::json!({
        "beat0_offset_seconds": timing.beat0_offset_seconds(),
        "beat0_group_offset_seconds": timing.beat0_group_offset_seconds(),
        "bpms": bpms,
        "stops": stops,
        "delays": delays,
        "time_signatures": time_signatures
            .into_iter()
            .map(|(beat, num, den)| serde_json::json!([beat, num, den]))
            .collect::<Vec<_>>(),
        "warps": warps,
        "labels": labels
            .into_iter()
            .map(|(beat, label)| serde_json::json!([beat, label]))
            .collect::<Vec<_>>(),
        "tickcounts": tickcounts
            .into_iter()
            .map(|(beat, count)| serde_json::json!([beat, count]))
            .collect::<Vec<_>>(),
        "combos": combos
            .into_iter()
            .map(|(beat, combo, miss)| serde_json::json!([beat, combo, miss]))
            .collect::<Vec<_>>(),
        "speeds": speeds,
        "scrolls": scrolls,
        "fakes": fakes,
    })
}

fn json_pattern_counts(chart: &ChartSummary) -> JsonValue {
    let mut obj = JsonMap::new();

    // Boxes
    let box_parts = compute_box_parts(&chart.detected_patterns);
    let corner_boxes = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
    let total_boxes = box_parts.lr + box_parts.ud + corner_boxes;
    obj.insert(
        "boxes".to_string(),
        serde_json::json!({
            "total_boxes": total_boxes,
            "lr_boxes": box_parts.lr,
            "ud_boxes": box_parts.ud,
            "corner_boxes": corner_boxes,
            "ld_boxes": box_parts.ld,
            "lu_boxes": box_parts.lu,
            "rd_boxes": box_parts.rd,
            "ru_boxes": box_parts.ru,
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
    let tower_parts = compute_tower_parts(&chart.detected_patterns);
    let corner_towers = tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
    let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
    obj.insert(
        "towers".to_string(),
        serde_json::json!({
            "total_towers": total_towers,
            "lr_towers": tower_parts.lr,
            "ud_towers": tower_parts.ud,
            "corner_towers": corner_towers,
            "ld_towers": tower_parts.ld,
            "lu_towers": tower_parts.lu,
            "rd_towers": tower_parts.rd,
            "ru_towers": tower_parts.ru,
        }),
    );

    // Triangles
    let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
    let total_triangles =
        triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
    obj.insert(
        "triangles".to_string(),
        serde_json::json!({
            "total_triangles": total_triangles,
            "ldl_triangles": triangle_parts.ldl,
            "lul_triangles": triangle_parts.lul,
            "rdr_triangles": triangle_parts.rdr,
            "rur_triangles": triangle_parts.rur,
        }),
    );

    // Staircases
    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let total_staircases =
        stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let total_alt = alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;
    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    let total_double = double_stairs.left
        + double_stairs.right
        + double_stairs.left_inv
        + double_stairs.right_inv;
    obj.insert(
        "staircases".to_string(),
        serde_json::json!({
            "total_staircases": total_staircases,
            "left_staircases": stairs.left,
            "right_staircases": stairs.right,
            "left_inv_staircases": stairs.left_inv,
            "right_inv_staircases": stairs.right_inv,
            "total_alt_staircases": total_alt,
            "left_alt_staircases": alt_stairs.left,
            "right_alt_staircases": alt_stairs.right,
            "left_inv_alt_staircases": alt_stairs.left_inv,
            "right_inv_alt_staircases": alt_stairs.right_inv,
            "total_double_staircases": total_double,
            "left_double_staircases": double_stairs.left,
            "right_double_staircases": double_stairs.right,
            "left_inv_double_staircases": double_stairs.left_inv,
            "right_inv_double_staircases": double_stairs.right_inv,
        }),
    );

    // Sweeps
    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    let total_sweeps = sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
    obj.insert(
        "sweeps".to_string(),
        serde_json::json!({
            "total_sweeps": total_sweeps,
            "left_sweeps": sweeps.left,
            "right_sweeps": sweeps.right,
            "left_inv_sweeps": sweeps.left_inv,
            "right_inv_sweeps": sweeps.right_inv,
        }),
    );

    // Candle Sweeps
    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    let total_candle_sweeps = candle_sweeps.left
        + candle_sweeps.right
        + candle_sweeps.left_inv
        + candle_sweeps.right_inv;
    obj.insert(
        "candle_sweeps".to_string(),
        serde_json::json!({
            "total_candle_sweeps": total_candle_sweeps,
            "left_candle_sweeps": candle_sweeps.left,
            "right_candle_sweeps": candle_sweeps.right,
            "left_inv_candle_sweeps": candle_sweeps.left_inv,
            "right_inv_candle_sweeps": candle_sweeps.right_inv,
        }),
    );

    // Copters
    let copters = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::CopterLeft,
        PatternVariant::CopterRight,
        PatternVariant::CopterInvLeft,
        PatternVariant::CopterInvRight,
    );
    let total_copters = copters.a + copters.b + copters.c + copters.d;
    obj.insert(
        "copters".to_string(),
        serde_json::json!({
            "total_copters": total_copters,
            "left_copters": copters.a,
            "right_copters": copters.b,
            "left_inv_copters": copters.c,
            "right_inv_copters": copters.d,
        }),
    );

    // Spirals
    let spirals = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::SpiralLeft,
        PatternVariant::SpiralRight,
        PatternVariant::SpiralInvLeft,
        PatternVariant::SpiralInvRight,
    );
    let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
    obj.insert(
        "spirals".to_string(),
        serde_json::json!({
            "total_spirals": total_spirals,
            "left_spirals": spirals.a,
            "right_spirals": spirals.b,
            "left_inv_spirals": spirals.c,
            "right_inv_spirals": spirals.d,
        }),
    );

    // Turbo Candles
    let turbo_candles = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::TurboCandleLeft,
        PatternVariant::TurboCandleRight,
        PatternVariant::TurboCandleInvLeft,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles =
        turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
    obj.insert(
        "turbo_candles".to_string(),
        serde_json::json!({
            "total_turbo_candles": total_turbo_candles,
            "left_turbo_candles": turbo_candles.a,
            "right_turbo_candles": turbo_candles.b,
            "left_inv_turbo_candles": turbo_candles.c,
            "right_inv_turbo_candles": turbo_candles.d,
        }),
    );

    // Hip Breakers
    let hip_breakers = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::HipBreakerLeft,
        PatternVariant::HipBreakerRight,
        PatternVariant::HipBreakerInvLeft,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers =
        hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
    obj.insert(
        "hip_breakers".to_string(),
        serde_json::json!({
            "total_hip_breakers": total_hip_breakers,
            "left_hip_breakers": hip_breakers.a,
            "right_hip_breakers": hip_breakers.b,
            "left_inv_hip_breakers": hip_breakers.c,
            "right_inv_hip_breakers": hip_breakers.d,
        }),
    );

    // Doritos
    let doritos = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::DoritoLeft,
        PatternVariant::DoritoRight,
        PatternVariant::DoritoInvLeft,
        PatternVariant::DoritoInvRight,
    );
    let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
    obj.insert(
        "doritos".to_string(),
        serde_json::json!({
            "total_doritos": total_doritos,
            "left_doritos": doritos.a,
            "right_doritos": doritos.b,
            "left_inv_doritos": doritos.c,
            "right_inv_doritos": doritos.d,
        }),
    );

    // Luchis
    let luchis = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::LuchiLeftDU,
        PatternVariant::LuchiLeftUD,
        PatternVariant::LuchiRightDU,
        PatternVariant::LuchiRightUD,
    );
    let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
    obj.insert(
        "luchis".to_string(),
        serde_json::json!({
            "total_luchis": total_luchis,
            "left_du_luchis": luchis.a,
            "left_ud_luchis": luchis.b,
            "right_du_luchis": luchis.c,
            "right_ud_luchis": luchis.d,
        }),
    );

    // Custom patterns
    if !chart.custom_patterns.is_empty() {
        let mut custom = JsonMap::new();
        for cp in &chart.custom_patterns {
            custom.insert(cp.pattern.clone(), JsonValue::from(cp.count));
        }
        obj.insert("custom_patterns".to_string(), JsonValue::Object(custom));
    }

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
            let mut chart_obj = JsonMap::new();

            chart_obj.insert("chart_info".to_string(), json_chart_info(chart));
            chart_obj.insert("arrow_stats".to_string(), json_arrow_stats(chart));
            chart_obj.insert("gimmicks".to_string(), json_gimmicks(chart, simfile));
            chart_obj.insert("timing".to_string(), json_timing(chart, simfile));
            chart_obj.insert("stream_info".to_string(), json_stream_info(chart));
            chart_obj.insert("nps".to_string(), json_nps(chart));
            chart_obj.insert("breakdown".to_string(), json_breakdown(chart));
            chart_obj.insert(
                "mono_candle_stats".to_string(),
                json_mono_candle_stats(chart),
            );
            chart_obj.insert("pattern_counts".to_string(), json_pattern_counts(chart));
            chart_obj.insert("tech_counts".to_string(), json_tech_counts(chart));

            JsonValue::Object(chart_obj)
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
total_steps,jumps,hands,holds,rolls,mines,lifts,fakes,stops_freezes,delays,warps,speeds,scrolls,\
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

    let stops = chart_or_global(&chart.chart_stops, &simfile.normalized_stops);
    let delays = chart_or_global(&chart.chart_delays, &simfile.normalized_delays);
    let warps = chart_or_global(&chart.chart_warps, &simfile.normalized_warps);
    let speeds = chart_or_global(&chart.chart_speeds, &simfile.normalized_speeds);
    let scrolls = chart_or_global(&chart.chart_scrolls, &simfile.normalized_scrolls);

    let stop_count = count_timing_segments(stops);
    let delay_count = count_timing_segments(delays);
    let warp_count = count_timing_segments(warps);
    let speed_count = count_gimmick_speed_segments(speeds);
    let scroll_count = count_gimmick_scroll_segments(scrolls);

    print!("{},{},{},{},{},",
        stop_count,
        delay_count,
        warp_count,
        speed_count,
        scroll_count,
    );

    let total_streams = chart.total_streams;
    let total_breaks = chart.stream_counts.total_breaks;
    let (_stream_percent, adj_stream_percent, _break_percent) =
        compute_stream_percentages(total_streams, total_breaks, chart.total_measures);
    print!("{},{},{},{},{},{},{},",
        total_streams,
        chart.stream_counts.run16_streams,
        chart.stream_counts.run20_streams,
        chart.stream_counts.run24_streams,
        chart.stream_counts.run32_streams,
        total_breaks,
        adj_stream_percent,
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

    let box_parts = compute_box_parts(&chart.detected_patterns);
    let corner_boxes = box_parts.ld + box_parts.lu + box_parts.rd + box_parts.ru;
    let total_boxes = box_parts.lr + box_parts.ud + corner_boxes;
    print!("{},{},{},{},{},{},{},{},",
        total_boxes,
        box_parts.lr,
        box_parts.ud,
        corner_boxes,
        box_parts.ld,
        box_parts.lu,
        box_parts.rd,
        box_parts.ru,
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

    let tower_parts = compute_tower_parts(&chart.detected_patterns);
    let corner_towers =
        tower_parts.ld + tower_parts.lu + tower_parts.rd + tower_parts.ru;
    let total_towers = tower_parts.lr + tower_parts.ud + corner_towers;
    print!("{},{},{},{},{},{},{},{},",
        total_towers,
        tower_parts.lr,
        tower_parts.ud,
        corner_towers,
        tower_parts.ld,
        tower_parts.lu,
        tower_parts.rd,
        tower_parts.ru,
    );

    let triangle_parts = compute_triangle_parts(&chart.detected_patterns);
    let total_triangles =
        triangle_parts.ldl + triangle_parts.lul + triangle_parts.rdr + triangle_parts.rur;
    print!("{},{},{},{},{},",
        total_triangles,
        triangle_parts.ldl,
        triangle_parts.lul,
        triangle_parts.rdr,
        triangle_parts.rur,
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

    let stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::StaircaseLeft,
        PatternVariant::StaircaseRight,
        PatternVariant::StaircaseInvLeft,
        PatternVariant::StaircaseInvRight,
    );
    let total_staircases =
        stairs.left + stairs.right + stairs.left_inv + stairs.right_inv;
    print!("{},{},{},{},{},",
        total_staircases,
        stairs.left,
        stairs.right,
        stairs.left_inv,
        stairs.right_inv,
    );

    let alt_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::AltStaircasesLeft,
        PatternVariant::AltStaircasesRight,
        PatternVariant::AltStaircasesInvLeft,
        PatternVariant::AltStaircasesInvRight,
    );
    let total_alt =
        alt_stairs.left + alt_stairs.right + alt_stairs.left_inv + alt_stairs.right_inv;

    let double_stairs = compute_stair_parts(
        &chart.detected_patterns,
        PatternVariant::DStaircaseLeft,
        PatternVariant::DStaircaseRight,
        PatternVariant::DStaircaseInvLeft,
        PatternVariant::DStaircaseInvRight,
    );
    let total_double = double_stairs.left
        + double_stairs.right
        + double_stairs.left_inv
        + double_stairs.right_inv;

    print!("{},{},{},{},{},{},{},{},{},{},",
        total_alt,
        alt_stairs.left,
        alt_stairs.right,
        alt_stairs.left_inv,
        alt_stairs.right_inv,
        total_double,
        double_stairs.left,
        double_stairs.right,
        double_stairs.left_inv,
        double_stairs.right_inv,
    );

    let sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepLeft,
        PatternVariant::SweepRight,
        PatternVariant::SweepInvLeft,
        PatternVariant::SweepInvRight,
    );
    let total_sweeps =
        sweeps.left + sweeps.right + sweeps.left_inv + sweeps.right_inv;
    print!("{},{},{},{},{},",
        total_sweeps,
        sweeps.left,
        sweeps.right,
        sweeps.left_inv,
        sweeps.right_inv,
    );

    let candle_sweeps = compute_sweep_parts(
        &chart.detected_patterns,
        PatternVariant::SweepCandleLeft,
        PatternVariant::SweepCandleRight,
        PatternVariant::SweepCandleInvLeft,
        PatternVariant::SweepCandleInvRight,
    );
    let total_candle_sweeps = candle_sweeps.left
        + candle_sweeps.right
        + candle_sweeps.left_inv
        + candle_sweeps.right_inv;
    print!("{},{},{},{},{},",
        total_candle_sweeps,
        candle_sweeps.left,
        candle_sweeps.right,
        candle_sweeps.left_inv,
        candle_sweeps.right_inv,
    );

    let copters = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::CopterLeft,
        PatternVariant::CopterRight,
        PatternVariant::CopterInvLeft,
        PatternVariant::CopterInvRight,
    );
    let total_copters = copters.a + copters.b + copters.c + copters.d;
    print!("{},{},{},{},{},",
        total_copters,
        copters.a,
        copters.b,
        copters.c,
        copters.d,
    );

    let spirals = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::SpiralLeft,
        PatternVariant::SpiralRight,
        PatternVariant::SpiralInvLeft,
        PatternVariant::SpiralInvRight,
    );
    let total_spirals = spirals.a + spirals.b + spirals.c + spirals.d;
    print!("{},{},{},{},{},",
        total_spirals,
        spirals.a,
        spirals.b,
        spirals.c,
        spirals.d,
    );

    let turbo_candles = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::TurboCandleLeft,
        PatternVariant::TurboCandleRight,
        PatternVariant::TurboCandleInvLeft,
        PatternVariant::TurboCandleInvRight,
    );
    let total_turbo_candles =
        turbo_candles.a + turbo_candles.b + turbo_candles.c + turbo_candles.d;
    print!("{},{},{},{},{},",
        total_turbo_candles,
        turbo_candles.a,
        turbo_candles.b,
        turbo_candles.c,
        turbo_candles.d,
    );

    let hip_breakers = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::HipBreakerLeft,
        PatternVariant::HipBreakerRight,
        PatternVariant::HipBreakerInvLeft,
        PatternVariant::HipBreakerInvRight,
    );
    let total_hip_breakers =
        hip_breakers.a + hip_breakers.b + hip_breakers.c + hip_breakers.d;
    print!("{},{},{},{},{},",
        total_hip_breakers,
        hip_breakers.a,
        hip_breakers.b,
        hip_breakers.c,
        hip_breakers.d,
    );

    let doritos = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::DoritoLeft,
        PatternVariant::DoritoRight,
        PatternVariant::DoritoInvLeft,
        PatternVariant::DoritoInvRight,
    );
    let total_doritos = doritos.a + doritos.b + doritos.c + doritos.d;
    print!("{},{},{},{},{},",
        total_doritos,
        doritos.a,
        doritos.b,
        doritos.c,
        doritos.d,
    );

    let luchis = compute_simple_quad_parts(
        &chart.detected_patterns,
        PatternVariant::LuchiLeftDU,
        PatternVariant::LuchiLeftUD,
        PatternVariant::LuchiRightDU,
        PatternVariant::LuchiRightUD,
    );
    let total_luchis = luchis.a + luchis.b + luchis.c + luchis.d;
    print!("{},{},{},{},{}",
        total_luchis,
        luchis.a,
        luchis.b,
        luchis.c,
        luchis.d,
    );

    for cp in &chart.custom_patterns {
        print!(",{}", cp.count);
    }

    println!();
}

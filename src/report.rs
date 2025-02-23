use std::collections::HashMap;
use std::time::Duration;

use crate::patterns::PatternVariant;
use crate::stats::{ArrowStats, StreamCounts};

pub struct ChartSummary {
    pub step_type_str:     String,
    pub step_artist_str:   String,
    pub difficulty_str:    String,
    pub rating_str:        String,
    pub tech_notation_str: String,

    pub stats:             ArrowStats,
    pub stream_counts:     StreamCounts,
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

    pub elapsed:           Duration,

    pub measure_densities: Vec<usize>,
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

static ALL_PATTERNS: &[PatternVariant] = &[
    PatternVariant::AltStaircasesLeft,
    PatternVariant::AltStaircasesRight,
    PatternVariant::AltStaircasesInvLeft,
    PatternVariant::AltStaircasesInvRight,
    PatternVariant::BoxLR,
    PatternVariant::BoxUD,
    PatternVariant::BoxCornerLD,
    PatternVariant::BoxCornerLU,
    PatternVariant::BoxCornerRD,
    PatternVariant::BoxCornerRU,
    PatternVariant::CandleLeft,
    PatternVariant::CandleRight,
    PatternVariant::CopterLeft,
    PatternVariant::CopterRight,
    PatternVariant::CopterInvLeft,
    PatternVariant::CopterInvRight,
    PatternVariant::DoritoLeft,
    PatternVariant::DoritoRight,
    PatternVariant::DoritoInvLeft,
    PatternVariant::DoritoInvRight,
    PatternVariant::DStaircaseLeft,
    PatternVariant::DStaircaseRight,
    PatternVariant::DStaircaseInvLeft,
    PatternVariant::DStaircaseInvRight,
    PatternVariant::HipBreakerLeft,
    PatternVariant::HipBreakerRight,
    PatternVariant::HipBreakerInvLeft,
    PatternVariant::HipBreakerInvRight,
    PatternVariant::LuchiLeftDU,
    PatternVariant::LuchiLeftUD,
    PatternVariant::LuchiRightUD,
    PatternVariant::LuchiRightDU,
    PatternVariant::SideswitchLeft,
    PatternVariant::SideswitchRight,
    PatternVariant::SpiralLeft,
    PatternVariant::SpiralRight,
    PatternVariant::SpiralInvLeft,
    PatternVariant::SpiralInvRight,
    PatternVariant::StaircaseLeft,
    PatternVariant::StaircaseRight,
    PatternVariant::StaircaseInvLeft,
    PatternVariant::StaircaseInvRight,
    PatternVariant::SweepCandleLeft,
    PatternVariant::SweepCandleRight,
    PatternVariant::SweepCandleInvLeft,
    PatternVariant::SweepCandleInvRight,
    PatternVariant::SweepLeft,
    PatternVariant::SweepRight,
    PatternVariant::SweepInvLeft,
    PatternVariant::SweepInvRight,
    PatternVariant::TowerLR,
    PatternVariant::TowerUD,
    PatternVariant::TowerCornerLD,
    PatternVariant::TowerCornerLU,
    PatternVariant::TowerCornerRD,
    PatternVariant::TowerCornerRU,
    PatternVariant::TriangleRUR,
    PatternVariant::TriangleLUL,
    PatternVariant::TriangleLDL,
    PatternVariant::TriangleRDR,
    PatternVariant::TurboCandleLeft,
    PatternVariant::TurboCandleRight,
    PatternVariant::TurboCandleInvLeft,
    PatternVariant::TurboCandleInvRight,
];

fn pattern_variant_name(pv: PatternVariant) -> &'static str {
    match pv {
        PatternVariant::AltStaircasesLeft => "alt_staircase_left",
        PatternVariant::AltStaircasesRight => "alt_staircase_right",
        PatternVariant::AltStaircasesInvLeft => "alt_staircase_inv_left",
        PatternVariant::AltStaircasesInvRight => "alt_staircase_inv_right",
        PatternVariant::BoxLR => "box_lr",
        PatternVariant::BoxUD => "box_ud",
        PatternVariant::BoxCornerLD => "box_corner_ld",
        PatternVariant::BoxCornerLU => "box_corner_lu",
        PatternVariant::BoxCornerRD => "box_corner_rd",
        PatternVariant::BoxCornerRU => "box_corner_ru",
        PatternVariant::CandleLeft => "candle_left",
        PatternVariant::CandleRight => "candle_right",
        PatternVariant::CopterLeft => "copter_left",
        PatternVariant::CopterRight => "copter_right",
        PatternVariant::CopterInvLeft => "copter_inv_left",
        PatternVariant::CopterInvRight => "copter_inv_right",
        PatternVariant::DoritoLeft => "dorito_left",
        PatternVariant::DoritoRight => "dorito_right",
        PatternVariant::DoritoInvLeft => "dorito_inv_left",
        PatternVariant::DoritoInvRight => "dorito_inv_right",
        PatternVariant::DStaircaseLeft => "double_staircase_left",
        PatternVariant::DStaircaseRight => "double_staircase_right",
        PatternVariant::DStaircaseInvLeft => "double_staircase_inv_left",
        PatternVariant::DStaircaseInvRight => "double_staircase_inv_right",
        PatternVariant::HipBreakerLeft => "hip_breaker_left",
        PatternVariant::HipBreakerRight => "hip_breaker_right",
        PatternVariant::HipBreakerInvLeft => "hip_breaker_inv_left",
        PatternVariant::HipBreakerInvRight => "hip_breaker_inv_right",
        PatternVariant::LuchiLeftDU => "luchi_left_du",
        PatternVariant::LuchiLeftUD => "luchi_left_ud",
        PatternVariant::LuchiRightUD => "luchi_right_ud",
        PatternVariant::LuchiRightDU => "luchi_right_du",
        PatternVariant::SideswitchLeft => "ss_left",
        PatternVariant::SideswitchRight => "ss_right",
        PatternVariant::SpiralLeft => "spiral_left",
        PatternVariant::SpiralRight => "spiral_right",
        PatternVariant::SpiralInvLeft => "spiral_inv_left",
        PatternVariant::SpiralInvRight => "spiral_inv_right",
        PatternVariant::StaircaseLeft => "staircase_left",
        PatternVariant::StaircaseRight => "staircase_right",
        PatternVariant::StaircaseInvLeft => "staircase_inv_left",
        PatternVariant::StaircaseInvRight => "staircase_inv_right",
        PatternVariant::SweepCandleLeft => "sweep_candle_left",
        PatternVariant::SweepCandleRight => "sweep_candle_right",
        PatternVariant::SweepCandleInvLeft => "sweep_candle_inv_left",
        PatternVariant::SweepCandleInvRight => "sweep_candle_inv_right",
        PatternVariant::SweepLeft => "sweep_left",
        PatternVariant::SweepRight => "sweep_right",
        PatternVariant::SweepInvLeft => "sweep_inv_left",
        PatternVariant::SweepInvRight => "sweep_inv_right",
        PatternVariant::TowerLR => "tower_lr",
        PatternVariant::TowerUD => "tower_ud",
        PatternVariant::TowerCornerLD => "tower_corner_ld",
        PatternVariant::TowerCornerLU => "tower_corner_lu",
        PatternVariant::TowerCornerRD => "tower_corner_rd",
        PatternVariant::TowerCornerRU => "tower_corner_ru",
        PatternVariant::TriangleRUR => "triangle_rur",
        PatternVariant::TriangleLUL => "triangle_lul",
        PatternVariant::TriangleLDL => "triangle_ldl",
        PatternVariant::TriangleRDR => "triangle_rdr",
        PatternVariant::TurboCandleLeft => "turbo_candle_left",
        PatternVariant::TurboCandleRight => "turbo_candle_right",
        PatternVariant::TurboCandleInvLeft => "turbo_candle_inv_left",
        PatternVariant::TurboCandleInvRight => "turbo_candle_inv_right",
    }
}

fn count(map: &HashMap<PatternVariant, u32>, v: PatternVariant) -> u32 {
    *map.get(&v).unwrap_or(&0)
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
    println!();

    for chart in &simfile.charts {
        print_pretty_chart(chart);
    }
}

fn print_pretty_chart(chart: &ChartSummary) {
    let header = format!("{} {} : {}", chart.difficulty_str, chart.rating_str, chart.step_artist_str);
    println!("\n{}", header);
    println!("{}", "-".repeat(header.len()));

    if (chart.median_nps - chart.max_nps).abs() < f64::EPSILON {
        println!("NPS: {:.2} Median/Peak", chart.median_nps);
    } else {
        println!("NPS: {:.2} Median, {:.2} Peak", chart.median_nps, chart.max_nps);
    }

    let total_stream = chart.total_streams;
    let total_break = chart.stream_counts.total_breaks;
    let stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else { 0.0 };
    println!("Total Stream: {} ({:.2}%)", total_stream, stream_percent);
    println!("Total Break: {} ({:.2}%)", total_break, 100.0 - stream_percent);

    println!("\n--- Chart Info ---");
    println!("Steps: {} ({} arrows)", chart.stats.total_steps, chart.stats.total_arrows);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Holds: {}", chart.stats.holds);
    println!("Mines: {}", chart.stats.mines);
    println!("Hands: {}", chart.stats.hands);
    println!("Rolls: {}", chart.stats.rolls);

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
    println!("Title: {}", simfile.title_str);
    println!("Title translate: {}", simfile.titletranslit_str);
    println!("Subtitle: {}", simfile.subtitle_str);
    println!("Subtitle translate: {}", simfile.subtitletranslit_str);
    println!("Artist: {}", simfile.artist_str);
    println!("Artist translate: {}", simfile.artisttranslit_str);
    println!("Offset: {:.3}", simfile.offset);
    println!("Normalized BPMs: {}", simfile.normalized_bpms);

    println!("Min BPM: {:.2}", simfile.min_bpm);
    println!("Max BPM: {:.2}", simfile.max_bpm);
    println!("Median BPM: {:.2}", simfile.median_bpm);
    println!("Average BPM: {:.2}", simfile.average_bpm);
    println!("Chart length (seconds): {}", simfile.total_length);

    println!("---\n");

    for chart in &simfile.charts {
        print_full_chart(chart);
        println!();
    }
    println!("Total Elapsed Time: {:?}", simfile.total_elapsed);
}

fn print_full_chart(chart: &ChartSummary) {
    println!("--- Chart Info ---");
    println!("Step type: {}", chart.step_type_str);
    println!("Difficulty: {}", chart.difficulty_str);
    println!("Rating: {}", chart.rating_str);
    println!("Step artist: {}", chart.step_artist_str);
    println!("Tech notation: {}", chart.tech_notation_str);
    println!("Hash (short): {}", chart.short_hash);

    println!("--- Arrow Stats ---");
    println!("Left: {}", chart.stats.left);
    println!("Down: {}", chart.stats.down);
    println!("Up: {}", chart.stats.up);
    println!("Right: {}", chart.stats.right);
    println!("Total arrows: {}", chart.stats.total_arrows);
    println!("Total steps: {}", chart.stats.total_steps);
    println!("Jumps: {}", chart.stats.jumps);
    println!("Hands: {}", chart.stats.hands);
    println!("Holds: {}", chart.stats.holds);
    println!("Rolls: {}", chart.stats.rolls);
    println!("Mines: {}", chart.stats.mines);

    println!("--- Stream Counts ---");
    println!("16th streams: {}", chart.stream_counts.run16_streams);
    println!("20th streams: {}", chart.stream_counts.run20_streams);
    println!("24th streams: {}", chart.stream_counts.run24_streams);
    println!("32nd streams: {}", chart.stream_counts.run32_streams);
    println!("Total streams: {}", chart.total_streams);
    println!("Total breaks: {}", chart.stream_counts.total_breaks);

    println!("--- Additional Stats ---");
    println!("Max NPS: {:.2}", chart.max_nps);
    println!("Median NPS: {:.2}", chart.median_nps);

    println!("--- Mono/Candle ---");
    println!("Facing left: {}", chart.facing_left);
    println!("Facing right: {}", chart.facing_right);
    println!("Mono total: {}", chart.mono_total);
    println!("Mono percentage: {:.2}%", chart.mono_percent);
    println!("Candle total: {}", chart.candle_total);
    println!("Candle percentage: {:.2}%", chart.candle_percent);

    println!("--- Anchors ---");
    println!("AnchorLeft: {}", chart.anchor_left);
    println!("AnchorDown: {}", chart.anchor_down);
    println!("AnchorUp: {}", chart.anchor_up);
    println!("AnchorRight: {}", chart.anchor_right);

    println!("--- Detected Patterns ---");
    for &pv in ALL_PATTERNS.iter() {
        let count = chart.detected_patterns.get(&pv).unwrap_or(&0);
        println!("{}: {}", pattern_variant_name(pv), count);
    }

    println!("--- Breakdowns ---");
    println!("Detailed: {}", chart.detailed);
    println!("Partial: {}", chart.partial);
    println!("Simple: {}", chart.simple);

    println!("--- Chart elapsed time: {:?}", chart.elapsed);
}

fn print_json_all(simfile: &SimfileSummary) {
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

    println!("{{");
    println!("  \"title\": \"{}\",", esc(&simfile.title_str));
    println!("  \"subtitle\": \"{}\",", esc(&simfile.subtitle_str));
    println!("  \"artist\": \"{}\",", esc(&simfile.artist_str));
    println!("  \"title_translit\": \"{}\",", esc(&simfile.titletranslit_str));
    println!("  \"subtitle_translit\": \"{}\",", esc(&simfile.subtitletranslit_str));
    println!("  \"artist_translit\": \"{}\",", esc(&simfile.artisttranslit_str));
    println!("  \"offset\": {:.3},", simfile.offset);
    println!("  \"normalized_bpms\": \"{}\",", esc(&simfile.normalized_bpms));
    println!("  \"min_bpm\": {:.2},", simfile.min_bpm);
    println!("  \"max_bpm\": {:.2},", simfile.max_bpm);
    println!("  \"median_bpm\": {:.2},", simfile.median_bpm);
    println!("  \"average_bpm\": {:.2},", simfile.average_bpm);
    println!("  \"total_length_s\": {},", simfile.total_length);

    println!("  \"charts\": [");
    for (i, chart) in simfile.charts.iter().enumerate() {
        print_json_chart(chart, i + 1 == simfile.charts.len());
    }
    println!("  ]");

    println!("}}");
}

fn print_json_chart(chart: &ChartSummary, is_last: bool) {
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

    println!("    {{");
    println!("      \"step_type\": \"{}\",", esc(&chart.step_type_str));
    println!("      \"difficulty\": \"{}\",", esc(&chart.difficulty_str));
    println!("      \"rating\": \"{}\",", esc(&chart.rating_str));
    println!("      \"step_artist\": \"{}\",", esc(&chart.step_artist_str));
    println!("      \"tech_notation\": \"{}\",", esc(&chart.tech_notation_str));
    println!("      \"hash_short\": \"{}\",", chart.short_hash);

    println!("      \"arrow_stats\": {{");
    println!("        \"left\": {},", chart.stats.left);
    println!("        \"down\": {},", chart.stats.down);
    println!("        \"up\": {},", chart.stats.up);
    println!("        \"right\": {},", chart.stats.right);
    println!("        \"total_arrows\": {},", chart.stats.total_arrows);
    println!("        \"total_steps\": {},", chart.stats.total_steps);
    println!("        \"jumps\": {},", chart.stats.jumps);
    println!("        \"hands\": {},", chart.stats.hands);
    println!("        \"holds\": {},", chart.stats.holds);
    println!("        \"rolls\": {},", chart.stats.rolls);
    println!("        \"mines\": {}", chart.stats.mines);
    println!("      }},");

    println!("      \"stream_counts\": {{");
    println!("        \"run16_streams\": {},", chart.stream_counts.run16_streams);
    println!("        \"run20_streams\": {},", chart.stream_counts.run20_streams);
    println!("        \"run24_streams\": {},", chart.stream_counts.run24_streams);
    println!("        \"run32_streams\": {},", chart.stream_counts.run32_streams);
    println!("        \"total_streams\": {},", chart.total_streams);
    println!("        \"total_breaks\": {}", chart.stream_counts.total_breaks);
    println!("      }},");

    println!("      \"max_nps\": {:.2},", chart.max_nps);
    println!("      \"median_nps\": {:.2},", chart.median_nps);
    println!("      \"mono_total\": {},", chart.mono_total);
    println!("      \"mono_percent\": {:.2},", chart.mono_percent);
    println!("      \"candle_total\": {},", chart.candle_total);
    println!("      \"candle_percent\": {:.2},", chart.candle_percent);

    println!("      \"pattern_counts\": {{");
    for (i, &pv) in ALL_PATTERNS.iter().enumerate() {
        let key = pattern_variant_name(pv);
        let val = count(&chart.detected_patterns, pv);
        if i + 1 < ALL_PATTERNS.len() {
            println!("        \"{}\": {},", key, val);
        } else {
            println!("        \"{}\": {}", key, val);
        }
    }
    println!("      }},");

    println!("      \"breakdown\": {{");
    println!("        \"detailed\": \"{}\",", esc(&chart.detailed));
    println!("        \"partial\": \"{}\",", esc(&chart.partial));
    println!("        \"simple\": \"{}\"", esc(&chart.simple));
    println!("      }}");

    println!("    }}{}", if is_last { "" } else { "," });
}

fn print_csv_all(simfile: &SimfileSummary) {
    let pattern_names: Vec<_> = ALL_PATTERNS.iter()
        .map(|&pv| pattern_variant_name(pv))
        .collect();

    println!(
        "title,subtitle,artist,min_bpm,max_bpm,difficulty,rating,step_artist,tech_notation,chart_length_s,\
total_arrows,total_streams,mono_percent,candle_percent,{}",
        pattern_names.join(",")
    );

    for chart in &simfile.charts {
        print_csv_row(simfile, chart, &pattern_names);
    }
}

fn print_csv_row(simfile: &SimfileSummary, chart: &ChartSummary, pattern_names: &[&str]) {
    fn esc_csv(s: &str) -> String {
        if s.contains('"') || s.contains(',') {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }

    print!("{},{},{},{:.2},{:.2},",
        esc_csv(&simfile.title_str),
        esc_csv(&simfile.subtitle_str),
        esc_csv(&simfile.artist_str),
        simfile.min_bpm,
        simfile.max_bpm
    );

    print!("{},{},{},{},",
        esc_csv(&chart.difficulty_str),
        esc_csv(&chart.rating_str),
        esc_csv(&chart.step_artist_str),
        esc_csv(&chart.tech_notation_str),
    );

    print!("{},{},{:.2},{:.2}",
        chart.stats.total_arrows,
        chart.total_streams,
        chart.mono_percent,
        chart.candle_percent
    );

    for pname in pattern_names {
        let pat_variant = ALL_PATTERNS.iter()
            .find(|&&x| pattern_variant_name(x) == *pname)
            .unwrap(); // we expect a match
        let val = count(&chart.detected_patterns, *pat_variant);
        print!(",{}", val);
    }

    println!();
}

use std::collections::HashMap;
use std::time::Duration;

use crate::patterns::PatternVariant;
use crate::stats::{ArrowStats, StreamCounts};

pub struct SimfileSummary {
    pub title_str:             String,
    pub subtitle_str:          String,
    pub artist_str:            String,
    pub titletranslit_str:     String,
    pub subtitletranslit_str:  String,
    pub artisttranslit_str:    String,

    pub offset:                f64,
    pub normalized_bpms:       String,
    pub step_type_str:         String,
    pub step_artist_str:       String,
    pub difficulty_str:        String,
    pub rating_str:            String,

    pub tech_notation_str:     String,

    pub stats:                 ArrowStats,
    pub stream_counts:         StreamCounts,
    pub total_streams:         u32,
    pub detailed:              String,
    pub partial:               String,
    pub simple:                String,

    pub min_bpm:               f64,
    pub max_bpm:               f64,
    pub total_length:          i32,
    pub max_nps:               f64,
    pub median_nps:            f64,
    pub median_bpm:            f64,
    pub average_bpm:           f64,
    
    pub detected_patterns:  HashMap<PatternVariant, u32>,

    pub anchor_left:           u32,
    pub anchor_down:           u32,
    pub anchor_up:             u32,
    pub anchor_right:          u32,
    pub facing_left:            u32,
    pub facing_right:           u32,
    pub mono_total:             u32,
    pub mono_percent:           f64,
    pub candle_total:           u32,
    pub candle_percent:         f64,

    pub short_hash:            String,

    pub elapsed:               Duration,

    pub measure_densities:     Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Full,
    Pretty,
    JSON,
    CSV,
}

pub fn print_report(data: &SimfileSummary, mode: OutputMode) {
    match mode {
        OutputMode::Full => print_full(data),
        OutputMode::Pretty => print_pretty(data),
        OutputMode::JSON => print_json(data),
        OutputMode::CSV  => print_csv(data),
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

fn print_pretty(data: &SimfileSummary) {
    println!("Song Details");
    println!("------------");
    println!("Title: {}{} by {}", 
        data.title_str, 
        if data.subtitle_str.is_empty() { 
            String::new() 
        } else { 
            format!(" {}", data.subtitle_str) 
        }, 
        data.artist_str
    );
    println!("Length: {}", format_duration(data.total_length));
    println!("{} {} : {}", data.difficulty_str, data.rating_str, data.step_artist_str);

    if (data.min_bpm - data.max_bpm).abs() < f64::EPSILON {
        println!("\nBPM: {:.0}", data.min_bpm);
    } else {
        println!("BPM: {:.0}-{:.0}", data.min_bpm, data.max_bpm);
        println!("Median BPM: {:.0}", data.median_bpm);
        println!("Average BPM: {:.0}", data.average_bpm);
    }

    if (data.median_nps - data.max_nps).abs() < f64::EPSILON {
        println!("NPS: {:.2} Median/Peak", data.median_nps);
    } else {
        println!("NPS: {:.2} Median, {:.2} Peak", data.median_nps, data.max_nps);
    }

    let total_stream = data.total_streams;
    let total_break = data.stream_counts.total_breaks;
    let stream_percent = if total_stream + total_break > 0 {
        (total_stream as f64 / (total_stream + total_break) as f64) * 100.0
    } else { 0.0 };

    println!("Total Stream: {} ({:.2}%)", total_stream, stream_percent);
    println!("Total Break: {} ({:.2}%)", total_break, 100.0 - stream_percent);
    
    println!("\nChart Info");
    println!("----------");
    println!("Steps: {} ({} arrows)", data.stats.total_steps, data.stats.total_arrows);
    println!("Jumps: {}", data.stats.jumps);
    println!("Holds: {}", data.stats.holds);
    println!("Mines: {}", data.stats.mines);
    println!("Hands: {}", data.stats.hands);
    println!("Rolls: {}", data.stats.rolls);

    println!("\nPattern Analysis");
    println!("----------------");
    let candle_left = data.detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
    let candle_right = data.detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
    println!("Candles: {} ({} left, {} right)", 
        candle_left + candle_right, 
        candle_left, 
        candle_right
    );
    println!("Candle%: {:.2}%", data.candle_percent);
    println!("Mono: {} ({} left-facing, {} right-facing)", data.mono_total, data.facing_left, data.facing_right);
    println!("Mono%: {:.2}%", data.mono_percent);
    let box_lr = data.detected_patterns.get(&PatternVariant::BoxLR).unwrap_or(&0);
    let box_ud = data.detected_patterns.get(&PatternVariant::BoxUD).unwrap_or(&0);
    let box_corners = 
        data.detected_patterns.get(&PatternVariant::BoxCornerLD).unwrap_or(&0) +
        data.detected_patterns.get(&PatternVariant::BoxCornerLU).unwrap_or(&0) +
        data.detected_patterns.get(&PatternVariant::BoxCornerRD).unwrap_or(&0) +
        data.detected_patterns.get(&PatternVariant::BoxCornerRU).unwrap_or(&0);
    println!("Boxes: {} ({} LRLR, {} UDUD, {} corner)", 
        box_lr + box_ud + box_corners, 
        box_lr, 
        box_ud, 
        box_corners
    );
    let anchor_total = data.anchor_left + data.anchor_down + data.anchor_up + data.anchor_right;
    println!("Anchors: {} ({} left, {} down, {} up, {} right)", 
        anchor_total, 
        data.anchor_left, 
        data.anchor_down, 
        data.anchor_up, 
        data.anchor_right
    );
    
    if !data.detailed.is_empty() {
        println!("\nDetailed Breakdown");
        println!("{}", data.detailed);
        println!("Partially Simplified");
        println!("{}", data.partial);
        println!("Simplified Breakdown");
        println!("{}", data.simple);
    }

    println!("\nElapsed time: {:?}", data.elapsed);
}

fn print_full(data: &SimfileSummary) {
    println!("Title: {}", data.title_str);
    println!("Title translate: {}", data.titletranslit_str);
    println!("Subtitle: {}", data.subtitle_str);
    println!("Subtitle translate: {}", data.subtitletranslit_str);
    println!("Artist: {}", data.artist_str);
    println!("Artist translate: {}", data.artisttranslit_str);
    println!("Offset: {:.3}", data.offset);
    println!("Normalized BPMs: {}", data.normalized_bpms);
    println!("Steptype: {}", data.step_type_str);
    println!("Difficulty: {}", data.difficulty_str);
    println!("Rating: {}", data.rating_str);
    println!("Step artist: {}", data.step_artist_str);
    println!("Tech notation: {}", data.tech_notation_str);
    println!("Hash (first 16 hex chars): {}", data.short_hash);

    println!("--- Arrow Stats ---");
    println!("Left: {}", data.stats.left);
    println!("Down: {}", data.stats.down);
    println!("Up: {}", data.stats.up);
    println!("Right: {}", data.stats.right);
    println!("Total arrows: {}", data.stats.total_arrows);
    println!("Total steps: {}", data.stats.total_steps);
    println!("Jumps (2-arrow steps): {}", data.stats.jumps);
    println!("Hands (3+ arrow steps): {}", data.stats.hands);
    println!("Holds: {}", data.stats.holds);
    println!("Rolls: {}", data.stats.rolls);
    println!("Mines: {}", data.stats.mines);

    println!("--- Stream Counts ---");
    println!("16th streams: {}", data.stream_counts.run16_streams);
    println!("20th streams: {}", data.stream_counts.run20_streams);
    println!("24th streams: {}", data.stream_counts.run24_streams);
    println!("32nd streams: {}", data.stream_counts.run32_streams);
    println!("Total streams: {}", data.total_streams);
    println!("Total breaks: {}", data.stream_counts.total_breaks);

    println!("Detailed breakdown: {}", data.detailed);
    println!("Partially simplified: {}", data.partial);
    println!("Simplified breakdown: {}", data.simple);

    println!("--- Additional Chart Info ---");
    println!("Min BPM: {:.2}", data.min_bpm);
    println!("Max BPM: {:.2}", data.max_bpm);
    println!("Chart length (seconds): {}", data.total_length);
    println!("Max NPS: {:.2}", data.max_nps);
    println!("Median NPS: {:.2}", data.median_nps);

    println!("--- Mono Patterns ---");
    println!("Left-facing steps: {}", data.facing_left);
    println!("Right-facing steps: {}", data.facing_right);
    println!("Mono total: {}", data.mono_total);
    println!("Mono percentage: {:.2}%", data.mono_percent);

    println!("--- Candle Patterns ---");
    println!("Candle total: {}", data.candle_total);
    println!("Candle percentage: {:.2}%", data.candle_percent);

    println!("--- Pattern Counts (non-anchors) ---");
    for &pv in ALL_PATTERNS {
        let key = pattern_variant_name(pv);
        let val = count(&data.detected_patterns, pv);
        println!("{}: {}", key, val);
    }

    println!("--- Anchors ---");
    println!("AnchorLeft: {}",  data.anchor_left);
    println!("AnchorDown: {}",  data.anchor_down);
    println!("AnchorUp: {}",    data.anchor_up);
    println!("AnchorRight: {}", data.anchor_right);

    println!("---");
    println!("Elapsed time: {:?}", data.elapsed);
}

fn print_json(data: &SimfileSummary) {
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
    println!("  \"title\": \"{}\",", esc(&data.title_str));
    println!("  \"title_translit\": \"{}\",", esc(&data.titletranslit_str));
    println!("  \"subtitle\": \"{}\",", esc(&data.subtitle_str));
    println!("  \"subtitle_translit\": \"{}\",", esc(&data.subtitletranslit_str));
    println!("  \"artist\": \"{}\",", esc(&data.artist_str));
    println!("  \"artist_translit\": \"{}\",", esc(&data.artisttranslit_str));
    println!("  \"offset\": {:.3},", data.offset);
    println!("  \"bpms\": \"{}\",", esc(&data.normalized_bpms));
    println!("  \"step_type\": \"{}\",", esc(&data.step_type_str));
    println!("  \"step_artist\": \"{}\",", esc(&data.step_artist_str));
    println!("  \"difficulty\": \"{}\",", esc(&data.difficulty_str));
    println!("  \"rating\": \"{}\",", esc(&data.rating_str));
    println!("  \"tech_notation\": \"{}\",", esc(&data.tech_notation_str));
    println!("  \"hash_short\": \"{}\",", data.short_hash);

    println!("  \"arrow_stats\": {{");
    println!("     \"left\": {},", data.stats.left);
    println!("     \"down\": {},", data.stats.down);
    println!("     \"up\": {},", data.stats.up);
    println!("     \"right\": {},", data.stats.right);
    println!("     \"total_arrows\": {},", data.stats.total_arrows);
    println!("     \"total_steps\": {},", data.stats.total_steps);
    println!("     \"jumps\": {},", data.stats.jumps);
    println!("     \"hands\": {},", data.stats.hands);
    println!("     \"holds\": {},", data.stats.holds);
    println!("     \"rolls\": {},", data.stats.rolls);
    println!("     \"mines\": {}", data.stats.mines);
    println!("  }},");

    println!("  \"stream_counts\": {{");
    println!("     \"run16_streams\": {},", data.stream_counts.run16_streams);
    println!("     \"run20_streams\": {},", data.stream_counts.run20_streams);
    println!("     \"run24_streams\": {},", data.stream_counts.run24_streams);
    println!("     \"run32_streams\": {},", data.stream_counts.run32_streams);
    println!("     \"total_streams\": {},", data.total_streams);
    println!("     \"total_breaks\": {}", data.stream_counts.total_breaks);
    println!("  }},");

    println!("  \"breakdown\": {{");
    println!("     \"detailed\": \"{}\",", esc(&data.detailed));
    println!("     \"partial\": \"{}\",", esc(&data.partial));
    println!("     \"simple\": \"{}\"", esc(&data.simple));
    println!("  }},");

    println!("  \"bpm_info\": {{");
    println!("     \"min_bpm\": {:.2},", data.min_bpm);
    println!("     \"max_bpm\": {:.2},", data.max_bpm);
    println!("     \"chart_length_s\": {},", data.total_length);
    println!("     \"max_nps\": {:.4},", data.max_nps);
    println!("     \"median_nps\": {:.4}", data.median_nps);
    println!("  }},");

    println!("  \"mono_counts\": {{");
    println!("     \"left-facing\": {},", data.facing_left);
    println!("     \"right-facing\": {},", data.facing_right);
    println!("     \"mono_total\": {},", data.mono_total);
    println!("     \"mono_percent\": {:.2}", data.mono_percent);
    println!("  }},");

    println!("  \"candle_counts\": {{");
    println!("     \"candle_total\": {},", data.candle_total);
    println!("     \"candle_percent\": {:.2}", data.candle_percent);
    println!("  }},");

    println!("  \"pattern_counts\": {{");
    for (i, &pv) in ALL_PATTERNS.iter().enumerate() {
        let key = pattern_variant_name(pv);
        let val = count(&data.detected_patterns, pv);
        if i + 1 < ALL_PATTERNS.len() {
            println!("     \"{}\": {},", key, val);
        } else {
            println!("     \"{}\": {}", key, val);
        }
    }
    println!("  }},");

    println!("  \"anchor_counts\": {{");
    println!("     \"anchor_left\": {},",  data.anchor_left);
    println!("     \"anchor_down\": {},",  data.anchor_down);
    println!("     \"anchor_up\": {},",    data.anchor_up);
    println!("     \"anchor_right\": {}",  data.anchor_right);
    println!("  }},");

    println!("  \"elapsed\": \"{:?}\"", data.elapsed);
    println!("}}");
}

fn print_csv(data: &SimfileSummary) {   
    println!("title,subtitle,artist,normalized_bpms,step_type,step_artist,difficulty,rating,tech_notation,hash_short,\
        min_bpm,max_bpm,total_length,max_nps,median_nps,anchor_left,anchor_down,\
        anchor_up,anchor_right,{},total_arrows,jumps,total_breaks",
        ALL_PATTERNS.iter().map(|p| pattern_variant_name(*p)).collect::<Vec<_>>().join(",")
    );

    let mut cols = Vec::new();

    fn esc_csv(s: &str) -> String {
        s.replace('"', "\"\"")
    }

    cols.push(format!("\"{}\"", esc_csv(&data.title_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.subtitle_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.artist_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.normalized_bpms)));
    cols.push(format!("\"{}\"", esc_csv(&data.step_type_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.step_artist_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.difficulty_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.rating_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.tech_notation_str)));
    cols.push(format!("\"{}\"", data.short_hash));
    cols.push(format!("{:.2}", data.min_bpm));
    cols.push(format!("{:.2}", data.max_bpm));
    cols.push(format!("{}",  data.total_length));
    cols.push(format!("{:.2}", data.max_nps));
    cols.push(format!("{:.2}", data.median_nps));

    cols.push(format!("{}", data.anchor_left));
    cols.push(format!("{}", data.anchor_down));
    cols.push(format!("{}", data.anchor_up));
    cols.push(format!("{}", data.anchor_right));

    for &pv in ALL_PATTERNS {
        let val = count(&data.detected_patterns, pv);
        cols.push(format!("{}", val));
    }

    cols.push(format!("{}", data.stats.total_arrows));
    cols.push(format!("{}", data.stats.jumps));
    cols.push(format!("{}", data.stream_counts.total_breaks));

    println!("{}", cols.join(","));
}

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

    pub normalized_bpms:       String,
    pub step_type_str:         String,
    pub difficulty_str:        String,
    pub rating_str:            String,

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

    pub detected_non_anchors:  HashMap<PatternVariant, u32>,

    pub anchor_left:           u32,
    pub anchor_down:           u32,
    pub anchor_up:             u32,
    pub anchor_right:          u32,

    pub short_hash:            String,

    pub elapsed:               Duration,

    pub measure_densities:     Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    JSON,
    CSV,
}

pub fn print_report(data: &SimfileSummary, mode: OutputMode) {
    match mode {
        OutputMode::Text => print_text(data),
        OutputMode::JSON => print_json(data),
        OutputMode::CSV  => print_csv(data),
    }
}

static ALL_PATTERNS: &[PatternVariant] = &[
    PatternVariant::CandleLeft,
    PatternVariant::CandleRight,
    PatternVariant::BoxLR,
    PatternVariant::BoxUD,
    PatternVariant::BoxCornerLD,
    PatternVariant::BoxCornerLU,
    PatternVariant::BoxCornerRD,
    PatternVariant::BoxCornerRU,
    PatternVariant::DoritoLeft,
    PatternVariant::DoritoRight,
    PatternVariant::DoritoInvLeft,
    PatternVariant::DoritoInvRight,
    PatternVariant::SpiralLeft,
    PatternVariant::SpiralRight,
    PatternVariant::CopterLeft,
    PatternVariant::CopterRight,
    PatternVariant::LuchiLeft,
    PatternVariant::LuchiRight,
    PatternVariant::HipBreakerLeft,
    PatternVariant::HipBreakerRight,
    PatternVariant::SweepLeft,
    PatternVariant::SweepRight,
    PatternVariant::SweepInvLeft,
    PatternVariant::SweepInvRight,
];

fn count(map: &HashMap<PatternVariant, u32>, v: PatternVariant) -> u32 {
    *map.get(&v).unwrap_or(&0)
}

fn pattern_variant_name(pv: PatternVariant) -> &'static str {
    match pv {
        PatternVariant::CandleLeft      => "candle_left",
        PatternVariant::CandleRight     => "candle_right",
        PatternVariant::BoxLR           => "box_lr",
        PatternVariant::BoxUD           => "box_ud",
        PatternVariant::BoxCornerLD     => "box_corner_ld",
        PatternVariant::BoxCornerLU     => "box_corner_lu",
        PatternVariant::BoxCornerRD     => "box_corner_rd",
        PatternVariant::BoxCornerRU     => "box_corner_ru",
        PatternVariant::DoritoLeft      => "dorito_left",
        PatternVariant::DoritoRight     => "dorito_right",
        PatternVariant::DoritoInvLeft   => "dorito_inv_left",
        PatternVariant::DoritoInvRight  => "dorito_inv_right",
        PatternVariant::SpiralLeft      => "left_spiral",
        PatternVariant::SpiralRight     => "right_spiral",
        PatternVariant::CopterLeft      => "left_copter",
        PatternVariant::CopterRight     => "right_copter",
        PatternVariant::LuchiLeft       => "left_luchi",
        PatternVariant::LuchiRight      => "right_luchi",
        PatternVariant::HipBreakerLeft  => "left_hip_breaker",
        PatternVariant::HipBreakerRight => "right_hip_breaker",
        PatternVariant::SweepLeft       => "left_sweep",
        PatternVariant::SweepRight      => "right_sweep",
        PatternVariant::SweepInvLeft    => "left_inv_sweep",
        PatternVariant::SweepInvRight   => "right_inv_sweep",
    }
}

fn print_text(data: &SimfileSummary) {
    println!("Title: {}", data.title_str);
    println!("Title translate: {}", data.titletranslit_str);
    println!("Subtitle: {}", data.subtitle_str);
    println!("Subtitle translate: {}", data.subtitletranslit_str);
    println!("Artist: {}", data.artist_str);
    println!("Artist translate: {}", data.artisttranslit_str);
    println!("Normalized BPMs: {}", data.normalized_bpms);
    println!("Steptype: {}", data.step_type_str);
    println!("Difficulty: {}", data.difficulty_str);
    println!("Rating: {}", data.rating_str);
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

    println!("--- Pattern Counts (non-anchors) ---");
    for &pv in ALL_PATTERNS {
        let key = pattern_variant_name(pv);
        let val = count(&data.detected_non_anchors, pv);
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
    println!("  \"bpms\": \"{}\",", esc(&data.normalized_bpms));
    println!("  \"step_type\": \"{}\",", esc(&data.step_type_str));
    println!("  \"difficulty\": \"{}\",", esc(&data.difficulty_str));
    println!("  \"rating\": \"{}\",", esc(&data.rating_str));
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

    println!("  \"pattern_counts\": {{");
    for (i, &pv) in ALL_PATTERNS.iter().enumerate() {
        let key = pattern_variant_name(pv);
        let val = count(&data.detected_non_anchors, pv);
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
    println!("title,subtitle,artist,normalized_bpms,step_type,difficulty,rating,hash_short,\
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
    cols.push(format!("\"{}\"", esc_csv(&data.difficulty_str)));
    cols.push(format!("\"{}\"", esc_csv(&data.rating_str)));
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
        let val = count(&data.detected_non_anchors, pv);
        cols.push(format!("{}", val));
    }

    cols.push(format!("{}", data.stats.total_arrows));
    cols.push(format!("{}", data.stats.jumps));
    cols.push(format!("{}", data.stream_counts.total_breaks));

    println!("{}", cols.join(","));
}

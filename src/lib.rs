use std::time::Instant;

pub mod bpm;
pub mod graph;
pub mod hashing;
pub mod matrix;
pub mod parse;
pub mod patterns;
pub mod report;
pub mod stats;
pub mod tech;

// Re-export the primary data structures for library users
pub use report::{ChartSummary, SimfileSummary};

use crate::bpm::*;
use crate::hashing::*;
use crate::matrix::compute_matrix_rating;
use crate::parse::*;
use crate::patterns::*;
use crate::stats::*;
use crate::tech::parse_step_artist_and_tech;

/// Options for controlling simfile analysis.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalysisOptions {
    pub strip_tags: bool,
    pub mono_threshold: usize,
}

pub fn analyze(
    simfile_data: &[u8],
    extension: &str,
    options: AnalysisOptions,
) -> Result<SimfileSummary, String> {
    let total_start_time = Instant::now();

    let (
        title_opt,
        subtitle_opt,
        artist_opt,
        titletranslit_opt,
        subtitletranslit_opt,
        artisttranslit_opt,
        offset_opt,
        bpms_opt,
        banner_opt,
        background_opt,
        music_opt,
        notes_list,
    ) = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    let mut title_str = title_opt
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(unescape_tag)
        .map(|s| clean_tag(&s))
        .unwrap_or_else(|| "<invalid-title>".to_string());
    if options.strip_tags {
        title_str = strip_title_tags(&title_str);
    }

    let subtitle_str = subtitle_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let artist_str = artist_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let titletranslit_str = titletranslit_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let subtitletranslit_str = subtitletranslit_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let artisttranslit_str = artisttranslit_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let banner_path_str = banner_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let background_path_str = background_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let music_path_str = music_opt.and_then(|b| std::str::from_utf8(b).ok()).map(unescape_tag).unwrap_or_default();
    let offset = offset_opt.and_then(|b| std::str::from_utf8(b).ok()).and_then(|s| s.parse::<f64>().ok()).map(|f| (f * 1000.0).trunc() / 1000.0).unwrap_or(0.0);
    let global_bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>")).unwrap_or("<invalid-bpms>");
    let normalized_global_bpms = normalize_float_digits(global_bpms_raw);

    let global_bpm_map = parse_bpm_map(&normalized_global_bpms);
    let (min_bpm_i32, max_bpm_i32) = compute_bpm_range(&global_bpm_map);
    let bpm_values: Vec<f64> = global_bpm_map.iter().map(|&(_, bpm)| bpm).collect();
    let (median_bpm, average_bpm) = compute_bpm_stats(&bpm_values);

    let mut chart_summaries = Vec::new();

    for (_chart_num, (notes_data, chart_bpms_opt)) in notes_list.into_iter().enumerate() {
        let chart_start_time = Instant::now();

        let (fields, chart_data) = split_notes_fields(&notes_data);
        if fields.len() < 5 {
            continue;
        }

        let step_type_str = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_owned();
        let description = std::str::from_utf8(fields[1]).unwrap_or("").trim().to_owned();
        let difficulty_str = std::str::from_utf8(fields[2]).unwrap_or("").trim().to_owned();
        let rating_str = std::str::from_utf8(fields[3]).unwrap_or("").trim().to_owned();
        let credit = if extension.eq_ignore_ascii_case("ssc") {
            std::str::from_utf8(fields[4]).unwrap_or("").trim().to_owned()
        } else {
            String::new()
        };

        let (step_artist_str, tech_notation_str) = parse_step_artist_and_tech(&credit, &description);
        let (mut minimized_chart, stats, measure_densities) = minimize_chart_and_count(chart_data);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }

        let bpms_to_use = if let Some(chart_bpms) = chart_bpms_opt {
            normalize_float_digits(std::str::from_utf8(&chart_bpms).unwrap_or(""))
        } else {
            normalized_global_bpms.clone()
        };
        let bpm_map = parse_bpm_map(&bpms_to_use);

        let stream_counts = compute_stream_counts(&measure_densities);
        let total_measures = measure_densities.len();
        let total_streams = stream_counts.run16_streams
            + stream_counts.run20_streams
            + stream_counts.run24_streams
            + stream_counts.run32_streams;

        let detailed = generate_breakdown(&measure_densities, BreakdownMode::Detailed);
        let partial = generate_breakdown(&measure_densities, BreakdownMode::Partial);
        let simple = generate_breakdown(&measure_densities, BreakdownMode::Simplified);

        let measure_nps_vec = compute_measure_nps_vec(&measure_densities, &bpm_map);
        let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);
        let bpm_neutral_hash = compute_chart_hash(&minimized_chart, "0.000=0.000");
        let tier_bpm = compute_tier_bpm(&measure_densities, &bpm_map, 4.0);
        let matrix_rating = compute_matrix_rating(&measure_densities, &bpm_map);
        let short_hash = compute_chart_hash(&minimized_chart, &bpms_to_use);

        let bitmasks = {
            let mut res = Vec::new();
            for line in minimized_chart.split(|&b| b == b'\n') {
                if line.len() >= 4 {
                    let mut mask = 0u8;
                    if matches!(line[0], b'1' | b'2' | b'4') { mask |= 1 << 0; }
                    if matches!(line[1], b'1' | b'2' | b'4') { mask |= 1 << 1; }
                    if matches!(line[2], b'1' | b'2' | b'4') { mask |= 1 << 2; }
                    if matches!(line[3], b'1' | b'2' | b'4') { mask |= 1 << 3; }
                    if mask != 0 || line.iter().any(|&b| !(b == b',' || b == b' ')) {
                        res.push(mask);
                    }
                }
            }
            res
        };

        let default_patterns: &Vec<(PatternVariant, Vec<u8>)> = &*DEFAULT_PATTERNS;
        let extra_patterns: &Vec<(PatternVariant, Vec<u8>)> = &*EXTRA_PATTERNS;
        let pattern_list: Vec<(PatternVariant, Vec<u8>)> = default_patterns.iter().chain(extra_patterns.iter()).cloned().collect();

        let detected_patterns = detect_patterns(&bitmasks, &pattern_list);
        let (anchor_left, anchor_down, anchor_up, anchor_right) = count_anchors(&bitmasks);

        let (facing_left, facing_right, mono_total, mono_percent, candle_total, candle_percent) =
            if stats.total_steps > 1 {
                let (f_left, f_right) = count_facing_steps(&bitmasks, options.mono_threshold);
                let mono_total = f_left + f_right;
                let mono_percent = (mono_total as f64 / stats.total_steps as f64) * 100.0;
                let candle_left = *detected_patterns.get(&PatternVariant::CandleLeft).unwrap_or(&0);
                let candle_right = *detected_patterns.get(&PatternVariant::CandleRight).unwrap_or(&0);
                let candle_total = candle_left + candle_right;
                let max_candles = (stats.total_steps - 1) / 2;
                let candle_percent = if max_candles > 0 { (candle_total as f64 / max_candles as f64) * 100.0 } else { 0.0 };
                (f_left, f_right, mono_total, mono_percent, candle_total, candle_percent)
            } else {
                (0, 0, 0, 0.0, 0, 0.0)
            };

        let elapsed_chart = chart_start_time.elapsed();

        let summary = ChartSummary {
            step_type_str, step_artist_str, difficulty_str, rating_str, tech_notation_str,
            tier_bpm, matrix_rating, stats, stream_counts, total_streams, total_measures,
            detailed, partial, simple, max_nps, median_nps, detected_patterns,
            anchor_left, anchor_down, anchor_up, anchor_right,
            facing_left, facing_right, mono_total, mono_percent,
            candle_total, candle_percent, short_hash, bpm_neutral_hash,
            elapsed: elapsed_chart, measure_densities, measure_nps_vec,
            notes: bitmasks,
        };

        chart_summaries.push(summary);
    }

    let total_length = if !chart_summaries.is_empty() {
        let first_measures = &chart_summaries[0].measure_densities;
        compute_total_chart_length(first_measures, &global_bpm_map)
    } else {
        0
    };

    let total_elapsed = total_start_time.elapsed();

    Ok(SimfileSummary {
        title_str, subtitle_str, artist_str, titletranslit_str, subtitletranslit_str,
        artisttranslit_str, offset, normalized_bpms: normalized_global_bpms,
        banner_path: banner_path_str,
        background_path: background_path_str,
        music_path: music_path_str,
        min_bpm: min_bpm_i32 as f64, max_bpm: max_bpm_i32 as f64,
        median_bpm, average_bpm, total_length, charts: chart_summaries, total_elapsed,
    })
}

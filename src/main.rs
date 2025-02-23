use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::{Instant};

use rssp::graph::*;
use rssp::parse::*;
use rssp::stats::*;
use rssp::patterns::*;
use rssp::bpm::*;
use rssp::hashing::*;
use rssp::report::*;
use rssp::tech::{parse_step_artist_and_tech};

fn main() -> io::Result<()> {
    let total_start_time = Instant::now();
    let args: Vec<String> = args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path> [--png] [--json] [--csv] [--strip-tags] [--mono-threshold <value>]", args[0]);
        std::process::exit(1);
    }

    let simfile_path  = &args[1];

    let generate_full = args.iter().any(|a| a == "--full");
    let generate_png  = args.iter().any(|a| a == "--png");
    let generate_png_alt = args.iter().any(|a| a == "--png-alt");
    let generate_json = args.iter().any(|a| a == "--json");
    let generate_csv  = args.iter().any(|a| a == "--csv");
    let strip_tags    = args.iter().any(|a| a == "--strip-tags");

    let mut mono_threshold = 6;
    if let Some(pos) = args.iter().position(|arg| arg == "--mono-threshold") {
        if pos + 1 < args.len() {
            if let Ok(value) = args[pos + 1].parse::<usize>() {
                mono_threshold = value;
            } else {
                eprintln!("Error: Invalid value for --mono-threshold. Must be a positive integer.");
                std::process::exit(1);
            }
        } else {
            eprintln!("Error: Missing value for --mono-threshold.");
            std::process::exit(1);
        }
    }

    let extension = match simfile_path.rsplit_once('.') {
        Some((_, ext)) => ext, 
        None => "", // no extension
    };

    let mut file = File::open(simfile_path)?;
    let mut simfile_data = Vec::new();
    file.read_to_end(&mut simfile_data)?;

    let (
        title_opt,
        subtitle_opt,
        artist_opt,
        titletranslit_opt,
        subtitletranslit_opt,
        artisttranslit_opt,
        offset_opt,
        bpms_opt,
        notes_list,
    ) = match extract_sections(&simfile_data, extension) {
        Ok(tup) => tup,
        Err(e) => {
            eprintln!("Error extracting sections: {}", e);
            std::process::exit(1);
        }
    };

    // Process file metadata once
    let mut title_str = std::str::from_utf8(title_opt.unwrap_or(b"<invalid-title>"))
        .unwrap_or("<invalid-title>")
        .to_owned();
    if strip_tags {
        title_str = strip_title_tags(&title_str);
    }

    let subtitle_str = std::str::from_utf8(subtitle_opt.unwrap_or(b""))
        .unwrap_or("")
        .to_owned();
    let artist_str   = std::str::from_utf8(artist_opt.unwrap_or(b""))
        .unwrap_or("")
        .to_owned();

    let titletranslit_str = std::str::from_utf8(titletranslit_opt.unwrap_or(b""))
        .unwrap_or("")
        .to_owned();
    let subtitletranslit_str = std::str::from_utf8(subtitletranslit_opt.unwrap_or(b""))
        .unwrap_or("")
        .to_owned();
    let artisttranslit_str = std::str::from_utf8(artisttranslit_opt.unwrap_or(b""))
        .unwrap_or("")
        .to_owned();

    let offset = offset_opt
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| (f * 1000.0).trunc() / 1000.0) // Truncate to 3 decimals
        .unwrap_or(0.000);

    let bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");
    let normalized_bpms = normalize_float_digits(bpms_raw);

    let bpm_map = parse_bpm_map(&normalized_bpms);
    let (min_bpm_i32, max_bpm_i32) = compute_bpm_range(&bpm_map);
    let min_bpm = min_bpm_i32 as f64;
    let max_bpm = max_bpm_i32 as f64;

    let bpm_values: Vec<f64> = bpm_map.iter().map(|&(_, bpm)| bpm).collect();
    let (median_bpm, average_bpm) = compute_bpm_stats(&bpm_values);

    let mut chart_summaries = Vec::new();

    // Process each chart
    for (chart_num, notes_data) in notes_list.into_iter().enumerate() {
        let chart_start_time = Instant::now();

        let (fields, chart_data) = split_notes_fields(&notes_data);
        if fields.len() < 5 {
            eprintln!("Chart {}: #NOTES section incomplete", chart_num + 1);
            continue;
        }

        let step_type_str  = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_owned();

        let field1_str     = std::str::from_utf8(fields[1]).unwrap_or("").trim().to_owned();
        let (step_artist_str, tech_notations) = parse_step_artist_and_tech(&field1_str);

        let tech_notation_str = tech_notations
            .iter()
            .map(|tn| tn.0.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let difficulty_str = std::str::from_utf8(fields[2]).unwrap_or("").trim().to_owned();
        let rating_str     = std::str::from_utf8(fields[3]).unwrap_or("").trim().to_owned();

        let (mut minimized_chart, stats, measure_densities) = minimize_chart_and_count(chart_data);
        if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
            minimized_chart.truncate(pos + 1);
        }
        let stream_counts = compute_stream_counts(&measure_densities);
        let total_streams = stream_counts.run16_streams
            + stream_counts.run20_streams
            + stream_counts.run24_streams
            + stream_counts.run32_streams;

        let detailed = generate_breakdown(&measure_densities, BreakdownMode::Detailed);
        let partial  = generate_breakdown(&measure_densities, BreakdownMode::Partial);
        let simple   = generate_breakdown(&measure_densities, BreakdownMode::Simplified);

        let measure_nps_vec = compute_measure_nps_vec(&measure_densities, &bpm_map);
        let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

        let short_hash = compute_chart_hash(&minimized_chart, &normalized_bpms);

        let bitmasks = {
            let mut res = Vec::new();
            for line in minimized_chart.split(|&b| b == b'\n') {
                if line.len() >= 4 {
                    let mut mask = 0u8;
                    if matches!(line[0], b'1' | b'2' | b'4') {
                        mask |= 1 << 0;
                    }
                    if matches!(line[1], b'1' | b'2' | b'4') {
                        mask |= 1 << 1;
                    }
                    if matches!(line[2], b'1' | b'2' | b'4') {
                        mask |= 1 << 2;
                    }
                    if matches!(line[3], b'1' | b'2' | b'4') {
                        mask |= 1 << 3;
                    }
                    if mask != 0 || line.iter().any(|&b| !(b == b',' || b == b' ')) {
                        res.push(mask);
                    }
                }
            }
            res
        };

        let default_patterns: &Vec<(PatternVariant, Vec<u8>)> = &*DEFAULT_PATTERNS;
        let extra_patterns: &Vec<(PatternVariant, Vec<u8>)> = &*EXTRA_PATTERNS;

        let pattern_list: Vec<(PatternVariant, Vec<u8>)> = if generate_full {
            default_patterns.iter().chain(extra_patterns.iter()).cloned().collect()
        } else {
            default_patterns.clone()
        };

        let detected_patterns = detect_patterns(&bitmasks, &pattern_list);
        let (anchor_left, anchor_down, anchor_up, anchor_right) = count_anchors(&bitmasks);

        // Mono/candle
        let (facing_left, facing_right, mono_total, mono_percent, candle_total, candle_percent) =
            if stats.total_steps > 1 {
                let (f_left, f_right) = count_facing_steps(&bitmasks, mono_threshold);
                let mono_total = f_left + f_right;
                let mono_percent = (mono_total as f64 / stats.total_steps as f64) * 100.0;

                let candle_left = *detected_patterns.get(&rssp::patterns::PatternVariant::CandleLeft).unwrap_or(&0);
                let candle_right = *detected_patterns.get(&rssp::patterns::PatternVariant::CandleRight).unwrap_or(&0);
                let candle_total = candle_left + candle_right;
                let max_candles = (stats.total_steps - 1) / 2;
                let candle_percent = if max_candles > 0 {
                    (candle_total as f64 / max_candles as f64) * 100.0
                } else {
                    0.0
                };

                (f_left, f_right, mono_total, mono_percent, candle_total, candle_percent)
            } else {
                (0, 0, 0, 0.0, 0, 0.0)
            };

        let elapsed_chart = chart_start_time.elapsed();

        let summary = ChartSummary {
            step_type_str,
            step_artist_str,
            difficulty_str,
            rating_str,
            tech_notation_str,

            stats,
            stream_counts,
            total_streams,

            detailed,
            partial,
            simple,

            max_nps,
            median_nps,

            detected_patterns,

            anchor_left,
            anchor_down,
            anchor_up,
            anchor_right,
            facing_left,
            facing_right,
            mono_total,
            mono_percent,
            candle_total,
            candle_percent,

            short_hash,

            elapsed: elapsed_chart,

            measure_densities,
        };

        // Store it
        chart_summaries.push(summary);
    }

    let total_length = if !chart_summaries.is_empty() {
        let first_measures = &chart_summaries[0].measure_densities;
        compute_total_chart_length(first_measures, &bpm_map)
    } else {
        0
    };

    let simfile = SimfileSummary {
        title_str,
        subtitle_str,
        artist_str,
        titletranslit_str,
        subtitletranslit_str,
        artisttranslit_str,
        offset,
        normalized_bpms,
        min_bpm,
        max_bpm,
        median_bpm,
        average_bpm,
        total_length,

        charts: chart_summaries,
    };

    let mode = if generate_csv {
        OutputMode::CSV
    } else if generate_json {
        OutputMode::JSON
    } else if generate_full {
        OutputMode::Full
    } else {
        OutputMode::Pretty
    };

    let total_elapsed = total_start_time.elapsed();

    print_reports(&simfile, mode);

    if generate_png || generate_png_alt {
        let color_scheme = if generate_png_alt { ColorScheme::Alternative } else { ColorScheme::Default };

        for chart_summary in &simfile.charts {
            // measure_nps_vec can be derived from chart_summary.measure_densities + the same BPM map
            // but we already have it in the chart’s code. If we want to store it in ChartSummary,
            // we can do that. For demonstration, we’ll recompute:
            let measure_nps_vec = compute_measure_nps_vec(&chart_summary.measure_densities, &bpm_map);

            generate_density_graph_png(
                &measure_nps_vec,
                chart_summary.max_nps,
                &chart_summary.short_hash,
                &color_scheme,
            )?;
        }
    }

    eprintln!("Done in {:?}.", total_elapsed);

    Ok(())
}

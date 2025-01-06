use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::{Duration, Instant};

use rssp::parse::*;
use rssp::stats::*;
use rssp::patterns::*;
use rssp::bpm::*;
use rssp::hashing::*;
use rssp::report::*;

fn main() -> io::Result<()> {
    let start_time = Instant::now();
    let args: Vec<String> = args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path> [--png] [--json] [--csv] [--strip-tags]", args[0]);
        std::process::exit(1);
    }

    let simfile_path  = &args[1];
    let generate_png  = args.iter().any(|a| a == "--png");
    let generate_json = args.iter().any(|a| a == "--json");
    let generate_csv  = args.iter().any(|a| a == "--csv");
    let strip_tags    = args.iter().any(|a| a == "--strip-tags");

    let mut file = File::open(simfile_path)?;
    let mut simfile_data = Vec::new();
    file.read_to_end(&mut simfile_data)?;

    let mut summary = match summarize_simfile(&simfile_data, strip_tags) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    };

    let mode = if generate_csv {
        OutputMode::CSV
    } else if generate_json {
        OutputMode::JSON
    } else {
        OutputMode::Text
    };

    let elapsed = start_time.elapsed();
    summary.elapsed = elapsed;

    print_report(&summary, mode);

    if generate_png {
        let bpm_map = parse_bpm_map(&summary.normalized_bpms);
        let measure_nps_vec = compute_measure_nps_vec(&summary.measure_densities, &bpm_map);
        if !measure_nps_vec.is_empty() && summary.max_nps > 0.0 {
            rssp::graph::generate_density_graph_png(
                &measure_nps_vec,
                summary.max_nps,
                &summary.short_hash
            )?;
        }
    }

    Ok(())
}

fn summarize_simfile(
    simfile_data: &[u8],
    strip_tags: bool,
) -> Result<SimfileSummary, String> {
    let (title_opt, subtitle_opt, artist_opt,
         titletranslit_opt, subtitletranslit_opt, artisttranslit_opt,
         bpms_opt, notes_opt)
       = extract_sections(simfile_data)
         .map_err(|e| format!("Error: could not extract sections: {}", e))?;

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

    let bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");
    let normalized_bpms = normalize_float_digits(bpms_raw);

    let notes_data = notes_opt.ok_or_else(|| String::from("Missing #NOTES section"))?;
    let (fields, chart_data) = split_notes_fields(notes_data);
    if fields.len() < 5 {
        return Err("#NOTES section incomplete".to_string());
    }

    let step_type_str  = std::str::from_utf8(fields[0]).unwrap_or("").trim().to_owned();
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

    let bpm_map = parse_bpm_map(&normalized_bpms);
    let (min_bpm_i32, max_bpm_i32) = compute_bpm_range(&bpm_map);
    let min_bpm = min_bpm_i32 as f64;
    let max_bpm = max_bpm_i32 as f64;

    let measure_nps_vec = compute_measure_nps_vec(&measure_densities, &bpm_map);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);
    let total_length = compute_total_chart_length(&measure_densities, &bpm_map);

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
                // Keep lines if they have data or are not purely a comma line
                if mask != 0 || line.iter().any(|&b| !(b == b',' || b == b' ')) {
                    res.push(mask);
                }
            }
        }
        res
    };
    let detected_non_anchors = detect_all_patterns_non_anchors(&bitmasks);
    let (anchor_left, anchor_down, anchor_up, anchor_right) = count_anchors(&bitmasks);

    Ok(SimfileSummary {
        title_str,
        subtitle_str,
        artist_str,
        titletranslit_str,
        subtitletranslit_str,
        artisttranslit_str,

        normalized_bpms,
        step_type_str,
        difficulty_str,
        rating_str,

        stats,
        stream_counts,
        total_streams,
        detailed,
        partial,
        simple,

        min_bpm,
        max_bpm,
        total_length,
        max_nps,
        median_nps,

        detected_non_anchors,
        anchor_left,
        anchor_down,
        anchor_up,
        anchor_right,

        short_hash,

        elapsed: Duration::default(),
        measure_densities,
    })
}

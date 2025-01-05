use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::Instant;

use sha1::{Digest, Sha1};

use rssp::parse::{*};
use rssp::stats::{*};
use rssp::patterns::{*};
use rssp::bpm::{*};
use rssp::graph::{*};
use rssp::output::{*};

fn main() -> io::Result<()> {
    let start_time = Instant::now();

    let args: Vec<String> = args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path> [--png] [--json] [--strip-tags]", args[0]);
        std::process::exit(1);
    }

    let simfile_path = &args[1];
    let mut file = File::open(simfile_path)?;
    let mut simfile_data = Vec::new();
    file.read_to_end(&mut simfile_data)?;

    let generate_png  = args.iter().any(|a| a == "--png");
    let generate_json = args.iter().any(|a| a == "--json");
    let strip_tags    = args.iter().any(|a| a == "--strip-tags");

    let (
        title_opt,
        subtitle_opt,
        artist_opt,
        titletranslit_opt,
        subtitletranslit_opt,
        artisttranslit_opt,
        bpms_opt,
        notes_opt,
    ) = extract_sections(&simfile_data)?;

    let mut title_str = std::str::from_utf8(title_opt.unwrap_or(b"<invalid-title>"))
        .unwrap_or("<invalid-title>")
        .to_owned();
    if strip_tags {
        title_str = strip_title_tags(&title_str);
    }

    let subtitle_str = std::str::from_utf8(subtitle_opt.unwrap_or(b""))
        .unwrap_or("");
    let artist_str = std::str::from_utf8(artist_opt.unwrap_or(b"<invalid-artist>"))
        .unwrap_or("<invalid-artist>");
    let bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");
    let normalized_bpms = normalize_float_digits(bpms_raw);

    let titletranslit_str = std::str::from_utf8(titletranslit_opt.unwrap_or(b""))
        .unwrap_or("");
    let subtitletranslit_str = std::str::from_utf8(subtitletranslit_opt.unwrap_or(b""))
        .unwrap_or("");
    let artisttranslit_str = std::str::from_utf8(artisttranslit_opt.unwrap_or(b""))
        .unwrap_or("");

    let notes_bytes = notes_opt.unwrap_or(b"<invalid-notes>");
    let (fields, chart_data) = split_notes_fields(notes_bytes);
    if fields.len() < 5 {
        eprintln!("#NOTES section is incomplete.");
        std::process::exit(1);
    }

    let step_type_str  = std::str::from_utf8(fields[0]).unwrap_or("").trim();
    let difficulty_str = std::str::from_utf8(fields[2]).unwrap_or("").trim();
    let rating_str     = std::str::from_utf8(fields[3]).unwrap_or("").trim();

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

    let mut hasher = Sha1::new();
    hasher.update(&minimized_chart);
    hasher.update(normalized_bpms.as_bytes());
    let hash_result = hasher.finalize();

    let mut hash_hex = String::with_capacity(hash_result.len() * 2);
    for byte in hash_result {
        // Each byte => 2 hex characters
        use std::fmt::Write;
        write!(&mut hash_hex, "{:02x}", byte).expect("Unable to write");
    }
    let short_hash = &hash_hex[..16];

    let bpm_map = parse_bpm_map(&normalized_bpms);
    let (min_bpm, max_bpm) = compute_bpm_range(&bpm_map);

    let measure_nps_vec = compute_measure_nps_vec(&measure_densities, &bpm_map);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

    let total_length = compute_total_chart_length(&measure_densities, &bpm_map);

    // Convert minimized chart into bitmasks
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

    // Single-pass detection for all non-anchor patterns
    let detected_non_anchors = detect_all_patterns_non_anchors(&bitmasks);

    // Specialized anchor detection
    let (anchor_left, anchor_down, anchor_up, anchor_right) = count_anchors(&bitmasks);

    let elapsed = start_time.elapsed();

    // For convenience, define a small helper to fetch counts from the pattern detection map.
    fn c(d: &std::collections::HashMap<PatternVariant, u32>, v: PatternVariant) -> u32 {
        *d.get(&v).unwrap_or(&0)
    }

    if generate_json {
        println!("{{");
        println!("  \"title\": \"{}\",", escape_json(&title_str));
        println!("  \"title_translit\": \"{}\",", escape_json(titletranslit_str));
        println!("  \"subtitle\": \"{}\",", escape_json(subtitle_str));
        println!("  \"subtitle_translit\": \"{}\",", escape_json(subtitletranslit_str));
        println!("  \"artist\": \"{}\",", escape_json(artist_str));
        println!("  \"artist_translit\": \"{}\",", escape_json(artisttranslit_str));
        println!("  \"bpms\": \"{}\",", escape_json(&normalized_bpms));
        println!("  \"step_type\": \"{}\",", escape_json(step_type_str));
        println!("  \"difficulty\": \"{}\",", escape_json(difficulty_str));
        println!("  \"rating\": \"{}\",", escape_json(rating_str));
        println!("  \"hash_short\": \"{}\",", short_hash);

        // Arrow stats
        println!("  \"arrow_stats\": {{");
        println!("     \"left\": {},", stats.left);
        println!("     \"down\": {},", stats.down);
        println!("     \"up\": {},", stats.up);
        println!("     \"right\": {},", stats.right);
        println!("     \"total_arrows\": {},", stats.total_arrows);
        println!("     \"total_steps\": {},", stats.total_steps);
        println!("     \"jumps\": {},", stats.jumps);
        println!("     \"hands\": {},", stats.hands);
        println!("     \"holds\": {},", stats.holds);
        println!("     \"rolls\": {},", stats.rolls);
        println!("     \"mines\": {}", stats.mines);
        println!("  }},");

        // Stream counts
        println!("  \"stream_counts\": {{");
        println!("     \"run16_streams\": {},", stream_counts.run16_streams);
        println!("     \"run20_streams\": {},", stream_counts.run20_streams);
        println!("     \"run24_streams\": {},", stream_counts.run24_streams);
        println!("     \"run32_streams\": {},", stream_counts.run32_streams);
        println!("     \"total_streams\": {},", total_streams);
        println!("     \"total_breaks\": {}", stream_counts.total_breaks);
        println!("  }},");

        // Breakdown
        println!("  \"breakdown\": {{");
        println!("     \"detailed\": \"{}\",", escape_json(&detailed));
        println!("     \"partial\": \"{}\",", escape_json(&partial));
        println!("     \"simple\": \"{}\"", escape_json(&simple));
        println!("  }},");

        // BPM info
        println!("  \"bpm_info\": {{");
        println!("     \"min_bpm\": {:.2},", min_bpm);
        println!("     \"max_bpm\": {:.2},", max_bpm);
        println!("     \"chart_length_s\": {},", total_length);
        println!("     \"max_nps\": {:.4},", max_nps);
        println!("     \"median_nps\": {:.4}", median_nps);
        println!("  }},");

        // Pattern counts
        println!("  \"pattern_counts\": {{");
        // Candles
        println!("     \"candle_left\": {},", c(&detected_non_anchors, PatternVariant::CandleLeft));
        println!("     \"candle_right\": {},", c(&detected_non_anchors, PatternVariant::CandleRight));
        //TOTAL CANDLES, PERCENT, etc. (you can add if you want)
        
        // Boxes
        println!("     \"box_lr\": {},", c(&detected_non_anchors, PatternVariant::BoxLR));
        println!("     \"box_ud\": {},", c(&detected_non_anchors, PatternVariant::BoxUD));
        println!("     \"box_corner_ld\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerLD));
        println!("     \"box_corner_lu\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerLU));
        println!("     \"box_corner_rd\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerRD));
        println!("     \"box_corner_ru\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerRU));
        //TOTAL BOXES, etc.

        // Doritos
        println!("     \"dorito_right\": {},", c(&detected_non_anchors, PatternVariant::DoritoRight));
        println!("     \"dorito_left\": {},", c(&detected_non_anchors, PatternVariant::DoritoLeft));
        println!("     \"dorito_inv_right\": {},", c(&detected_non_anchors, PatternVariant::DoritoInvRight));
        println!("     \"dorito_inv_left\": {},", c(&detected_non_anchors, PatternVariant::DoritoInvLeft));
        //TOTAL DORITOS, etc.

        // Spirals
        println!("     \"left_spiral\": {},", c(&detected_non_anchors, PatternVariant::SpiralLeft));
        println!("     \"right_spiral\": {},", c(&detected_non_anchors, PatternVariant::SpiralRight));
        //TOTAL SPIRALS, etc.

        // Copters
        println!("     \"left_copter\": {},", c(&detected_non_anchors, PatternVariant::CopterLeft));
        println!("     \"right_copter\": {},", c(&detected_non_anchors, PatternVariant::CopterRight));
        //TOTAL COPTERS, etc.

        // Luchi
        println!("     \"left_luchi\": {},", c(&detected_non_anchors, PatternVariant::LuchiLeft));
        println!("     \"right_luchi\": {},", c(&detected_non_anchors, PatternVariant::LuchiRight));
        //TOTAL LUCHI, etc.

        // Hip-Breakers
        println!("     \"left_hip_breaker\": {},", c(&detected_non_anchors, PatternVariant::HipBreakerLeft));
        println!("     \"right_hip_breaker\": {},", c(&detected_non_anchors, PatternVariant::HipBreakerRight));
        //TOTAL HIPBREAKERS, etc.

        // Sweeps
        println!("     \"left_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepLeft));
        println!("     \"right_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepRight));
        println!("     \"left_inv_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepInvLeft));
        println!("     \"right_inv_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepInvRight));
        //TOTAL SWEEPS, etc.

        // Anchors
        println!("     \"anchor_left\": {},", anchor_left);
        println!("     \"anchor_down\": {},", anchor_down);
        println!("     \"anchor_up\": {},", anchor_up);
        println!("     \"anchor_right\": {}", anchor_right);
        //TOTAL ANCHORS, etc.
        println!("  }},");

        // Elapsed
        println!("  \"elapsed\": \"{:?}\"", elapsed);
        println!("}}");
    } else {
        // ---------------- TEXT OUTPUT ----------------
        println!("Title: {}", title_str);
        println!("Title translate: {}", titletranslit_str);
        println!("Subtitle: {}", subtitle_str);
        println!("Subtitle translate: {}", subtitletranslit_str);
        println!("Artist: {}", artist_str);
        println!("Artist translate: {}", artisttranslit_str);
        println!("Normalized BPMs: {}", normalized_bpms);
        println!("Steptype: {}", step_type_str);
        println!("Difficulty: {}", difficulty_str);
        println!("Rating: {}", rating_str);
        println!("Hash (first 16 hex chars): {}", short_hash);

        println!("--- Arrow Stats ---");
        println!("Left: {}", stats.left);
        println!("Down: {}", stats.down);
        println!("Up: {}", stats.up);
        println!("Right: {}", stats.right);
        println!("Total arrows: {}", stats.total_arrows);
        println!("Total steps: {}", stats.total_steps);
        println!("Jumps (2-arrow steps): {}", stats.jumps);
        println!("Hands (3+ arrow steps): {}", stats.hands);
        println!("Holds: {}", stats.holds);
        println!("Rolls: {}", stats.rolls);
        println!("Mines: {}", stats.mines);

        println!("--- Stream Counts ---");
        println!("16th streams: {}", stream_counts.run16_streams);
        println!("20th streams: {}", stream_counts.run20_streams);
        println!("24th streams: {}", stream_counts.run24_streams);
        println!("32nd streams: {}", stream_counts.run32_streams);
        println!("Total streams: {}", total_streams);
        println!("Total breaks: {}", stream_counts.total_breaks);

        println!("Detailed breakdown: {}", detailed);
        println!("Partially simplified: {}", partial);
        println!("Simplified breakdown: {}", simple);

        println!("--- Additional Chart Info ---");
        println!("Min BPM: {:.2}", min_bpm);
        println!("Max BPM: {:.2}", max_bpm);
        println!("Chart length (seconds): {}", total_length);
        println!("Max NPS: {:.2}", max_nps);
        println!("Median NPS: {:.2}", median_nps);

        println!("--- Pattern Counts (non-anchors) ---");
        println!("CandleLeft: {}",      c(&detected_non_anchors, PatternVariant::CandleLeft));
        println!("CandleRight: {}",     c(&detected_non_anchors, PatternVariant::CandleRight));
        println!("BoxLR: {}",           c(&detected_non_anchors, PatternVariant::BoxLR));
        println!("BoxUD: {}",           c(&detected_non_anchors, PatternVariant::BoxUD));
        println!("BoxCornerLD: {}",     c(&detected_non_anchors, PatternVariant::BoxCornerLD));
        println!("BoxCornerLU: {}",     c(&detected_non_anchors, PatternVariant::BoxCornerLU));
        println!("BoxCornerRD: {}",     c(&detected_non_anchors, PatternVariant::BoxCornerRD));
        println!("BoxCornerRU: {}",     c(&detected_non_anchors, PatternVariant::BoxCornerRU));
        println!("DoritoRight: {}",     c(&detected_non_anchors, PatternVariant::DoritoRight));
        println!("DoritoLeft: {}",      c(&detected_non_anchors, PatternVariant::DoritoLeft));
        println!("DoritoInvRight: {}",  c(&detected_non_anchors, PatternVariant::DoritoInvRight));
        println!("DoritoInvLeft: {}",   c(&detected_non_anchors, PatternVariant::DoritoInvLeft));
        println!("LeftSpiral: {}",      c(&detected_non_anchors, PatternVariant::SpiralLeft));
        println!("RightSpiral: {}",     c(&detected_non_anchors, PatternVariant::SpiralRight));
        println!("LeftCopter: {}",      c(&detected_non_anchors, PatternVariant::CopterLeft));
        println!("RightCopter: {}",     c(&detected_non_anchors, PatternVariant::CopterRight));
        println!("LeftLuchi: {}",       c(&detected_non_anchors, PatternVariant::LuchiLeft));
        println!("RightLuchi: {}",      c(&detected_non_anchors, PatternVariant::LuchiRight));
        println!("LeftHipBreaker: {}",  c(&detected_non_anchors, PatternVariant::HipBreakerLeft));
        println!("RightHipBreaker: {}", c(&detected_non_anchors, PatternVariant::HipBreakerRight));
        println!("LeftSweep: {}",       c(&detected_non_anchors, PatternVariant::SweepLeft));
        println!("RightSweep: {}",      c(&detected_non_anchors, PatternVariant::SweepRight));
        println!("LeftInvSweep: {}",    c(&detected_non_anchors, PatternVariant::SweepInvLeft));
        println!("RightInvSweep: {}",   c(&detected_non_anchors, PatternVariant::SweepInvRight));

        // Anchors
        println!("--- Anchors ---");
        println!("AnchorLeft: {}",  anchor_left);
        println!("AnchorDown: {}",  anchor_down);
        println!("AnchorUp: {}",    anchor_up);
        println!("AnchorRight: {}", anchor_right);

        println!("---");
        println!("Elapsed time: {:?}", elapsed);
    }

    if generate_png {
        generate_density_graph_png(&measure_nps_vec, max_nps, short_hash)?;
    }

    Ok(())
}

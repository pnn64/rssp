use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::Instant;
use std::fmt::Write; // for normalize_float_digits
use sha1::{Digest, Sha1};

/// All arrow/step-related counts.
#[derive(Default)]
struct ArrowStats {
    total_arrows: u32,
    left: u32,
    down: u32,
    up: u32,
    right: u32,
    total_steps: u32,
    jumps: u32,
    hands: u32,
    mines: u32,
    holds: u32,
    rolls: u32,
}

/// Tracks how many dense measures appear at each run level.
#[derive(Default)]
struct StreamCounts {
    run16_streams: u32,
    run20_streams: u32,
    run24_streams: u32,
    run32_streams: u32,
    total_breaks: u32,
}

/// A measure’s “density” category.
#[derive(Debug, Clone, Copy, PartialEq)]
enum RunDensity {
    Run32,
    Run24,
    Run20,
    Run16,
    Break,
}

/// Which kind of breakdown are we generating?
#[derive(Debug, Clone, Copy, PartialEq)]
enum BreakdownMode {
    Detailed,
    Partial,
    Simplified,
}

// --------------------------------------------------------------------
// Minimization & Counting
// --------------------------------------------------------------------

#[inline(always)]
fn is_all_zero(line: &[u8; 4]) -> bool {
    line.iter().all(|&b| b == b'0')
}

#[inline(always)]
fn minimize_measure(measure: &mut Vec<[u8; 4]>) {
    while measure.len() >= 2 && measure.len() % 2 == 0 {
        if (1..measure.len()).step_by(2).any(|i| !is_all_zero(&measure[i])) {
            break;
        }
        let half_len = measure.len() / 2;
        for i in 0..half_len {
            measure[i] = measure[i * 2];
        }
        measure.truncate(half_len);
    }

    // If everything is zero, keep only 1 line
    if !measure.is_empty() && measure.iter().all(is_all_zero) {
        measure.truncate(1);
    }
}

#[inline(always)]
fn count_line(line: &[u8; 4], stats: &mut ArrowStats) -> bool {
    let mut pressed = 0u32;
    for &ch in line {
        match ch {
            b'1' => pressed += 1,
            b'2' => {
                stats.holds += 1;
                pressed += 1;
            }
            b'4' => {
                stats.rolls += 1;
                pressed += 1;
            }
            b'M' | b'm' => {
                stats.mines += 1;
            }
            _ => {}
        }
    }

    // Column-based counting
    if line[0] == b'1' || line[0] == b'2' || line[0] == b'4' {
        stats.left += 1;
    }
    if line[1] == b'1' || line[1] == b'2' || line[1] == b'4' {
        stats.down += 1;
    }
    if line[2] == b'1' || line[2] == b'2' || line[2] == b'4' {
        stats.up += 1;
    }
    if line[3] == b'1' || line[3] == b'2' || line[3] == b'4' {
        stats.right += 1;
    }

    if pressed > 0 {
        stats.total_steps += 1;
    }
    if pressed == 2 {
        stats.jumps += 1;
    } else if pressed >= 3 {
        stats.hands += 1;
    }
    stats.total_arrows += pressed;

    pressed > 0
}

/// Minimizes chart + counts arrows, returning (final chart bytes, arrow stats, measure densities).
fn minimize_chart_and_count(notes_data: &[u8]) -> (Vec<u8>, ArrowStats, Vec<usize>) {
    let mut output = Vec::with_capacity(notes_data.len());
    let mut measure = Vec::with_capacity(64);

    let mut stats = ArrowStats::default();
    let mut measure_densities = Vec::new();
    let mut saw_semicolon = false;

    #[inline(always)]
    fn finalize_measure(
        measure: &mut Vec<[u8; 4]>,
        output: &mut Vec<u8>,
        stats: &mut ArrowStats,
        measure_densities: &mut Vec<usize>,
    ) {
        if measure.is_empty() {
            measure_densities.push(0);
            return;
        }
        minimize_measure(measure);
        output.reserve(measure.len() * 5);

        let mut density = 0usize;
        for mline in measure.iter() {
            if count_line(mline, stats) {
                density += 1;
            }
            output.extend_from_slice(mline);
            output.push(b'\n');
        }
        measure.clear();
        measure_densities.push(density);
    }

    for line in notes_data.split(|&b| b == b'\n') {
        if line.is_empty() {
            continue;
        }
        match line[0] {
            b',' => {
                finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
                output.extend_from_slice(b",\n");
            }
            b';' => {
                finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
                saw_semicolon = true;
                break;
            }
            b' ' => {
                // skip lines of only spaces
            }
            _ => {
                if line.len() < 4 {
                    // skip malformed lines
                    continue;
                }
                let mut arr = [0u8; 4];
                arr.copy_from_slice(&line[..4]);
                measure.push(arr);
            }
        }
    }

    if !saw_semicolon && !measure.is_empty() {
        finalize_measure(&mut measure, &mut output, &mut stats, &mut measure_densities);
    }

    // remove trailing ",\n"
    if output.ends_with(&[b',', b'\n']) {
        output.truncate(output.len() - 2);
    }

    (output, stats, measure_densities)
}

#[inline]
fn categorize_measure_density(d: usize) -> RunDensity {
    match d {
        d if d >= 32 => RunDensity::Run32,
        d if d >= 24 => RunDensity::Run24,
        d if d >= 20 => RunDensity::Run20,
        d if d >= 16 => RunDensity::Run16,
        _ => RunDensity::Break,
    }
}

#[inline]
fn compute_stream_counts(measure_densities: &[usize]) -> StreamCounts {
    let mut sc = StreamCounts::default();
    for &d in measure_densities {
        match categorize_measure_density(d) {
            RunDensity::Run32 => sc.run32_streams += 1,
            RunDensity::Run24 => sc.run24_streams += 1,
            RunDensity::Run20 => sc.run20_streams += 1,
            RunDensity::Run16 => sc.run16_streams += 1,
            RunDensity::Break => sc.total_breaks += 1,
        }
    }
    sc
}

// --------------------------------------------------------------------
// Single function for all 3 breakdowns
// --------------------------------------------------------------------

/// A token for run or break.
#[derive(Debug)]
enum Token {
    Run(RunDensity, usize), // e.g. (Run16, length=3)
    Break(usize),           // e.g. (5)
}

#[inline]
fn format_run_symbol(cat: RunDensity, length: usize, star: bool) -> String {
    let base = match cat {
        RunDensity::Run16 => format!("{}", length),
        RunDensity::Run20 => format!("~{}~", length),
        RunDensity::Run24 => format!(r"\{}\", length),
        RunDensity::Run32 => format!("={}=", length),
        RunDensity::Break => unreachable!(),
    };
    if star {
        format!("{}*", base)
    } else {
        base
    }
}

/// The single function that builds tokens, merges them based on `BreakdownMode`,
/// and outputs the final string.
fn generate_breakdown(measure_densities: &[usize], mode: BreakdownMode) -> String {
    // 1) Convert measure densities -> category
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    // 2) skip leading/trailing breaks
    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return String::new();
    }

    // 3) Build run-length tokens
    let mut tokens = Vec::new();
    {
        let mut i = first_run.unwrap();
        let end = last_run.unwrap();
        while i <= end {
            let c = cats[i];
            let mut length = 1;
            let mut j = i + 1;
            while j <= end && cats[j] == c {
                length += 1;
                j += 1;
            }
            if c == RunDensity::Break {
                tokens.push(Token::Break(length));
            } else {
                tokens.push(Token::Run(c, length));
            }
            i = j;
        }
    }

    // 4) We'll produce the final tokens by merging if needed
    let mut output = Vec::new();
    let mut idx = 0;

    while idx < tokens.len() {
        match tokens[idx] {
            Token::Run(cat, mut run_len) => {
                let mut star = false;

                // Decide how to merge based on mode
                // Detailed => no merges
                // Partial => merges break==1, same cat
                // Simplified => merges break<=4, same cat
                if mode != BreakdownMode::Detailed {
                    // Repeatedly try merges:
                    while idx + 2 < tokens.len() {
                        let can_merge = match (&tokens[idx + 1], &tokens[idx + 2]) {
                            (Token::Break(bk_len), Token::Run(next_cat, next_len)) => {
                                if cat == *next_cat {
                                    match mode {
                                        BreakdownMode::Partial => {
                                            // break==1 => Some(1)
                                            if *bk_len == 1 {
                                                // merge run_len += bk_len + next_len
                                                run_len += bk_len + *next_len;
                                                true
                                            } else {
                                                false
                                            }
                                        }
                                        BreakdownMode::Simplified => {
                                            // break<=4 => Some(bk_len)
                                            if *bk_len <= 4 {
                                                run_len += bk_len + *next_len;
                                                true
                                            } else {
                                                false
                                            }
                                        }
                                        BreakdownMode::Detailed => false,
                                    }
                                } else {
                                    false
                                }
                            }
                            _ => false,
                        };
                        if can_merge {
                            star = true;
                            idx += 2; // skip those tokens
                        } else {
                            break;
                        }
                    }
                }

                // Now push the run token
                let s = format_run_symbol(cat, run_len, star);
                output.push(s);

                idx += 1; // consumed
            }
            Token::Break(bk_len) => {
                // Format leftover breaks. 
                // Detailed => skip single break, if bk_len>1 => (bk_len)
                // Partial => bk_len<=4 => "-", <=32=>"/", else "|"
                // Simplified => bk_len<=4 => skip, <=32=>"/", else "|"

                match mode {
                    BreakdownMode::Detailed => {
                        if bk_len > 1 {
                            output.push(format!("({})", bk_len));
                        }
                    }
                    BreakdownMode::Partial => {
                        if bk_len <= 4 {
                            output.push("-".to_string());
                        } else if bk_len <= 32 {
                            output.push("/".to_string());
                        } else {
                            output.push("|".to_string());
                        }
                    }
                    BreakdownMode::Simplified => {
                        if bk_len <= 4 {
                            // skip
                        } else if bk_len <= 32 {
                            output.push("/".to_string());
                        } else {
                            output.push("|".to_string());
                        }
                    }
                }

                idx += 1;
            }
        }
    }
    output.join(" ")
}

// --------------------------------------------------------------------
// Normalizes BPM floats
// --------------------------------------------------------------------
#[inline]
fn normalize_float_digits(param: &str) -> String {
    let mut output = String::with_capacity(param.len());
    let mut first = true;
    for beat_bpm in param.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !first {
            output.push(',');
        } else {
            first = false;
        }

        let mut eq_split = beat_bpm.split('=');
        let beat_str = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());
        let bpm_str  = eq_split.next().unwrap_or("").trim_matches(|c: char| c.is_control());

        if let (Ok(beat_val), Ok(bpm_val)) = (beat_str.parse::<f64>(), bpm_str.parse::<f64>()) {
            let beat_rounded = (beat_val * 1000.0).round() / 1000.0;
            let bpm_rounded  = (bpm_val * 1000.0).round() / 1000.0;
            let _ = write!(&mut output, "{:.3}={:.3}", beat_rounded, bpm_rounded);
        } else {
            output.push_str(beat_bpm);
        }
    }
    output
}

// --------------------------------------------------------------------
// Extract sections
// --------------------------------------------------------------------
#[inline(always)]
fn extract_sections(
    data: &[u8],
) -> io::Result<(
    Option<&[u8]>, // title
    Option<&[u8]>, // subtitle
    Option<&[u8]>, // artist
    Option<&[u8]>, // titletranslit
    Option<&[u8]>, // subtitletranslit
    Option<&[u8]>, // artisttranslit
    Option<&[u8]>, // bpms
    Option<&[u8]>, // notes
)> {
    let mut title = None;
    let mut subtitle = None;
    let mut artist = None;
    let mut titletranslit = None;
    let mut subtitletranslit = None;
    let mut artisttranslit = None;
    let mut bpms = None;
    let mut notes = None;

    let mut i = 0;
    while i < data.len() {
        if title.is_some() && subtitle.is_some() && artist.is_some() && bpms.is_some() && notes.is_some() {
            break;
        }

        #[inline(always)]
        fn parse_tag<'a>(
            data: &'a [u8], 
            idx: &mut usize, 
            tag_len: usize
        ) -> Option<&'a [u8]> {
            let start_idx = *idx + tag_len;
            if start_idx > data.len() {
                return None;
            }
            if let Some(end_off) = data[start_idx..].iter().position(|&b| b == b';') {
                let result = &data[start_idx..start_idx + end_off];
                *idx = start_idx + end_off + 1;
                Some(result)
            } else {
                None
            }
        }

        let slice = &data[i..];
        if slice.starts_with(b"#TITLE:") && title.is_none() {
            title = parse_tag(data, &mut i, b"#TITLE:".len());
            continue;
        } else if slice.starts_with(b"#SUBTITLE:") && subtitle.is_none() {
            subtitle = parse_tag(data, &mut i, b"#SUBTITLE:".len());
            continue;
        } else if slice.starts_with(b"#ARTIST:") && artist.is_none() {
            artist = parse_tag(data, &mut i, b"#ARTIST:".len());
            continue;
        } else if slice.starts_with(b"#TITLETRANSLIT:") && titletranslit.is_none() {
            titletranslit = parse_tag(data, &mut i, b"#TITLETRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#SUBTITLETRANSLIT:") && subtitletranslit.is_none() {
            subtitletranslit = parse_tag(data, &mut i, b"#SUBTITLETRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#ARTISTTRANSLIT:") && artisttranslit.is_none() {
            artisttranslit = parse_tag(data, &mut i, b"#ARTISTTRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#BPMS:") && bpms.is_none() {
            bpms = parse_tag(data, &mut i, b"#BPMS:".len());
            continue;
        } else if slice.starts_with(b"#NOTES:") && notes.is_none() {
            let start_idx = i + b"#NOTES:".len();
            if start_idx < data.len() {
                notes = Some(&data[start_idx..]);
            }
            break;
        }
        i += 1;
    }

    Ok((
        title,
        subtitle,
        artist,
        titletranslit,
        subtitletranslit,
        artisttranslit,
        bpms,
        notes,
    ))
}

#[inline]
fn split_notes_fields<'a>(notes_block: &'a [u8]) -> (Vec<&'a [u8]>, &'a [u8]) {
    let mut fields = Vec::with_capacity(5);
    let mut colon_count = 0;
    let mut start = 0;
    for (i, &b) in notes_block.iter().enumerate() {
        if b == b':' {
            fields.push(&notes_block[start..i]);
            start = i + 1;
            colon_count += 1;
            if colon_count == 5 {
                let remainder = &notes_block[start..];
                return (fields, remainder);
            }
        }
    }
    (fields, &notes_block[notes_block.len()..])
}

// --------------------------------------------------------------------
// Main
// --------------------------------------------------------------------

fn main() -> io::Result<()> {
    let before = Instant::now();
    let args: Vec<String> = args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path>", args[0]);
        std::process::exit(1);
    }

    let simfile_path = &args[1];
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
        bpms_opt,
        notes_opt,
    ) = extract_sections(&simfile_data)?;

    let title_str = std::str::from_utf8(title_opt.unwrap_or(b"<invalid-title>"))
        .unwrap_or("<invalid-title>");
    let subtitle_str = std::str::from_utf8(subtitle_opt.unwrap_or(b"<invalid-subtitle>"))
        .unwrap_or("<invalid-subtitle>");
    let artist_str = std::str::from_utf8(artist_opt.unwrap_or(b"<invalid-artist>"))
        .unwrap_or("<invalid-artist>");
    let bpms_raw = std::str::from_utf8(bpms_opt.unwrap_or(b"<invalid-bpms>"))
        .unwrap_or("<invalid-bpms>");
    let normalized_bpms = normalize_float_digits(bpms_raw);

    // Handle transliterated fields
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

    // Minimize + count arrows
    let (mut minimized_chart, stats, measure_densities) = minimize_chart_and_count(chart_data);

    // remove trailing newlines
    if let Some(pos) = minimized_chart.iter().rposition(|&b| b != b'\n') {
        minimized_chart.truncate(pos + 1);
    }

    // Compute stream counts
    let stream_counts = compute_stream_counts(&measure_densities);

    // Generate breakdowns
    let detailed = generate_breakdown(&measure_densities, BreakdownMode::Detailed);
    let partial  = generate_breakdown(&measure_densities, BreakdownMode::Partial);
    let simple   = generate_breakdown(&measure_densities, BreakdownMode::Simplified);

    // Build hash
    let mut hasher = Sha1::new();
    hasher.update(&minimized_chart);
    hasher.update(normalized_bpms.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex = hex::encode(hash_result);
    let short_hash = &hash_hex[..16];

    // Print
    println!("Elapsed time: {:.2?}", before.elapsed());
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
    println!("Left: {}",   stats.left);
    println!("Down: {}",   stats.down);
    println!("Up: {}",     stats.up);
    println!("Right: {}",  stats.right);
    println!("Total arrows: {}", stats.total_arrows);
    println!("Total steps: {}",  stats.total_steps);
    println!("Jumps (2-arrow steps): {}", stats.jumps);
    println!("Hands (3+ arrow steps): {}", stats.hands);
    println!("Holds: {}", stats.holds);
    println!("Rolls: {}", stats.rolls);
    println!("Mines: {}", stats.mines);

    println!("--- Stream Counts ---");
    println!("16th streams: {}",  stream_counts.run16_streams);
    println!("20th streams: {}",  stream_counts.run20_streams);
    println!("24th streams: {}",  stream_counts.run24_streams);
    println!("32nd streams: {}",  stream_counts.run32_streams);
    println!("Total breaks: {}",  stream_counts.total_breaks);

    println!("Detailed breakdown:      {}", detailed);
    println!("Partially simplified:    {}", partial);
    println!("Simplified breakdown:    {}", simple);

    Ok(())
}

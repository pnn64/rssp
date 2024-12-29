use std::env::args;
use std::fs::File;
use std::io;
use std::io::Read;
use std::time::Instant;
use std::fmt::Write; // for normalize_float_digits
use sha1::{Digest, Sha1};
use image::{ImageBuffer, Rgb, RgbImage, ImageError};

/// Strip bracketed numeric tags (e.g. [16] [300]) and leading numeric prefixes (e.g. "8. - ")
/// from a title string.
fn strip_title_tags(title: &str) -> String {
    let mut s = title.trim_start();

    loop {
        if s.starts_with('[') {
            if let Some(end_bracket) = s.find(']') {
                let tag_content = &s[1..end_bracket];
                // Only strip if the bracket contents are digits or periods (e.g. [16], [300], [2.5])
                if tag_content.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    // Advance past the bracket and trim again
                    s = s[end_bracket + 1..].trim_start();
                    continue;
                }
            }
        } else {
            // Also strip leading numeric prefixes like "8. - "
            let mut chars = s.char_indices();
            let mut pos = 0;
            while let Some((i, c)) = chars.next() {
                if c.is_ascii_digit() || c == '.' {
                    pos = i + c.len_utf8();
                } else {
                    break;
                }
            }
            if pos > 0 && s[pos..].starts_with("- ") {
                s = s[pos + 2..].trim_start();
                continue;
            }
        }
        break;
    }

    s.to_string()
}

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

/// Pattern stats (foot candles, anchors, boxes, etc.).
#[derive(Default)]
struct PatternStats {
    left_foot_candles: u32,
    right_foot_candles: u32,
    total_candles: u32,
    candles_percent: f64,
    ld_ru_mono: u32,
    lu_rd_mono: u32,
    mono_percent: f64,
    lr_boxes: u32,
    ud_boxes: u32,
    corner_ld_boxes: u32,
    corner_lu_boxes: u32,
    corner_rd_boxes: u32,
    corner_ru_boxes: u32,
    anchor_left: u32,
    anchor_down: u32,
    anchor_up: u32,
    anchor_right: u32,
    right_dorito: u32,
    left_dorito: u32,
    inv_right_dorito: u32,
    inv_left_dorito: u32,
}

// --------------------------------------------------------------------
// Minimization & Counting
// --------------------------------------------------------------------

#[inline]
fn is_all_zero(line: &[u8; 4]) -> bool {
    line.iter().all(|&b| b == b'0')
}

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

    #[inline]
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

fn compute_stream_counts(measure_densities: &[usize]) -> StreamCounts {
    let mut sc = StreamCounts::default();

    // First, convert measures to their density category
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    // Find the first measure that isn't a break
    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    // Find the last measure that isn't a break
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);

    // If everything is a break (or empty), just return defaults
    if first_run.is_none() || last_run.is_none() {
        return sc;
    }

    let start_idx = first_run.unwrap();
    let end_idx   = last_run.unwrap();

    // Only count categories from first_run..=last_run
    for &cat in &cats[start_idx..=end_idx] {
        match cat {
            RunDensity::Run16 => sc.run16_streams += 1,
            RunDensity::Run20 => sc.run20_streams += 1,
            RunDensity::Run24 => sc.run24_streams += 1,
            RunDensity::Run32 => sc.run32_streams += 1,
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

/// The single function that builds tokens, merges them based on BreakdownMode,
/// and outputs the final string.
fn generate_breakdown(measure_densities: &[usize], mode: BreakdownMode) -> String {
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    // skip leading/trailing breaks
    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return String::new();
    }

    // build run-length tokens
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

    // merges if needed
    let mut output = Vec::new();
    let mut idx = 0;

    while idx < tokens.len() {
        match tokens[idx] {
            Token::Run(cat, mut run_len) => {
                let mut star = false;
                if mode != BreakdownMode::Detailed {
                    while idx + 2 < tokens.len() {
                        let can_merge = match (&tokens[idx + 1], &tokens[idx + 2]) {
                            (Token::Break(bk_len), Token::Run(next_cat, next_len)) => {
                                if cat == *next_cat {
                                    match mode {
                                        BreakdownMode::Partial => {
                                            if *bk_len == 1 {
                                                run_len += bk_len + *next_len;
                                                true
                                            } else {
                                                false
                                            }
                                        }
                                        BreakdownMode::Simplified => {
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
                            idx += 2;
                        } else {
                            break;
                        }
                    }
                }
                let s = format_run_symbol(cat, run_len, star);
                output.push(s);
                idx += 1;
            }
            Token::Break(bk_len) => {
                match mode {
                    BreakdownMode::Detailed => {
                        if bk_len > 1 {
                            output.push(format!("({})", bk_len));
                        }
                    }
                    BreakdownMode::Partial => {
                        if bk_len == 1 {
                            // skip
                        } else if bk_len <= 4 {
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
// BPM utilities
// --------------------------------------------------------------------

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

fn parse_bpm_map(normalized_bpms: &str) -> Vec<(f64, f64)> {
    let mut bpms_vec = Vec::new();
    for chunk in normalized_bpms.split(',') {
        let chunk = chunk.trim();
        if let Some(eq_pos) = chunk.find('=') {
            let left = &chunk[..eq_pos].trim();
            let right = &chunk[eq_pos + 1..].trim();
            if let (Ok(beat), Ok(bpm)) = (left.parse::<f64>(), right.parse::<f64>()) {
                bpms_vec.push((beat, bpm));
            }
        }
    }
    bpms_vec.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    bpms_vec
}

/// Returns the BPM in effect at a given beat
fn get_current_bpm(beat: f64, bpm_map: &[(f64, f64)]) -> f64 {
    let mut curr_bpm = if !bpm_map.is_empty() { bpm_map[0].1 } else { 0.0 };
    for &(b_beat, b_bpm) in bpm_map {
        if beat >= b_beat {
            curr_bpm = b_bpm;
        } else {
            break;
        }
    }
    curr_bpm
}

/// Compute min_bpm/max_bpm from the entire BPM map (or (0,0) if empty).
fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (f64, f64) {
    if bpm_map.is_empty() {
        return (0.0, 0.0);
    }
    let mut min_bpm = f64::MAX;
    let mut max_bpm = f64::MIN;
    for &(_, bpm) in bpm_map {
        if bpm < min_bpm {
            min_bpm = bpm;
        }
        if bpm > max_bpm {
            max_bpm = bpm;
        }
    }
    (min_bpm, max_bpm)
}

// --------------------------------------------------------------------
// Chart length (in seconds, int).
// --------------------------------------------------------------------

fn compute_total_chart_length(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> i32 {
    let mut total_length_seconds = 0.0;
    for (i, _) in measure_densities.iter().enumerate() {
        let measure_start_beat = i as f64 * 4.0;
        let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
        if curr_bpm <= 0.0 {
            continue;
        }
        let measure_length_s = (4.0 / curr_bpm) * 60.0;
        total_length_seconds += measure_length_s;
    }
    total_length_seconds.round() as i32
}

// --------------------------------------------------------------------
// NPS calculations
// --------------------------------------------------------------------

/// Computes a per-measure NPS vector (notes-per-second) from measure densities.
fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    let mut measure_nps_vec = Vec::with_capacity(measure_densities.len());
    for (i, &density) in measure_densities.iter().enumerate() {
        let measure_start_beat = i as f64 * 4.0;
        let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
        if curr_bpm <= 0.0 {
            measure_nps_vec.push(0.0);
            continue;
        }
        // measure_nps = (notes in measure) / measure duration
        // measure duration = 4 beats / curr_bpm => * 60 for sec => so 4/curr_bpm*60.
        // dividing density by measure length => density / (4/curr_bpm*60) => density*(curr_bpm/4)/60
        let measure_nps = density as f64 * (curr_bpm / 4.0) / 60.0;
        measure_nps_vec.push(measure_nps);
    }
    measure_nps_vec
}

/// Returns (max_nps, median_nps) from the measure_nps_vec.
fn get_nps_stats(measure_nps_vec: &[f64]) -> (f64, f64) {
    let max_nps = if measure_nps_vec.is_empty() {
        0.0
    } else {
        measure_nps_vec.iter().fold(f64::MIN, |a, &b| a.max(b))
    };
    let median_nps = median(measure_nps_vec);
    (max_nps, median_nps)
}

// --------------------------------------------------------------------
// Extract sections
// --------------------------------------------------------------------
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
        if title.is_some()
            && subtitle.is_some()
            && artist.is_some()
            && bpms.is_some()
            && notes.is_some()
        {
            break;
        }

        #[inline]
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
// Compute median of a slice of f64
// --------------------------------------------------------------------
fn median(arr: &[f64]) -> f64 {
    if arr.is_empty() {
        return 0.0;
    }
    let mut sorted = arr.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let len = sorted.len();
    if len % 2 == 0 {
        (sorted[len / 2 - 1] + sorted[len / 2]) / 2.0
    } else {
        sorted[len / 2]
    }
}

// --------------------------------------------------------------------
// Pattern Analysis
// --------------------------------------------------------------------

#[inline]
fn line_to_bitmask(line: &[u8]) -> u8 {
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
    mask
}

/// Parse lines from minimized chart => produce a Vec<u8> of bitmasks.
fn parse_bitmask_chart(chart_data: &[u8]) -> Vec<u8> {
    let mut bitmasks = Vec::new();
    for line in chart_data.split(|&b| b == b'\n') {
        if line.len() >= 4 {
            let m = line_to_bitmask(line);
            // Also check if it's not just commas or spaces
            if m != 0 || line.iter().any(|&b| !(b == b',' || b == b' ')) {
                bitmasks.push(m);
            }
        }
    }
    bitmasks
}

fn count_candles(bitmasks: &[u8]) -> (u32, u32) {
    const LEFT_FOOT_PATTERNS: &[[u8; 3]] = &[
        [0b0010, 0b1000, 0b0100],
        [0b0100, 0b1000, 0b0010],
    ];
    const RIGHT_FOOT_PATTERNS: &[[u8; 3]] = &[
        [0b0010, 0b0001, 0b0100],
        [0b0100, 0b0001, 0b0010],
    ];

    let mut left_foot = 0u32;
    let mut right_foot = 0u32;

    if bitmasks.len() < 3 {
        return (0, 0);
    }

    for i in 0..(bitmasks.len() - 2) {
        let block = [bitmasks[i], bitmasks[i + 1], bitmasks[i + 2]];
        if LEFT_FOOT_PATTERNS.iter().any(|pat| *pat == block) {
            left_foot += 1;
        }
        if RIGHT_FOOT_PATTERNS.iter().any(|pat| *pat == block) {
            right_foot += 1;
        }
    }
    (left_foot, right_foot)
}

fn count_monos(bitmasks: &[u8]) -> (u32, u32) {
    const VALID_LD_RU: &[[u8; 4]] = &[
        [0b0001, 0b0100, 0b0010, 0b1000],
        [0b0001, 0b1000, 0b0010, 0b0100],
        [0b0010, 0b0100, 0b0001, 0b1000],
        [0b0010, 0b1000, 0b0001, 0b0100],
        [0b0100, 0b0001, 0b1000, 0b0010],
        [0b0100, 0b0010, 0b1000, 0b0001],
        [0b1000, 0b0001, 0b0100, 0b0010],
        [0b1000, 0b0010, 0b0100, 0b0001],
    ];
    const VALID_LU_RD: &[[u8; 4]] = &[
        [0b0001, 0b0010, 0b0100, 0b1000],
        [0b0001, 0b1000, 0b0100, 0b0010],
        [0b0100, 0b0010, 0b0001, 0b1000],
        [0b0100, 0b1000, 0b0001, 0b0010],
        [0b0010, 0b0001, 0b1000, 0b0100],
        [0b0010, 0b0100, 0b1000, 0b0001],
        [0b1000, 0b0001, 0b0010, 0b0100],
        [0b1000, 0b0100, 0b0010, 0b0001],
    ];

    let mut ld_ru = 0u32;
    let mut lu_rd = 0u32;

    if bitmasks.len() < 4 {
        return (0, 0);
    }

    let mut i = 0;
    while i + 3 < bitmasks.len() {
        let block = &bitmasks[i..i + 4];
        if block.iter().all(|&b| b.count_ones() == 1) {
            if VALID_LD_RU.iter().any(|pat| pat == block) {
                ld_ru += 1;
                i += 4;
                continue;
            } else if VALID_LU_RD.iter().any(|pat| pat == block) {
                lu_rd += 1;
                i += 4;
                continue;
            }
        }
        i += 1;
    }
    (ld_ru, lu_rd)
}

fn count_boxes(bitmasks: &[u8]) -> (u32, u32, u32, u32, u32, u32) {
    const LR_BOXES: &[[u8; 4]] = &[
        [0b0001, 0b1000, 0b0001, 0b1000],
        [0b1000, 0b0001, 0b1000, 0b0001],
    ];
    const UD_BOXES: &[[u8; 4]] = &[
        [0b0100, 0b0010, 0b0100, 0b0010],
        [0b0010, 0b0100, 0b0010, 0b0100],
    ];
    const CORNER_LD_BOXES: &[[u8; 4]] = &[
        [0b0001, 0b0010, 0b0001, 0b0010],
        [0b0010, 0b0001, 0b0010, 0b0001],
    ];
    const CORNER_LU_BOXES: &[[u8; 4]] = &[
        [0b0001, 0b0100, 0b0001, 0b0100],
        [0b0100, 0b0001, 0b0100, 0b0001],
    ];
    const CORNER_RD_BOXES: &[[u8; 4]] = &[
        [0b1000, 0b0010, 0b1000, 0b0010],
        [0b0010, 0b1000, 0b0010, 0b1000],
    ];
    const CORNER_RU_BOXES: &[[u8; 4]] = &[
        [0b1000, 0b0100, 0b1000, 0b0100],
        [0b0100, 0b1000, 0b0100, 0b1000],
    ];

    let mut lr = 0;
    let mut ud = 0;
    let mut corner_ld = 0;
    let mut corner_lu = 0;
    let mut corner_rd = 0;
    let mut corner_ru = 0;

    if bitmasks.len() < 4 {
        return (0, 0, 0, 0, 0, 0);
    }

    for i in 0..(bitmasks.len() - 3) {
        let block = [bitmasks[i], bitmasks[i + 1], bitmasks[i + 2], bitmasks[i + 3]];
        if LR_BOXES.iter().any(|pat| *pat == block) {
            lr += 1;
        }
        if UD_BOXES.iter().any(|pat| *pat == block) {
            ud += 1;
        }
        if CORNER_LD_BOXES.iter().any(|pat| *pat == block) {
            corner_ld += 1;
        }
        if CORNER_LU_BOXES.iter().any(|pat| *pat == block) {
            corner_lu += 1;
        }
        if CORNER_RD_BOXES.iter().any(|pat| *pat == block) {
            corner_rd += 1;
        }
        if CORNER_RU_BOXES.iter().any(|pat| *pat == block) {
            corner_ru += 1;
        }
    }
    (lr, ud, corner_ld, corner_lu, corner_rd, corner_ru)
}

#[inline]
fn count_doritos(bitmasks: &[u8]) -> (u32, u32, u32, u32) {
    const RIGHT_DORITO: [u8; 5] = [
        0b1000, 0b0100, 0b0010, 0b0100, 0b1000
    ];
    const LEFT_DORITO: [u8; 5] = [
        0b0001, 0b0010, 0b0100, 0b0010, 0b0001
    ];
    const INV_RIGHT_DORITO: [u8; 5] = [
        0b1000, 0b0010, 0b0100, 0b0010, 0b1000
    ];
    const INV_LEFT_DORITO: [u8; 5] = [
        0b0001, 0b0100, 0b0010, 0b0100, 0b0001
    ];

    let mut rd_count = 0;
    let mut ld_count = 0;
    let mut ird_count = 0;
    let mut ild_count = 0;

    if bitmasks.len() < 5 {
        return (0, 0, 0, 0);
    }

    let mut i = 0;
    while i + 4 < bitmasks.len() {
        if bitmasks[i..i+5].iter().all(|&b| b.count_ones() == 1) {
            let block = &bitmasks[i..i+5];
            if block == &RIGHT_DORITO {
                rd_count += 1;
                i += 5;
                continue;
            } else if block == &LEFT_DORITO {
                ld_count += 1;
                i += 5;
                continue;
            } else if block == &INV_RIGHT_DORITO {
                ird_count += 1;
                i += 5;
                continue;
            } else if block == &INV_LEFT_DORITO {
                ild_count += 1;
                i += 5;
                continue;
            }
        }
        i += 1;
    }

    (rd_count, ld_count, ird_count, ild_count)
}

fn count_anchors(bitmasks: &[u8], arrow_bit: u8) -> u32 {
    let mut count = 0;
    let n = bitmasks.len();
    let mask = 1 << arrow_bit;
    let mut i = 0;

    while i + 4 < n {
        if (bitmasks[i] & mask) != 0
            && (bitmasks[i + 2] & mask) != 0
            && (bitmasks[i + 4] & mask) != 0
        {
            count += 1;
            i += 5;
        } else {
            i += 1;
        }
    }
    count
}

fn do_pattern_analysis(bitmasks: &[u8], total_arrows: u32) -> PatternStats {
    let (left_foot_candles, right_foot_candles) = count_candles(bitmasks);
    let total_candles = left_foot_candles + right_foot_candles;

    let candles_percent = if total_arrows > 1 {
        let denom = ((total_arrows.saturating_sub(1) / 2) as f64).floor();
        if denom > 0.0 {
            (total_candles as f64 / denom) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    let (ld_ru_mono, lu_rd_mono) = count_monos(bitmasks);
    let total_mono_arrows = (ld_ru_mono + lu_rd_mono) * 4;
    let mono_percent = if total_arrows > 0 {
        (total_mono_arrows as f64 / total_arrows as f64) * 100.0
    } else {
        0.0
    };

    let (lr_boxes, ud_boxes, corner_ld_boxes, corner_lu_boxes, corner_rd_boxes, corner_ru_boxes)
        = count_boxes(bitmasks);

    let anchor_left  = count_anchors(bitmasks, 0);
    let anchor_down  = count_anchors(bitmasks, 1);
    let anchor_up    = count_anchors(bitmasks, 2);
    let anchor_right = count_anchors(bitmasks, 3);

    let (rd, ld, ird, ild) = count_doritos(bitmasks);

    PatternStats {
        left_foot_candles,
        right_foot_candles,
        total_candles,
        candles_percent,
        ld_ru_mono,
        lu_rd_mono,
        mono_percent,
        lr_boxes,
        ud_boxes,
        corner_ld_boxes,
        corner_lu_boxes,
        corner_rd_boxes,
        corner_ru_boxes,
        anchor_left,
        anchor_down,
        anchor_up,
        anchor_right,
        right_dorito: rd,
        left_dorito: ld,
        inv_right_dorito: ird,
        inv_left_dorito: ild,
    }
}

// --------------------------------------------------------------------
// Density Graph PNG
// --------------------------------------------------------------------

fn generate_density_graph_png(
    measure_nps_vec: &[f64],
    max_nps: f64,
    short_hash: &str,
) -> Result<(), ImageError> {
    const IMAGE_WIDTH: u32 = 1000;
    const GRAPH_HEIGHT: u32 = 400;

    let mut imgbuf: RgbImage = ImageBuffer::new(IMAGE_WIDTH, GRAPH_HEIGHT);

    let bg_color = Rgb([0x03, 0x11, 0x2c]);
    for pixel in imgbuf.pixels_mut() {
        *pixel = bg_color;
    }

    if measure_nps_vec.is_empty() || max_nps <= 0.0 {
        let filename = format!("{}.png", short_hash);
        imgbuf.save(&filename)?;
        return Ok(());
    }

    let measure_width = IMAGE_WIDTH as f64 / measure_nps_vec.len() as f64;

    fn lerp(a: u8, b: u8, t: f64) -> u8 {
        let t = t.clamp(0.0, 1.0);
        let af = a as f64;
        let bf = b as f64;
        (af + (bf - af) * t).round() as u8
    }

    // gradient from bottom_color => top_color
    let bottom_color = (0x00, 0xb8, 0xcc);
    let top_color    = (0x82, 0x00, 0xa1);

    for (i, &nps) in measure_nps_vec.iter().enumerate() {
        let x_start = (i as f64 * measure_width).round() as u32;
        let x_end   = (((i + 1) as f64 * measure_width).round() as u32).min(IMAGE_WIDTH);

        let height_fraction = nps / max_nps;
        let bar_height = (height_fraction * GRAPH_HEIGHT as f64).round() as i32;
        let y_top = GRAPH_HEIGHT as i32 - bar_height;

        for x in x_start..x_end {
            for y in y_top..(GRAPH_HEIGHT as i32) {
                if y < 0 || y >= GRAPH_HEIGHT as i32 || x >= IMAGE_WIDTH {
                    continue;
                }
                let dist_from_bottom = (GRAPH_HEIGHT as i32 - 1 - y) as f64;
                let frac = dist_from_bottom / (GRAPH_HEIGHT as f64 - 1.0);

                let rr = lerp(bottom_color.0, top_color.0, frac);
                let gg = lerp(bottom_color.1, top_color.1, frac);
                let bb = lerp(bottom_color.2, top_color.2, frac);

                *imgbuf.get_pixel_mut(x, y as u32) = Rgb([rr, gg, bb]);
            }
        }
    }

    let output_file = format!("{}.png", short_hash);
    imgbuf.save(&output_file)?;

    Ok(())
}

// --------------------------------------------------------------------
// Main
// --------------------------------------------------------------------

fn main() -> io::Result<()> {
    // Start timer BEFORE any processing:
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

    // Convert to owned String so we can conditionally strip tags.
    let mut title_str = std::str::from_utf8(title_opt.unwrap_or(b"<invalid-title>"))
        .unwrap_or("<invalid-title>")
        .to_owned();
    
    // If --strip-tags is present, remove bracketed numeric tags from the title
    if strip_tags {
        title_str = strip_title_tags(&title_str);
    }

    let subtitle_str = std::str::from_utf8(subtitle_opt.unwrap_or(b"<invalid-subtitle>"))
        .unwrap_or("<invalid-subtitle>");
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
    let detailed = generate_breakdown(&measure_densities, BreakdownMode::Detailed);
    let partial  = generate_breakdown(&measure_densities, BreakdownMode::Partial);
    let simple   = generate_breakdown(&measure_densities, BreakdownMode::Simplified);

    // Hash
    let mut hasher = Sha1::new();
    hasher.update(&minimized_chart);
    hasher.update(normalized_bpms.as_bytes());
    let hash_result = hasher.finalize();
    let hash_hex = hex::encode(hash_result);
    let short_hash = &hash_hex[..16];

    // BPM map and range
    let bpm_map = parse_bpm_map(&normalized_bpms);
    let (min_bpm, max_bpm) = compute_bpm_range(&bpm_map);

    // NPS vector + stats
    let measure_nps_vec = compute_measure_nps_vec(&measure_densities, &bpm_map);
    let (max_nps, median_nps) = get_nps_stats(&measure_nps_vec);

    // Chart length (seconds) as int
    let total_length = compute_total_chart_length(&measure_densities, &bpm_map);

    // Pattern stats
    let bitmasks = parse_bitmask_chart(&minimized_chart);
    let pattern_stats = do_pattern_analysis(&bitmasks, stats.total_arrows);

    // Generate PNG if requested (but DO NOT return yet).
    if generate_png {
        // If there's an error in PNG generation, convert to io::Error
        generate_density_graph_png(&measure_nps_vec, max_nps, short_hash)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    }

    // Finally, print elapsed time at the END
    let elapsed = start_time.elapsed();

    // Now do JSON or text output (or neither).
    if generate_json {
        println!("{{");
        // We place elapsed time at the END, so skip for now.

        // Basic info
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

        // Arrow Stats
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

        // Stream Counts
        println!("  \"stream_counts\": {{");
        println!("     \"run16_streams\": {},", stream_counts.run16_streams);
        println!("     \"run20_streams\": {},", stream_counts.run20_streams);
        println!("     \"run24_streams\": {},", stream_counts.run24_streams);
        println!("     \"run32_streams\": {},", stream_counts.run32_streams);
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
        println!("     \"max_nps\": {:.2},", max_nps);
        println!("     \"median_nps\": {:.2}", median_nps);
        println!("  }},");

        // Pattern stats
        println!("  \"pattern_stats\": {{");
        println!("     \"left_foot_candles\": {},", pattern_stats.left_foot_candles);
        println!("     \"right_foot_candles\": {},", pattern_stats.right_foot_candles);
        println!("     \"total_candles\": {},", pattern_stats.total_candles);
        println!("     \"candles_percent\": {:.2},", pattern_stats.candles_percent);
        println!("     \"ld_ru_mono\": {},", pattern_stats.ld_ru_mono);
        println!("     \"lu_rd_mono\": {},", pattern_stats.lu_rd_mono);
        println!("     \"mono_percent\": {:.2},", pattern_stats.mono_percent);
        println!("     \"lr_boxes\": {},", pattern_stats.lr_boxes);
        println!("     \"ud_boxes\": {},", pattern_stats.ud_boxes);
        println!("     \"corner_ld_boxes\": {},", pattern_stats.corner_ld_boxes);
        println!("     \"corner_lu_boxes\": {},", pattern_stats.corner_lu_boxes);
        println!("     \"corner_rd_boxes\": {},", pattern_stats.corner_rd_boxes);
        println!("     \"corner_ru_boxes\": {},", pattern_stats.corner_ru_boxes);
        println!("     \"anchor_left\": {},", pattern_stats.anchor_left);
        println!("     \"anchor_down\": {},", pattern_stats.anchor_down);
        println!("     \"anchor_up\": {},", pattern_stats.anchor_up);
        println!("     \"anchor_right\": {},", pattern_stats.anchor_right);
        println!("     \"right_dorito\": {},", pattern_stats.right_dorito);
        println!("     \"left_dorito\": {},", pattern_stats.left_dorito);
        println!("     \"inv_right_dorito\": {},", pattern_stats.inv_right_dorito);
        println!("     \"inv_left_dorito\": {}", pattern_stats.inv_left_dorito);
        println!("  }},");

        // Execution time
        println!("  \"elapsed\": \"{:?}\"", elapsed);
        println!("}}");
    } else {
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

        println!("--- Pattern Stats ---");
        println!("left_foot_candles: {}", pattern_stats.left_foot_candles);
        println!("right_foot_candles: {}", pattern_stats.right_foot_candles);
        println!("total_candles: {}", pattern_stats.total_candles);
        println!("candles_percent: {:.2}", pattern_stats.candles_percent);
        println!("ld_ru_mono: {}", pattern_stats.ld_ru_mono);
        println!("lu_rd_mono: {}", pattern_stats.lu_rd_mono);
        println!("mono_percent: {:.2}", pattern_stats.mono_percent);
        println!("lr_boxes: {}", pattern_stats.lr_boxes);
        println!("ud_boxes: {}", pattern_stats.ud_boxes);
        println!("corner_ld_boxes: {}", pattern_stats.corner_ld_boxes);
        println!("corner_lu_boxes: {}", pattern_stats.corner_lu_boxes);
        println!("corner_rd_boxes: {}", pattern_stats.corner_rd_boxes);
        println!("corner_ru_boxes: {}", pattern_stats.corner_ru_boxes);
        println!("anchor_left: {}", pattern_stats.anchor_left);
        println!("anchor_down: {}", pattern_stats.anchor_down);
        println!("anchor_up: {}", pattern_stats.anchor_up);
        println!("anchor_right: {}", pattern_stats.anchor_right);
        println!("right_dorito: {}", pattern_stats.right_dorito);
        println!("left_dorito: {}", pattern_stats.left_dorito);
        println!("inv_right_dorito: {}", pattern_stats.inv_right_dorito);
        println!("inv_left_dorito: {}", pattern_stats.inv_left_dorito);
        println!("---");
        println!("Elapsed time: {:?}", elapsed);
    }

    Ok(())
}

/// Minimal “escape” function for JSON strings (handle quotes, backslashes, etc.).
fn escape_json(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
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

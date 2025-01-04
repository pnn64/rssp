use std::env::args;
use std::fs::File;
use std::io::{self, Read};
use std::time::Instant;
use std::fmt::Write as FmtWrite;
use std::collections::HashMap;
use std::sync::LazyLock;

use sha1::{Digest, Sha1};
use png;

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

#[derive(Default)]
struct StreamCounts {
    run16_streams: u32,
    run20_streams: u32,
    run24_streams: u32,
    run32_streams: u32,
    total_breaks: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum RunDensity {
    Run32,
    Run24,
    Run20,
    Run16,
    Break,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BreakdownMode {
    Detailed,
    Partial,
    Simplified,
}

#[inline]
fn is_all_zero(line: &[u8; 4]) -> bool {
    line.iter().all(|&b| b == b'0')
}

#[inline]
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

#[inline]
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
            b'M' => {
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
            b'/' => {
                // skip lines starting with comment
            }
            _ => {
                if line.len() < 4 {
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

    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return sc;
    }

    let start_idx = first_run.unwrap();
    let end_idx   = last_run.unwrap();

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

#[derive(Debug)]
enum Token {
    Run(RunDensity, usize),
    Break(usize),
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

fn generate_breakdown(measure_densities: &[usize], mode: BreakdownMode) -> String {
    let cats: Vec<RunDensity> = measure_densities
        .iter()
        .map(|&d| categorize_measure_density(d))
        .collect();

    let first_run = cats.iter().position(|&c| c != RunDensity::Break);
    let last_run  = cats.iter().rposition(|&c| c != RunDensity::Break);
    if first_run.is_none() || last_run.is_none() {
        return String::new();
    }

    let mut tokens = Vec::new();
    {
        let mut i = first_run.unwrap();
        let end = last_run.unwrap();
        while i <= end {
            let cat = cats[i];
            let mut length = 1;
            let mut j = i + 1;
            while j <= end && cats[j] == cat {
                length += 1;
                j += 1;
            }
            if cat == RunDensity::Break {
                tokens.push(Token::Break(length));
            } else {
                tokens.push(Token::Run(cat, length));
            }
            i = j;
        }
    }

    let mut output = Vec::new();
    let mut idx = 0;

    let threshold = match mode {
        BreakdownMode::Partial => 1,
        BreakdownMode::Simplified => 4,
        BreakdownMode::Detailed => 0,
    };

    while idx < tokens.len() {
        match tokens[idx] {
            Token::Run(curr_cat, mut curr_len) => {
                let mut star = false;

                if mode != BreakdownMode::Detailed {
                    'merge_loop: loop {
                        if idx + 1 >= tokens.len() {
                            break;
                        }
                        let Token::Break(bk_len) = tokens[idx + 1] else {
                            break;
                        };
                        if bk_len > threshold {
                            break;
                        }
                        if idx + 2 >= tokens.len() {
                            break;
                        }

                        if let Token::Run(next_cat, next_len) = tokens[idx + 2] {
                            if next_cat == curr_cat {
                                curr_len += bk_len + next_len;
                                star = true;
                                tokens.remove(idx + 1);
                                tokens.remove(idx + 1);
                                continue 'merge_loop;
                            } else {
                                if bk_len == 1 {
                                    break 'merge_loop;
                                }
                                if mode == BreakdownMode::Simplified && bk_len <= 4 {
                                    curr_len += bk_len;
                                    star = true;
                                    tokens.remove(idx + 1);
                                }
                                break 'merge_loop;
                            }
                        } else {
                            break 'merge_loop;
                        }
                    }
                }

                let s = format_run_symbol(curr_cat, curr_len, star);
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

fn compute_bpm_range(bpm_map: &[(f64, f64)]) -> (i32, i32) {
    if bpm_map.is_empty() {
        return (0, 0);
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
    (
        min_bpm.round() as i32,
        max_bpm.round() as i32,
    )
}

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
    total_length_seconds.floor() as i32
}

fn compute_measure_nps_vec(measure_densities: &[usize], bpm_map: &[(f64, f64)]) -> Vec<f64> {
    let mut measure_nps_vec = Vec::with_capacity(measure_densities.len());
    for (i, &density) in measure_densities.iter().enumerate() {
        let measure_start_beat = i as f64 * 4.0;
        let curr_bpm = get_current_bpm(measure_start_beat, bpm_map);
        if curr_bpm <= 0.0 {
            measure_nps_vec.push(0.0);
            continue;
        }
        let measure_nps = density as f64 * (curr_bpm / 4.0) / 60.0;
        measure_nps_vec.push(measure_nps);
    }
    measure_nps_vec
}

/// A small helper to compute median of a slice of f64.
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

fn get_nps_stats(measure_nps_vec: &[f64]) -> (f64, f64) {
    let max_nps = if measure_nps_vec.is_empty() {
        0.0
    } else {
        measure_nps_vec.iter().fold(f64::MIN, |a, &b| a.max(b))
    };
    let median_nps = median(measure_nps_vec);
    (max_nps, median_nps)
}

fn extract_sections(
    data: &[u8],
) -> io::Result<(
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
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

    Ok((title, subtitle, artist, titletranslit, subtitletranslit, artisttranslit, bpms, notes))
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
// 8) Single-Pass detection for everything except anchors
// --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PatternVariant {
    CandleLeft,
    CandleRight,
    BoxLR,
    BoxUD,
    BoxCornerLD,
    BoxCornerLU,
    BoxCornerRD,
    BoxCornerRU,
    DoritoRight,
    DoritoLeft,
    DoritoInvRight,
    DoritoInvLeft,
    SpiralLeft,
    SpiralRight,
    CopterLeft,
    CopterRight,
    LuchiLeft,
    LuchiRight,
    HipBreakerLeft,
    HipBreakerRight,
    SweepLeft,
    SweepRight,
    SweepInvLeft,
    SweepInvRight,
}

fn string_to_pattern_bits(p: &str) -> Vec<u8> {
    let mut result = Vec::with_capacity(p.len());
    for c in p.chars() {
        let mask = match c {
            'L' => 0b0001,
            'D' => 0b0010,
            'U' => 0b0100,
            'R' => 0b1000,
            _ => 0b0000,
        };
        result.push(mask);
    }
    result
}

static ALL_PATTERNS_NON_ANCHORS: LazyLock<Vec<(PatternVariant, Vec<u8>)>> = LazyLock::new(|| {
    let mut patterns = Vec::new();

    // Candles
    patterns.push((PatternVariant::CandleLeft,  string_to_pattern_bits("ULD")));
    patterns.push((PatternVariant::CandleLeft,  string_to_pattern_bits("DLU")));
    patterns.push((PatternVariant::CandleRight, string_to_pattern_bits("URD")));
    patterns.push((PatternVariant::CandleRight, string_to_pattern_bits("DRU")));

    // Boxes
    patterns.push((PatternVariant::BoxLR,       string_to_pattern_bits("LRLR")));
    patterns.push((PatternVariant::BoxLR,       string_to_pattern_bits("RLRL")));
    patterns.push((PatternVariant::BoxUD,       string_to_pattern_bits("UDUD")));
    patterns.push((PatternVariant::BoxUD,       string_to_pattern_bits("DUDU")));
    patterns.push((PatternVariant::BoxCornerLD, string_to_pattern_bits("LDLD")));
    patterns.push((PatternVariant::BoxCornerLD, string_to_pattern_bits("DLDL")));
    patterns.push((PatternVariant::BoxCornerLU, string_to_pattern_bits("LULU")));
    patterns.push((PatternVariant::BoxCornerLU, string_to_pattern_bits("ULUL")));
    patterns.push((PatternVariant::BoxCornerRD, string_to_pattern_bits("RDRD")));
    patterns.push((PatternVariant::BoxCornerRD, string_to_pattern_bits("DRDR")));
    patterns.push((PatternVariant::BoxCornerRU, string_to_pattern_bits("RURU")));
    patterns.push((PatternVariant::BoxCornerRU, string_to_pattern_bits("URUR")));

    // Doritos
    patterns.push((PatternVariant::DoritoLeft,     string_to_pattern_bits("LDUDL")));
    patterns.push((PatternVariant::DoritoRight,    string_to_pattern_bits("RUDUR")));
    patterns.push((PatternVariant::DoritoInvLeft,  string_to_pattern_bits("LUDUL")));
    patterns.push((PatternVariant::DoritoInvRight, string_to_pattern_bits("RDUDR")));

    // Spirals
    patterns.push((PatternVariant::SpiralLeft,  string_to_pattern_bits("LDURDR")));
    patterns.push((PatternVariant::SpiralRight, string_to_pattern_bits("RUDLUL")));

    // Copters
    patterns.push((PatternVariant::CopterLeft,  string_to_pattern_bits("LDURDULDURDU")));
    patterns.push((PatternVariant::CopterLeft,  string_to_pattern_bits("DULDURDULDUR")));
    patterns.push((PatternVariant::CopterRight, string_to_pattern_bits("RUDLUDRUDLUD")));
    patterns.push((PatternVariant::CopterRight, string_to_pattern_bits("UDRUDLUDRUDL")));

    // Luchi
    patterns.push((PatternVariant::LuchiLeft,  string_to_pattern_bits("LDLRURDRLULD")));
    patterns.push((PatternVariant::LuchiRight, string_to_pattern_bits("RURLDLULRDRU")));

    // Hip-Breakers
    patterns.push((PatternVariant::HipBreakerLeft,  string_to_pattern_bits("LDUDLUDULDUDL")));
    patterns.push((PatternVariant::HipBreakerRight, string_to_pattern_bits("RUDURDUDRUDUR")));

    // Sweeps
    patterns.push((PatternVariant::SweepLeft,     string_to_pattern_bits("LDURUDL")));
    patterns.push((PatternVariant::SweepRight,    string_to_pattern_bits("RUDLDUR")));
    patterns.push((PatternVariant::SweepInvLeft,  string_to_pattern_bits("LUDRDUL")));
    patterns.push((PatternVariant::SweepInvRight, string_to_pattern_bits("RDULUDR")));

    patterns
});

fn detect_all_patterns_non_anchors(bitmasks: &[u8]) -> HashMap<PatternVariant, u32> {
    let mut results: HashMap<PatternVariant, u32> = HashMap::new();
    let defs: &[(PatternVariant, Vec<u8>)] = ALL_PATTERNS_NON_ANCHORS.as_ref();

    let mut i = 0;
    while i < bitmasks.len() {
        let mut matched_any = false;
        for (variant, pat_bits) in defs.iter() {
            let plen = pat_bits.len();
            if i + plen <= bitmasks.len() {
                if bitmasks[i..i + plen] == pat_bits[..] {
                    *results.entry(*variant).or_insert(0) += 1;
                    i += plen; 
                    matched_any = true;
                    break;
                }
            }
        }
        if !matched_any {
            i += 1;
        }
    }

    results
}

fn count_anchors(bitmasks: &[u8]) -> (u32, u32, u32, u32) {
    let mut anchor_left = 0;
    let mut anchor_down = 0;
    let mut anchor_up = 0;
    let mut anchor_right = 0;

    let n = bitmasks.len();
    let mut i = 0;
    while i + 4 < n {
        if (bitmasks[i] & 0b0001) != 0
            && (bitmasks[i + 2] & 0b0001) != 0
            && (bitmasks[i + 4] & 0b0001) != 0
        {
            anchor_left += 1;
        }
        if (bitmasks[i] & 0b0010) != 0
            && (bitmasks[i + 2] & 0b0010) != 0
            && (bitmasks[i + 4] & 0b0010) != 0
        {
            anchor_down += 1;
        }
        if (bitmasks[i] & 0b0100) != 0
            && (bitmasks[i + 2] & 0b0100) != 0
            && (bitmasks[i + 4] & 0b0100) != 0
        {
            anchor_up += 1;
        }
        if (bitmasks[i] & 0b1000) != 0
            && (bitmasks[i + 2] & 0b1000) != 0
            && (bitmasks[i + 4] & 0b1000) != 0
        {
            anchor_right += 1;
        }
        i += 1;
    }

    (anchor_left, anchor_down, anchor_up, anchor_right)
}

fn generate_density_graph_png(
    measure_nps_vec: &[f64],
    max_nps: f64,
    short_hash: &str,
) -> io::Result<()> {
    const IMAGE_WIDTH: u32 = 1000;
    const GRAPH_HEIGHT: u32 = 400;

    let bg_color = [3, 17, 44];
    let bottom_color = [0, 184, 204];
    let top_color = [130, 0, 161];

    let mut img_buffer = vec![0u8; (IMAGE_WIDTH * GRAPH_HEIGHT * 3) as usize];
    for y in 0..GRAPH_HEIGHT {
        for x in 0..IMAGE_WIDTH {
            let idx = ((y * IMAGE_WIDTH + x) * 3) as usize;
            img_buffer[idx] = bg_color[0];
            img_buffer[idx + 1] = bg_color[1];
            img_buffer[idx + 2] = bg_color[2];
        }
    }

    if !measure_nps_vec.is_empty() && max_nps > 0.0 {
        let measure_width = IMAGE_WIDTH as f64 / measure_nps_vec.len() as f64;

        for (i, &nps) in measure_nps_vec.iter().enumerate() {
            let x_start = (i as f64 * measure_width).round() as u32;
            let x_end = ((i as f64 + 1.0) * measure_width).round() as u32;
            let x_end = x_end.min(IMAGE_WIDTH);

            let height_fraction = (nps / max_nps).min(1.0);
            let bar_height = (height_fraction * GRAPH_HEIGHT as f64).round() as u32;
            let y_top = GRAPH_HEIGHT.saturating_sub(bar_height);

            for x in x_start..x_end {
                for y in y_top..GRAPH_HEIGHT {
                    let dist_from_bottom = (GRAPH_HEIGHT - 1 - y) as f64;
                    let frac = dist_from_bottom / (GRAPH_HEIGHT as f64 - 1.0);

                    let r = ((bottom_color[0] as f64)
                        + ((top_color[0] as f64 - bottom_color[0] as f64) * frac))
                        .round() as u8;
                    let g = ((bottom_color[1] as f64)
                        + ((top_color[1] as f64 - bottom_color[1] as f64) * frac))
                        .round() as u8;
                    let b = ((bottom_color[2] as f64)
                        + ((top_color[2] as f64 - bottom_color[2] as f64) * frac))
                        .round() as u8;

                    let idx = ((y * IMAGE_WIDTH + x) * 3) as usize;
                    img_buffer[idx] = r;
                    img_buffer[idx + 1] = g;
                    img_buffer[idx + 2] = b;
                }
            }
        }
    }

    let filename = format!("{}.png", short_hash);
    let file = File::create(filename)?;

    let mut encoder = png::Encoder::new(file, IMAGE_WIDTH, GRAPH_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    writer.write_image_data(&img_buffer)?;
    Ok(())
}

// --------------------------------------------------------------------
// 11) Main
// --------------------------------------------------------------------

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
    let hash_hex = hex::encode(hash_result);
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

    // For convenience, define a small helper to fetch counts.
    fn c(d: &HashMap<PatternVariant, u32>, v: PatternVariant) -> u32 {
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
        //TOTAL CANDLES HERE
        //CANDLES % HERE

        //MONO INFO HERE
        //STAIRS, DOUBLE STAIRS INV STAIRS
        //TOTAL MONO
        //MONO % HERE

        // Boxes
        println!("     \"box_lr\": {},", c(&detected_non_anchors, PatternVariant::BoxLR));
        println!("     \"box_ud\": {},", c(&detected_non_anchors, PatternVariant::BoxUD));
        println!("     \"box_corner_ld\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerLD));
        println!("     \"box_corner_lu\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerLU));
        println!("     \"box_corner_rd\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerRD));
        println!("     \"box_corner_ru\": {},", c(&detected_non_anchors, PatternVariant::BoxCornerRU));
        //TOTAL BOXES HERE

        // Doritos
        println!("     \"dorito_right\": {},", c(&detected_non_anchors, PatternVariant::DoritoRight));
        println!("     \"dorito_left\": {},", c(&detected_non_anchors, PatternVariant::DoritoLeft));
        println!("     \"dorito_inv_right\": {},", c(&detected_non_anchors, PatternVariant::DoritoInvRight));
        println!("     \"dorito_inv_left\": {},", c(&detected_non_anchors, PatternVariant::DoritoInvLeft));
        //TOTAL DORITOS HERE

        // Spirals
        println!("     \"left_spiral\": {},", c(&detected_non_anchors, PatternVariant::SpiralLeft));
        println!("     \"right_spiral\": {},", c(&detected_non_anchors, PatternVariant::SpiralRight));
        //TOTAL SPIRALS HERE

        // Copters
        println!("     \"left_copter\": {},", c(&detected_non_anchors, PatternVariant::CopterLeft));
        println!("     \"right_copter\": {},", c(&detected_non_anchors, PatternVariant::CopterRight));
        //TOTAL COPTERS HERE

        // Luchi
        println!("     \"left_luchi\": {},", c(&detected_non_anchors, PatternVariant::LuchiLeft));
        println!("     \"right_luchi\": {},", c(&detected_non_anchors, PatternVariant::LuchiRight));
        //TOTAL LUCHI HERE

        // Hip-Breakers
        println!("     \"left_hip_breaker\": {},", c(&detected_non_anchors, PatternVariant::HipBreakerLeft));
        println!("     \"right_hip_breaker\": {},", c(&detected_non_anchors, PatternVariant::HipBreakerRight));
        //TOTAL HIPBREAKERS HERE

        // Sweeps
        println!("     \"left_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepLeft));
        println!("     \"right_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepRight));
        println!("     \"left_inv_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepInvLeft));
        println!("     \"right_inv_sweep\": {},", c(&detected_non_anchors, PatternVariant::SweepInvRight));
        //TOTAL SWEEPS HERE

        // Anchors
        println!("     \"anchor_left\": {},", anchor_left);
        println!("     \"anchor_down\": {},", anchor_down);
        println!("     \"anchor_up\": {},", anchor_up);
        println!("     \"anchor_right\": {}", anchor_right);
        //TOTAL ANCHORS HERE
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

        // For convenience, define a little helper:
        fn c(d: &HashMap<PatternVariant, u32>, v: PatternVariant) -> u32 {
            *d.get(&v).unwrap_or(&0)
        }

        // Now print all single-pass patterns we defined:
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

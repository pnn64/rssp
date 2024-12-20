#![recursion_limit = "256"]

use clap::Parser;
use serde_json::json;
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the simfile (.sm)
    simfile: String,

    /// Strip ECS/SRPG tags from the title
    #[arg(short, long)]
    strip_tags: bool,
}

const METADATA_KEYS: &[&str] = &[
    "#TITLE",
    "#TITLETRANSLIT",
    "#SUBTITLE",
    "#SUBTITLETRANSLIT",
    "#ARTIST",
    "#ARTISTTRANSLIT",
];

struct Simfile {
    metadata: HashMap<String, String>,
    charts: Vec<Chart>,
    bpms: String,
    bpms_map: Vec<(f64, f64)>,
}

struct Chart {
    steps_type: String,
    difficulty: String,
    difficulty_num: i32,
    note_data: String,
}

#[derive(Default)]
struct NoteCounts {
    arrows: usize,
    steps: usize,
    mines: usize,
    holds: usize,
    rolls: usize,
    jumps: usize,
    hands: usize,
    left: usize,
    down: usize,
    up: usize,
    right: usize,
    total_measures: usize,
}

#[derive(Default)]
struct StreamCounts {
    total_breaks: usize,
    run16_streams: usize,
    run20_streams: usize,
    run24_streams: usize,
    run32_streams: usize,
}

#[derive(Default)]
struct PatternInfo {
    left_foot_candles: usize,
    right_foot_candles: usize,
    total_candles: usize,
    candles_percent: f64,
    ld_ru_mono: usize,
    lu_rd_mono: usize,
    mono_percent: f64,
    lr_boxes: usize,
    ud_boxes: usize,
    corner_ld_boxes: usize,
    corner_lu_boxes: usize,
    corner_rd_boxes: usize,
    corner_ru_boxes: usize,
    anchor_left: usize,
    anchor_down: usize,
    anchor_up: usize,
    anchor_right: usize,
}

#[derive(Debug, Clone)]
struct BreakdownToken {
    length: usize,
    is_run: bool,
    density: Option<RunDensity>,
    run_symbol: String,
}

#[derive(Debug, Clone, PartialEq)]
enum RunDensity {
    Run32,
    Run24,
    Run20,
    Run16,
    Break,
}

fn main() {
    let args = Args::parse();
    process_file(&args.simfile, args.strip_tags);
}

fn process_file(filename: &str, strip_tags: bool) {
    if let Some(content) = get_simfile_content(filename) {
        match parse_simfile(&content, strip_tags) {
            Ok(simfile) => {
                let bpm_values: Vec<f64> = simfile.bpms_map.iter().map(|&(_, bpm)| bpm).collect();

                let max_bpm = bpm_values
                    .iter()
                    .cloned()
                    .fold(f64::NAN, f64::max)
                    .round() as i32;
                let min_bpm = bpm_values
                    .iter()
                    .cloned()
                    .fold(f64::NAN, f64::min)
                    .round() as i32;

                for chart in simfile.charts {
                    let notes = clean_note_data(&chart.note_data);
                    let data_to_hash = format!("{}{}", notes.trim_end(), simfile.bpms);

                    let hash_result = Sha1::digest(data_to_hash.as_bytes());
                    let hash_hex = hex::encode(&hash_result)[..16].to_string();

                    let (
                        counts,
                        detailed_breakdown,
                        partially_simplified,
                        simplified,
                        stream_counts,
                        measure_nps,
                        length_in_seconds,
                        pattern_info,
                    ) = process_chart(
                        &notes,
                        &chart.steps_type,
                        &simfile.bpms_map,
                    );

                    let length_in_seconds_int = length_in_seconds.floor() as usize;

                    let max_nps = measure_nps.iter().cloned().fold(f64::NAN, f64::max);

                    let mut sorted_nps = measure_nps.clone();
                    sorted_nps.sort_by(|a, b| a.partial_cmp(b).unwrap());

                    let median_nps =
                        if sorted_nps.len() % 2 == 0 && sorted_nps.len() > 0 {
                            let mid = sorted_nps.len() / 2;
                            (sorted_nps[mid - 1] + sorted_nps[mid]) / 2.0
                        } else if sorted_nps.len() > 0 {
                            sorted_nps[sorted_nps.len() / 2]
                        } else {
                            0.0
                        };

                    let total_streams = stream_counts.run16_streams
                        + stream_counts.run20_streams
                        + stream_counts.run24_streams
                        + stream_counts.run32_streams;

                    let rounded_max_nps = round_half_up(max_nps, 2);
                    let rounded_median_nps = round_half_up(median_nps, 2);

                    let chart_info = json!({
                        "title": simfile.metadata.get("title").map(|s| s.as_str()).unwrap_or_default(),
                        "titletranslit": simfile.metadata.get("titletranslit").map(|s| s.as_str()).unwrap_or_default(),
                        "subtitle": simfile.metadata.get("subtitle").map(|s| s.as_str()).unwrap_or_default(),
                        "subtitletranslit": simfile.metadata.get("subtitletranslit").map(|s| s.as_str()).unwrap_or_default(),
                        "artist": simfile.metadata.get("artist").map(|s| s.as_str()).unwrap_or_default(),
                        "artisttranslit": simfile.metadata.get("artisttranslit").map(|s| s.as_str()).unwrap_or_default(),
                        "bpms": simfile.bpms,
                        "steps_type": chart.steps_type,
                        "diff": chart.difficulty,
                        "diff_number": chart.difficulty_num.to_string(),
                        "hash": hash_hex,
                        "length": length_in_seconds_int,
                        "max_bpm": max_bpm,
                        "min_bpm": min_bpm,
                        "max_nps": format!("{:.2}", rounded_max_nps),
                        "median_nps": format!("{:.2}", rounded_median_nps),
                        "arrows": {
                            "total": counts.arrows,
                            "left": counts.left,
                            "down": counts.down,
                            "up": counts.up,
                            "right": counts.right,
                        },
                        "steps": counts.steps,
                        "mines": counts.mines,
                        "holds": counts.holds,
                        "rolls": counts.rolls,
                        "jumps": counts.jumps,
                        "hands": counts.hands,
                        "detailed_breakdown": detailed_breakdown,
                        "partially_simplified_breakdown": partially_simplified,
                        "simplified_breakdown": simplified,
                        "streams": {
                            "total": total_streams,
                            "16th": stream_counts.run16_streams,
                            "20th": stream_counts.run20_streams,
                            "24th": stream_counts.run24_streams,
                            "32nd": stream_counts.run32_streams,
                        },
                        "total_breaks": stream_counts.total_breaks,
                        "left_foot_candles": pattern_info.left_foot_candles,
                        "right_foot_candles": pattern_info.right_foot_candles,
                        "total_candles": pattern_info.total_candles,
                        "candles_percent": format!("{:.2}", pattern_info.candles_percent),
                        "ld_ru_mono": pattern_info.ld_ru_mono,
                        "lu_rd_mono": pattern_info.lu_rd_mono,
                        "mono_percent": format!("{:.2}", pattern_info.mono_percent),
                        "lr_boxes": pattern_info.lr_boxes,
                        "ud_boxes": pattern_info.ud_boxes,
                        "corner_ld_boxes": pattern_info.corner_ld_boxes,
                        "corner_lu_boxes": pattern_info.corner_lu_boxes,
                        "corner_rd_boxes": pattern_info.corner_rd_boxes,
                        "corner_ru_boxes": pattern_info.corner_ru_boxes,
                        "anchor_left": pattern_info.anchor_left,
                        "anchor_down": pattern_info.anchor_down,
                        "anchor_up": pattern_info.anchor_up,
                        "anchor_right": pattern_info.anchor_right,
                    });

                    println!("Chart Info:");
                    match serde_json::to_string_pretty(&chart_info) {
                        Ok(json_str) => println!("{}", json_str),
                        Err(e) => eprintln!("Error marshaling JSON: {}", e),
                    }
                }
            }
            Err(e) => eprintln!("Error parsing simfile: {}", e),
        }
    }
}

fn get_simfile_content(filename: &str) -> Option<String> {
    let path = Path::new(filename);
    let ext = path.extension()?.to_str()?;
    if !ext.eq_ignore_ascii_case("sm") {
        return None;
    }
    fs::read_to_string(filename)
        .map(|content| content.trim_start_matches('\u{FEFF}').to_owned())
        .ok()
}

fn parse_simfile(
    content: &str,
    strip_tags: bool,
) -> Result<Simfile, Box<dyn std::error::Error>> {
    let directives = parse_directives(content);

    let mut metadata = parse_metadata(&directives);

    if strip_tags {
        if let Some(title) = metadata.get("title") {
            let cleaned_title = strip_title_tags(title);
            metadata.insert("title".to_string(), cleaned_title);
        }
    }

    let bpms = parse_bpms(content);
    let bpms_map = parse_bpms_map(&bpms);

    let charts = parse_charts(content)?;

    Ok(Simfile {
        metadata,
        charts,
        bpms,
        bpms_map,
    })
}

fn parse_directives(content: &str) -> Vec<&str> {
    content
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_metadata(directives: &[&str]) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    for &directive in directives {
        for &key in METADATA_KEYS {
            if directive.len() >= key.len()
                && directive.as_bytes()[..key.len()].eq_ignore_ascii_case(key.as_bytes())
            {
                if let Some(value) = parse_value(directive, key) {
                    metadata.insert(key[1..].to_lowercase(), value);
                }
                break;
            }
        }
    }
    metadata
}

fn parse_value(directive: &str, key: &str) -> Option<String> {
    let rest = directive[key.len()..].trim_start();
    if rest.starts_with(':') {
        let rest = &rest[1..];
        let value = rest.trim_end_matches(';').trim();
        Some(value.to_string())
    } else {
        None
    }
}

fn parse_bpms(content: &str) -> String {
    if let Some(value) = extract_value(content, "#BPMS") {
        let bpm_data = value.replace(['\n', '\r'], "");
        normalize_float_digits(&bpm_data)
    } else {
        "0.000=0.000".to_string()
    }
}

fn extract_value(content: &str, key: &str) -> Option<String> {
    content
        .split(';')
        .find_map(|directive| {
            let directive = directive.trim();
            if directive.len() >= key.len()
                && directive.as_bytes()[..key.len()].eq_ignore_ascii_case(key.as_bytes())
            {
                parse_value(directive, key)
            } else {
                None
            }
        })
}

fn parse_bpms_map(bpms: &str) -> Vec<(f64, f64)> {
    let mut bpm_entries: Vec<(f64, f64)> = bpms
        .split(',')
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.split('=').collect();
            if parts.len() == 2 {
                let beat = parts[0].parse::<f64>().ok()?;
                let bpm = parts[1].parse::<f64>().ok()?;
                Some((beat, bpm))
            } else {
                None
            }
        })
        .collect();

    bpm_entries.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    bpm_entries
}

fn parse_charts(content: &str) -> Result<Vec<Chart>, Box<dyn std::error::Error>> {
    let mut charts = Vec::new();
    let mut idx = 0;

    while let Some(pos) = find_case_insensitive(&content[idx..], "#NOTES") {
        idx += pos;
        let rest = &content[idx + 6..]; // skip past "#NOTES"
        let rest = rest.trim_start();
        if rest.starts_with(':') {
            let rest = &rest[1..];
            if let Some(end_idx) = rest.find(';') {
                let notes_data = &rest[..end_idx];
                let normalized = normalize_line_endings(notes_data);
                let parts: Vec<&str> = normalized
                    .splitn(7, ':')
                    .map(str::trim)
                    .collect();

                if parts.len() >= 6 {
                    let diff_number = parts[3].parse::<i32>().unwrap_or(0);
                    charts.push(Chart {
                        steps_type: parts[0].to_string(),
                        difficulty: parts[2].to_string(),
                        difficulty_num: diff_number,
                        note_data: parts[5].to_string(),
                    });
                }
                idx += 6 + end_idx + 1;
            } else {
                break; // No matching ';'
            }
        } else {
            idx += 6;
        }
    }
    Ok(charts)
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .char_indices()
        .find(|&(i, _)| {
            haystack[i..]
                .chars()
                .zip(needle.chars())
                .all(|(h, n)| h.eq_ignore_ascii_case(&n))
        })
        .map(|(i, _)| i)
}

fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn strip_title_tags(title: &str) -> String {
    let mut s = title.trim_start();

    loop {
        if s.starts_with('[') {
            if let Some(end_bracket) = s.find(']') {
                let tag_content = &s[1..end_bracket];
                if tag_content.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    s = s[end_bracket + 1..].trim_start();
                    continue;
                }
            }
        } else {
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

fn clean_note_data(note_data: &str) -> String {
    let note_data: String = note_data
        .chars()
        .filter(|&c| !c.is_whitespace() || c == '\n')
        .collect();
    minimize_chart(&note_data)
}

fn minimize_chart(chart_string: &str) -> String {
    let mut final_chart_data = String::new();
    let mut measures = chart_string.split(',').peekable();

    while let Some(measure) = measures.next() {
        let mut lines = measure
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("//"));

        if lines.clone().next().is_some() {
            minimize_and_append_measure_iter(&mut final_chart_data, &mut lines);
            if measures.peek().is_some() {
                final_chart_data.push_str(",\n");
            }
        }
    }

    final_chart_data
}

#[inline(always)]
fn minimize_and_append_measure_iter<'a, I>(final_chart_data: &mut String, measure_lines: &mut I)
where
    I: Iterator<Item = &'a str> + Clone,
{
    let mut step = 1;
    let mut len = measure_lines.clone().count();

    while len % 2 == 0 && is_every_nth_line_empty(measure_lines.clone(), step) {
        step *= 2;
        len /= 2;
    }

    for line in measure_lines.clone().step_by(step) {
        final_chart_data.push_str(line);
        final_chart_data.push('\n');
    }
}

#[inline(always)]
fn is_every_nth_line_empty<'a, I>(lines: I, step: usize) -> bool
where
    I: Iterator<Item = &'a str>,
{
    lines
        .skip(step)
        .step_by(2 * step)
        .all(|line| line.chars().all(|c| c == '0'))
}

fn normalize_float_digits(param: &str) -> String {
    param
        .split(',')
        .filter_map(|beat_bpm| {
            let beat_bpm = beat_bpm.trim();
            if beat_bpm.is_empty() {
                None
            } else {
                let mut parts = beat_bpm.splitn(2, '=');
                let beat = parts.next()?.trim();
                let bpm = parts.next()?.trim();
                let beat = normalize_decimal(beat);
                let bpm = normalize_decimal(bpm);
                Some(format!("{}={}", beat, bpm))
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_decimal(decimal: &str) -> String {
    decimal
        .parse::<f64>()
        .map_or_else(|_| "0.000".to_string(), |f| format!("{:.3}", f))
}

fn split_measures(note_data: &str) -> impl Iterator<Item = &str> {
    note_data.split(',')
}

fn process_line(line: &str, steps_type: &str, counts: &mut NoteCounts) {
    let mut note_count = 0;

    let arrow_mapping = get_arrow_mapping(steps_type, line.len());

    for (i, c) in line.chars().enumerate() {
        match c {
            '1' => {
                counts.arrows += 1;
                note_count += 1;
                if let Some(direction) = arrow_mapping.get(i).and_then(|d| *d) {
                    increment_direction_count(direction, counts);
                }
            }
            '2' => {
                counts.arrows += 1;
                counts.holds += 1;
                note_count += 1;
                if let Some(direction) = arrow_mapping.get(i).and_then(|d| *d) {
                    increment_direction_count(direction, counts);
                }
            }
            '4' => {
                counts.arrows += 1;
                counts.rolls += 1;
                note_count += 1;
                if let Some(direction) = arrow_mapping.get(i).and_then(|d| *d) {
                    increment_direction_count(direction, counts);
                }
            }
            'M' => {
                counts.mines += 1;
            }
            _ => {}
        }
    }

    if note_count > 0 {
        counts.steps += 1;
    }

    if note_count == 2 {
        counts.jumps += 1;
    } else if note_count >= 3 {
        counts.hands += 1;
    }
}

fn increment_direction_count(direction: &str, counts: &mut NoteCounts) {
    match direction {
        "left" => counts.left += 1,
        "down" => counts.down += 1,
        "up" => counts.up += 1,
        "right" => counts.right += 1,
        _ => {}
    }
}

fn get_arrow_mapping(steps_type: &str, line_length: usize) -> Vec<Option<&'static str>> {
    match steps_type {
        "dance-single" => {
            vec![
                Some("left"),
                Some("down"),
                Some("up"),
                Some("right"),
            ]
        }
        _ => vec![None; line_length],
    }
}

fn process_measure(
    measure: &str,
    steps_type: &str,
    measure_start_beat: f64,
) -> (NoteCounts, usize, Vec<f64>, Vec<String>) {
    let mut counts = NoteCounts::default();
    let mut measure_density = 0;
    let mut note_beats = Vec::new();
    let mut measure_lines = Vec::new();

    let lines: Vec<&str> = measure.lines().collect();
    let num_lines = lines.len();

    for (i, line) in lines.iter().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut line_counts = NoteCounts::default();
        process_line(line, steps_type, &mut line_counts);
        if line_counts.arrows > 0 {
            measure_density += 1;

            // Calculate beat position of the line
            let line_beat = measure_start_beat + (i as f64 * 4.0 / num_lines as f64);
            note_beats.push(line_beat);
        }
        counts.arrows += line_counts.arrows;
        counts.steps += line_counts.steps;
        counts.mines += line_counts.mines;
        counts.holds += line_counts.holds;
        counts.rolls += line_counts.rolls;
        counts.jumps += line_counts.jumps;
        counts.hands += line_counts.hands;
        counts.left += line_counts.left;
        counts.down += line_counts.down;
        counts.up += line_counts.up;
        counts.right += line_counts.right;

        measure_lines.push(line.to_string());
    }

    (counts, measure_density, note_beats, measure_lines)
}

fn find_current_bpm(beat: f64, bpms: &[(f64, f64)]) -> f64 {
    let mut current_bpm = bpms[0].1;
    for &(bpm_beat, bpm_value) in bpms {
        if bpm_beat <= beat {
            current_bpm = bpm_value;
        } else {
            break;
        }
    }
    current_bpm
}

fn categorize_measure_density(measure_density: usize) -> RunDensity {
    match measure_density {
        d if d >= 32 => RunDensity::Run32,
        d if d >= 24 => RunDensity::Run24,
        d if d >= 20 => RunDensity::Run20,
        d if d >= 16 => RunDensity::Run16,
        _ => RunDensity::Break,
    }
}

fn generate_breakdown(measure_densities: &[usize]) -> String {
    let mut breakdown = String::new();

    let first_non_break = measure_densities
        .iter()
        .position(|&density| categorize_measure_density(density) != RunDensity::Break);

    let last_non_break = measure_densities
        .iter()
        .rposition(|&density| categorize_measure_density(density) != RunDensity::Break);

    if let (Some(start_idx), Some(end_idx)) = (first_non_break, last_non_break) {
        let mut previous_density = RunDensity::Break;
        let mut run_length = 0;

        for &density in &measure_densities[start_idx..=end_idx] {
            let current_density = categorize_measure_density(density);
            if current_density == previous_density {
                run_length += 1;
            } else {
                if run_length > 0 {
                    breakdown.push_str(&format_run(previous_density, run_length));
                }
                run_length = 1;
                previous_density = current_density;
            }
        }

        if run_length > 0 {
            breakdown.push_str(&format_run(previous_density, run_length));
        }
    }

    breakdown.trim().to_string()
}

fn format_run(density: RunDensity, length: usize) -> String {
    match density {
        RunDensity::Run32 => format!("={}= ", length),
        RunDensity::Run24 => format!("\\{}\\ ", length),
        RunDensity::Run20 => format!("~{}~ ", length),
        RunDensity::Run16 => format!("{} ", length),
        RunDensity::Break => {
            if length > 1 {
                format!("({}) ", length)
            } else {
                String::new()
            }
        }
    }
}

fn parse_token(token: &str) -> Option<BreakdownToken> {
    if token.starts_with('(') && token.ends_with(')') {
        token[1..token.len() - 1]
            .parse::<usize>()
            .ok()
            .map(|length| BreakdownToken {
                length,
                is_run: false,
                density: None,
                run_symbol: String::new(),
            })
    } else {
        let (density, length_str, run_symbol) = if let Some(number_str) = token
            .strip_prefix('=')
            .and_then(|s| s.strip_suffix('='))
        {
            (RunDensity::Run32, number_str, "=".to_string())
        } else if let Some(number_str) = token
            .strip_prefix('\\')
            .and_then(|s| s.strip_suffix('\\'))
        {
            (RunDensity::Run24, number_str, "\\".to_string())
        } else if let Some(number_str) = token.strip_prefix('~').and_then(|s| s.strip_suffix('~')) {
            (RunDensity::Run20, number_str, "~".to_string())
        } else {
            (RunDensity::Run16, token, "".to_string())
        };

        length_str.parse::<usize>().ok().map(|length| BreakdownToken {
            length,
            is_run: true,
            density: Some(density),
            run_symbol,
        })
    }
}

fn generate_simplified(detailed_breakdown: &str, partially: bool) -> String {
    let acceptable_break_length = if partially { 1 } else { 4 };

    let tokens = detailed_breakdown.split_whitespace();

    let mut simplified_tokens = Vec::new();

    let mut i = 0;
    let parsed_tokens: Vec<BreakdownToken> = tokens.filter_map(parse_token).collect();

    while i < parsed_tokens.len() {
        let token = &parsed_tokens[i];

        if token.is_run {
            let current_density = token.density.clone();
            let current_density_symbol = token.run_symbol.clone();
            let mut current_group_length = token.length;
            let mut current_group_runs = 1;
            let mut current_group_includes_breaks = false;

            let mut j = i + 1;

            while j < parsed_tokens.len() {
                let next_token = &parsed_tokens[j];

                if !next_token.is_run {
                    let break_length = next_token.length;
                    if break_length <= acceptable_break_length {
                        current_group_length += break_length;
                        current_group_includes_breaks = true;
                        j += 1;
                    } else {
                        break;
                    }
                } else if next_token.density == current_density {
                    if parsed_tokens[j - 1].is_run {
                        let implied_break_length = 1;
                        if implied_break_length <= acceptable_break_length {
                            current_group_length += implied_break_length;
                            current_group_includes_breaks = true;
                        } else {
                            break;
                        }
                    }
                    current_group_length += next_token.length;
                    current_group_runs += 1;
                    j += 1;
                } else {
                    break;
                }
            }

            let run_symbol = current_density_symbol.as_str();
            let formatted_run = if current_group_runs > 1 || current_group_includes_breaks {
                format!("{}{}{}*", run_symbol, current_group_length, run_symbol)
            } else {
                format!("{}{}{}", run_symbol, current_group_length, run_symbol)
            };
            simplified_tokens.push(formatted_run);

            i = j;
        } else {
            let break_length = token.length;
            let break_symbol = if break_length > 32 {
                "|".to_string()
            } else if break_length > 4 {
                "/".to_string()
            } else {
                "-".to_string()
            };
            simplified_tokens.push(break_symbol);
            i += 1;
        }
    }

    simplified_tokens.join(" ")
}

fn generate_breakdown_counts(measure_densities: &[usize], stream_counts: &mut StreamCounts) {
    let first_non_break = measure_densities
        .iter()
        .position(|&density| categorize_measure_density(density) != RunDensity::Break);

    let last_non_break = measure_densities
        .iter()
        .rposition(|&density| categorize_measure_density(density) != RunDensity::Break);

    if let (Some(start_idx), Some(end_idx)) = (first_non_break, last_non_break) {
        for &density in &measure_densities[start_idx..=end_idx] {
            match categorize_measure_density(density) {
                RunDensity::Run32 => stream_counts.run32_streams += 1,
                RunDensity::Run24 => stream_counts.run24_streams += 1,
                RunDensity::Run20 => stream_counts.run20_streams += 1,
                RunDensity::Run16 => stream_counts.run16_streams += 1,
                RunDensity::Break => stream_counts.total_breaks += 1,
            }
        }
    }
}

/// Rounds a floating-point number to the specified number of decimal places.
/// Uses the "round half up" strategy.
fn round_half_up(value: f64, decimals: u32) -> f64 {
    let multiplier = 10_f64.powi(decimals as i32);
    (value * multiplier).round() / multiplier
}

fn process_chart(
    notes: &str,
    steps_type: &str,
    bpms: &[(f64, f64)],
) -> (
    NoteCounts,
    String,
    String,
    String,
    StreamCounts,
    Vec<f64>,
    f64,
    PatternInfo,
) {
    let measures: Vec<&str> = split_measures(notes).collect();

    let mut total_counts = NoteCounts::default();
    let mut measure_densities = Vec::new();
    let mut measure_nps = Vec::new();
    let mut length_in_seconds = 0.0;
    let mut all_measures_lines: Vec<String> = Vec::new();

    let mut measure_index = 0;

    for measure in &measures {
        let measure_start_beat = measure_index as f64 * 4.0;
        let bpm = find_current_bpm(measure_start_beat, bpms);

        let (counts, measure_density, _, measure_lines) = process_measure(measure, steps_type, measure_start_beat);
        total_counts.arrows += counts.arrows;
        total_counts.steps += counts.steps;
        total_counts.mines += counts.mines;
        total_counts.holds += counts.holds;
        total_counts.rolls += counts.rolls;
        total_counts.jumps += counts.jumps;
        total_counts.hands += counts.hands;
        total_counts.left += counts.left;
        total_counts.down += counts.down;
        total_counts.up += counts.up;
        total_counts.right += counts.right;

        measure_densities.push(measure_density);

        let nps = (bpm / 4.0) * (measure_density as f64) / 60.0;
        measure_nps.push(nps);

        // Accumulate length for each measure
        length_in_seconds += (4.0 / bpm) * 60.0;

        // Collect all measure lines
        all_measures_lines.extend(measure_lines);

        measure_index += 1;
    }

    total_counts.total_measures = measure_index;

    let mut stream_counts = StreamCounts::default();
    generate_breakdown_counts(&measure_densities, &mut stream_counts);

    let detailed_breakdown = generate_breakdown(&measure_densities);
    let partially_simplified = generate_simplified(&detailed_breakdown, true);
    let simplified = generate_simplified(&detailed_breakdown, false);

    // Build arrows_per_line for pattern analysis
    let arrows_per_line = build_arrows_per_line(&all_measures_lines);

    // Perform pattern analysis
    let pattern_info = perform_pattern_analysis(&arrows_per_line, total_counts.arrows);

    (
        total_counts,
        detailed_breakdown,
        partially_simplified,
        simplified,
        stream_counts,
        measure_nps,
        length_in_seconds,
        pattern_info,
    )
}

fn parse_line_to_arrows(line: &str) -> Vec<usize> {
    let mut arrows = Vec::new();
    for (i, c) in line.chars().enumerate() {
        match c {
            '1' | '2' | '4' => arrows.push(i),
            _ => {},
        }
    }
    arrows
}

fn build_arrows_per_line(chart_lines: &[String]) -> Vec<Vec<usize>> {
    let mut arrows_per_line = Vec::new();
    for line in chart_lines {
        let arrows = parse_line_to_arrows(line);
        arrows_per_line.push(arrows);
    }
    arrows_per_line
}

fn count_left_foot_candles(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    for i in 0..(arrows_per_line.len() - 2) {
        let line1 = &arrows_per_line[i];
        let line2 = &arrows_per_line[i + 1];
        let line3 = &arrows_per_line[i + 2];

        // Pattern 1: Down, Right, Up
        if line1.contains(&1) && line2.contains(&3) && line3.contains(&2) {
            count += 1;
        }
        // Pattern 2: Up, Right, Down
        else if line1.contains(&2) && line2.contains(&3) && line3.contains(&1) {
            count += 1;
        }
    }
    count
}

fn count_right_foot_candles(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    for i in 0..(arrows_per_line.len() - 2) {
        let line1 = &arrows_per_line[i];
        let line2 = &arrows_per_line[i + 1];
        let line3 = &arrows_per_line[i + 2];

        // Pattern 1: Down, Left, Up
        if line1.contains(&1) && line2.contains(&0) && line3.contains(&2) {
            count += 1;
        }
        // Pattern 2: Up, Left, Down
        else if line1.contains(&2) && line2.contains(&0) && line3.contains(&1) {
            count += 1;
        }
    }
    count
}

fn count_ld_ru_mono(arrows_per_line: &[Vec<usize>]) -> usize {
    let patterns = vec![
        vec![0, 2, 1, 3],
        vec![0, 3, 1, 2],
        vec![1, 2, 0, 3],
        vec![1, 3, 0, 2],
        vec![2, 0, 3, 1],
        vec![2, 1, 3, 0],
        vec![3, 0, 2, 1],
        vec![3, 1, 2, 0],
    ];

    let mut count = 0;
    let mut i = 0;
    while i <= arrows_per_line.len().saturating_sub(4) {
        let line1 = &arrows_per_line[i];
        let line2 = &arrows_per_line[i + 1];
        let line3 = &arrows_per_line[i + 2];
        let line4 = &arrows_per_line[i + 3];

        // Ensure each line contains exactly one arrow
        if line1.len() == 1 && line2.len() == 1 && line3.len() == 1 && line4.len() == 1 {
            let sequence = vec![line1[0], line2[0], line3[0], line4[0]];

            if patterns.contains(&sequence) {
                count += 1;
                i += 4; // Skip ahead by the pattern length to avoid overlap
                continue;
            }
        }
        i += 1; // Move to the next step
    }
    count
}

fn count_lu_rd_mono(arrows_per_line: &[Vec<usize>]) -> usize {
    let patterns = vec![
        vec![0, 1, 2, 3],
        vec![0, 3, 2, 1],
        vec![2, 1, 0, 3],
        vec![2, 3, 0, 1],
        vec![1, 0, 3, 2],
        vec![1, 2, 3, 0],
        vec![3, 0, 1, 2],
        vec![3, 2, 1, 0],
    ];

    let mut count = 0;
    let mut i = 0;
    while i <= arrows_per_line.len().saturating_sub(4) {
        let line1 = &arrows_per_line[i];
        let line2 = &arrows_per_line[i + 1];
        let line3 = &arrows_per_line[i + 2];
        let line4 = &arrows_per_line[i + 3];

        // Ensure each line contains exactly one arrow
        if line1.len() == 1 && line2.len() == 1 && line3.len() == 1 && line4.len() == 1 {
            let sequence = vec![line1[0], line2[0], line3[0], line4[0]];

            if patterns.contains(&sequence) {
                count += 1;
                i += 4; // Skip ahead by the pattern length to avoid overlap
                continue;
            }
        }
        i += 1; // Move to the next step
    }
    count
}

fn count_anchors(arrows_per_line: &[Vec<usize>], anchor_arrow: usize) -> usize {
    let mut count = 0;
    let mut i = 0;

    while i <= arrows_per_line.len().saturating_sub(5) {
        if arrows_per_line[i].contains(&anchor_arrow) &&
           arrows_per_line[i + 2].contains(&anchor_arrow) &&
           arrows_per_line[i + 4].contains(&anchor_arrow)
        {
            count += 1;
            i += 5; // Skip steps to prevent overlapping
        } else {
            i += 1;
        }
    }
    count
}

fn count_lr_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Left, Right, Left, Right
        if arrows_per_line[i].contains(&0) &&
           arrows_per_line[i + 1].contains(&3) &&
           arrows_per_line[i + 2].contains(&0) &&
           arrows_per_line[i + 3].contains(&3) {
            count += 1;
        }
        // Pattern 2: Right, Left, Right, Left
        else if arrows_per_line[i].contains(&3) &&
                arrows_per_line[i + 1].contains(&0) &&
                arrows_per_line[i + 2].contains(&3) &&
                arrows_per_line[i + 3].contains(&0) {
            count += 1;
        }
    }
    count
}

fn count_ud_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Up, Down, Up, Down
        if arrows_per_line[i].contains(&2) &&
           arrows_per_line[i + 1].contains(&1) &&
           arrows_per_line[i + 2].contains(&2) &&
           arrows_per_line[i + 3].contains(&1) {
            count += 1;
        }
        // Pattern 2: Down, Up, Down, Up
        else if arrows_per_line[i].contains(&1) &&
                arrows_per_line[i + 1].contains(&2) &&
                arrows_per_line[i + 2].contains(&1) &&
                arrows_per_line[i + 3].contains(&2) {
            count += 1;
        }
    }
    count
}

fn count_corner_ld_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Left, Down, Left, Down
        if arrows_per_line[i].contains(&0) &&
           arrows_per_line[i + 1].contains(&1) &&
           arrows_per_line[i + 2].contains(&0) &&
           arrows_per_line[i + 3].contains(&1) {
            count += 1;
        }
        // Pattern 2: Down, Left, Down, Left
        else if arrows_per_line[i].contains(&1) &&
                arrows_per_line[i + 1].contains(&0) &&
                arrows_per_line[i + 2].contains(&1) &&
                arrows_per_line[i + 3].contains(&0) {
            count += 1;
        }
    }
    count
}

fn count_corner_lu_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Left, Up, Left, Up
        if arrows_per_line[i].contains(&0) &&
           arrows_per_line[i + 1].contains(&2) &&
           arrows_per_line[i + 2].contains(&0) &&
           arrows_per_line[i + 3].contains(&2) {
            count += 1;
        }
        // Pattern 2: Up, Left, Up, Left
        else if arrows_per_line[i].contains(&2) &&
                arrows_per_line[i + 1].contains(&0) &&
                arrows_per_line[i + 2].contains(&2) &&
                arrows_per_line[i + 3].contains(&0) {
            count += 1;
        }
    }
    count
}

fn count_corner_rd_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Right, Down, Right, Down
        if arrows_per_line[i].contains(&3) &&
           arrows_per_line[i + 1].contains(&1) &&
           arrows_per_line[i + 2].contains(&3) &&
           arrows_per_line[i + 3].contains(&1) {
            count += 1;
        }
        // Pattern 2: Down, Right, Down, Right
        else if arrows_per_line[i].contains(&1) &&
                arrows_per_line[i + 1].contains(&3) &&
                arrows_per_line[i + 2].contains(&1) &&
                arrows_per_line[i + 3].contains(&3) {
            count += 1;
        }
    }
    count
}

fn count_corner_ru_boxes(arrows_per_line: &[Vec<usize>]) -> usize {
    let mut count = 0;
    let pattern_length = 4;
    for i in 0..arrows_per_line.len().saturating_sub(pattern_length - 1) {
        // Pattern 1: Right, Up, Right, Up
        if arrows_per_line[i].contains(&3) &&
           arrows_per_line[i + 1].contains(&2) &&
           arrows_per_line[i + 2].contains(&3) &&
           arrows_per_line[i + 3].contains(&2) {
            count += 1;
        }
        // Pattern 2: Up, Right, Up, Right
        else if arrows_per_line[i].contains(&2) &&
                arrows_per_line[i + 1].contains(&3) &&
                arrows_per_line[i + 2].contains(&2) &&
                arrows_per_line[i + 3].contains(&3) {
            count += 1;
        }
    }
    count
}

fn perform_pattern_analysis(arrows_per_line: &[Vec<usize>], num_arrows: usize) -> PatternInfo {
    let left_foot_candles = count_left_foot_candles(arrows_per_line);
    let right_foot_candles = count_right_foot_candles(arrows_per_line);
    let total_candles = left_foot_candles + right_foot_candles;

    let candles_percent = if num_arrows > 1 {
        (total_candles as f64 / ((num_arrows - 1) as f64 / 2.0).floor()) * 100.0
    } else {
        0.0
    };

    let ld_ru_mono = count_ld_ru_mono(arrows_per_line);
    let lu_rd_mono = count_lu_rd_mono(arrows_per_line);

    let total_mono_arrows = (ld_ru_mono + lu_rd_mono) * 4;

    let mono_percent = if num_arrows > 0 {
        (total_mono_arrows as f64 / num_arrows as f64) * 100.0
    } else {
        0.0
    };

    let lr_boxes = count_lr_boxes(arrows_per_line);
    let ud_boxes = count_ud_boxes(arrows_per_line);
    let corner_ld_boxes = count_corner_ld_boxes(arrows_per_line);
    let corner_lu_boxes = count_corner_lu_boxes(arrows_per_line);
    let corner_rd_boxes = count_corner_rd_boxes(arrows_per_line);
    let corner_ru_boxes = count_corner_ru_boxes(arrows_per_line);

    let anchor_left = count_anchors(arrows_per_line, 0);
    let anchor_down = count_anchors(arrows_per_line, 1);
    let anchor_up = count_anchors(arrows_per_line, 2);
    let anchor_right = count_anchors(arrows_per_line, 3);

    PatternInfo {
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
    }
}

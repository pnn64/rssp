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
}

struct Chart {
    steps_type: String,
    difficulty: String,
    difficulty_num: i32,
    note_data: String,
}

struct NoteCounts {
    notes: usize,
    mines: usize,
    holds: usize,
    rolls: usize,
    jumps: usize,
    hands: usize,
}

#[derive(Debug, Clone)]
struct BreakdownToken {
    length: usize,
    is_run: bool,
    density: Option<RunDensity>,
    original: String,
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
                for chart in simfile.charts {
                    let notes = clean_note_data(&chart.note_data);
                    let data_to_hash = [notes.as_str(), &simfile.bpms].concat();

                    let hash_result = Sha1::digest(data_to_hash.as_bytes());
                    let hash_hex = hex::encode(hash_result)[..16].to_string();

                    // Process the chart to get counts and breakdowns
                    let (counts, detailed_breakdown, partially_simplified, simplified) =
                        process_chart(&notes);

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
                        "notes": counts.notes,
                        "mines": counts.mines,
                        "holds": counts.holds,
                        "rolls": counts.rolls,
                        "jumps": counts.jumps,
                        "hands": counts.hands,
                        "detailed_breakdown": detailed_breakdown,
                        "partially_simplified_breakdown": partially_simplified,
                        "simplified_breakdown": simplified,
                    });

                    println!("\nChart Info:");
                    match serde_json::to_string_pretty(&chart_info) {
                        Ok(json_str) => println!("{}", json_str),
                        Err(e) => eprintln!("Error marshaling JSON: {}", e),
                    }
                }
            }
            Err(e) => eprintln!("Error parsing simfile: {}", e),
        }
    } else {
        eprintln!("Invalid simfile: {}", filename);
    }
}

fn get_simfile_content(filename: &str) -> Option<String> {
    let path = Path::new(filename);
    let ext = path.extension()?.to_str()?.to_lowercase();
    if ext != "sm" && ext != "ssc" {
        return None;
    }
    fs::read_to_string(filename)
        .map(|content| content.trim_start_matches('\u{FEFF}').to_string())
        .ok()
}

fn parse_simfile(
    content: &str,
    strip_tags: bool,
) -> Result<Simfile, Box<dyn std::error::Error>> {
    let content = remove_comments(content);
    let directives = parse_directives(&content);

    let mut metadata = parse_metadata(&directives);

    if strip_tags {
        if let Some(title) = metadata.get("title") {
            let cleaned_title = strip_title_tags(title);
            metadata.insert("title".to_string(), cleaned_title);
        }
    }

    let bpms = parse_bpms(&content);

    let charts = parse_charts(&content)?;

    Ok(Simfile {
        metadata,
        charts,
        bpms,
    })
}

fn remove_comments(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '/' {
            if let Some('/') = chars.peek() {
                chars.next(); // Consume second '/'
                // Skip until newline or end of input
                while let Some(c) = chars.next() {
                    if c == '\n' {
                        result.push(c);
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

fn parse_directives(content: &str) -> Vec<String> {
    content
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut directive = s.to_string();
            if !directive.ends_with(';') {
                directive.push(';');
            }
            directive
        })
        .collect()
}

fn starts_with_case_insensitive(s: &str, prefix: &str) -> bool {
    s.chars()
        .zip(prefix.chars())
        .all(|(sc, pc)| sc.eq_ignore_ascii_case(&pc))
}

fn parse_metadata(directives: &[String]) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    for directive in directives {
        for &key in METADATA_KEYS {
            if starts_with_case_insensitive(directive, key) {
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
    let rest = &directive[key.len()..].trim_start();
    if rest.starts_with(':') {
        let rest = &rest[1..];
        let value = rest.trim_end_matches(';').trim();
        Some(value.to_string())
    } else {
        None
    }
}

fn extract_value(content: &str, key: &str) -> Option<String> {
    let mut idx = 0;

    while let Some(pos) = find_case_insensitive(&content[idx..], key) {
        idx += pos;
        let next_char_idx = idx + key.len();
        if content[next_char_idx..].trim_start().starts_with(':') {
            let rest = &content[next_char_idx..];
            let rest = rest.trim_start();
            if rest.starts_with(':') {
                let rest = &rest[1..];
                if let Some(end_idx) = rest.find(';') {
                    let value = &rest[..end_idx];
                    return Some(value.trim().to_string());
                }
            }
        }
        idx += key.len();
    }
    None
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_lowercase()
        .find(&needle.to_lowercase())
}

fn parse_bpms(content: &str) -> String {
    if let Some(value) = extract_value(content, "#BPMS") {
        let bpm_data = value.replace(['\n', '\r'], "");
        normalize_float_digits(&bpm_data)
    } else {
        "0.000=0.000".to_string()
    }
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
                idx += 6 + rest[..end_idx + 1].len();
            } else {
                break; // No matching ';'
            }
        } else {
            idx += 6;
        }
    }
    Ok(charts)
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
    let mut final_chart_data = Vec::new();
    let mut cur_measure = Vec::new();

    for line in chart_string.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line == "," {
            final_chart_data.extend(minimize_measure(&cur_measure));
            final_chart_data.push(",".to_string());
            cur_measure.clear();
        } else {
            cur_measure.push(line.to_string());
        }
    }

    if !cur_measure.is_empty() {
        final_chart_data.extend(minimize_measure(&cur_measure));
    }

    final_chart_data.join("\n")
}

fn minimize_measure(measure: &[String]) -> Vec<String> {
    let mut measure = measure.to_vec();

    loop {
        if measure.len() % 2 != 0 {
            break;
        }
        if measure
            .iter()
            .skip(1)
            .step_by(2)
            .all(|line| line.chars().all(|c| c == '0'))
        {
            measure = measure.iter().step_by(2).cloned().collect();
        } else {
            break;
        }
    }

    measure
}

fn normalize_float_digits(param: &str) -> String {
    param
        .split(',')
        .filter_map(|beat_bpm| {
            let beat_bpm = beat_bpm.trim();
            if beat_bpm.is_empty() {
                None
            } else {
                let parts: Vec<&str> = beat_bpm.splitn(2, '=').collect();
                if parts.len() == 2 {
                    let beat = normalize_decimal(parts[0]);
                    let bpm = normalize_decimal(parts[1]);
                    Some(format!("{}={}", beat, bpm))
                } else {
                    None
                }
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_decimal(decimal: &str) -> String {
    remove_control_characters(decimal)
        .parse::<f64>()
        .map_or_else(|_| "0.000".to_string(), |f| format!("{:.3}", f))
}

fn remove_control_characters(s: &str) -> String {
    s.chars()
        .filter(|&c| !c.is_control() || c == '\n')
        .collect()
}

fn split_measures(note_data: &str) -> Vec<&str> {
    note_data.split(',').collect()
}

fn process_line(line: &str, counts: &mut NoteCounts, holding: &mut usize) {
    let mut note_count = 0;

    for c in line.chars() {
        match c {
            '1' => {
                counts.notes += 1;
                note_count += 1;
            }
            '2' => {
                counts.notes += 1;
                counts.holds += 1;
                *holding += 1;
                note_count += 1;
            }
            '3' => {
                // Hold end
                if *holding > 0 {
                    *holding -= 1;
                }
            }
            '4' => {
                counts.notes += 1;
                counts.rolls += 1;
                *holding += 1;
                note_count += 1;
            }
            'M' => {
                counts.mines += 1;
            }
            _ => {}
        }
    }

    // Count jumps and hands
    if note_count >= 2 {
        counts.jumps += 1;
    }
    if note_count + *holding >= 3 {
        counts.hands += 1;
    }
}

fn process_measure(
    measure: &str,
    counts: &mut NoteCounts,
    holding: &mut usize,
    measure_densities: &mut Vec<usize>,
) {
    let mut measure_density = 0;
    for line in measure.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut has_note = false;
        for c in line.chars() {
            if c == '1' || c == '2' || c == '4' {
                has_note = true;
                break;
            }
        }
        if has_note {
            measure_density += 1;
        }
        process_line(line, counts, holding);
    }
    measure_densities.push(measure_density);
}

fn categorize_measure_density(measure_density: usize) -> RunDensity {
    if measure_density >= 32 {
        RunDensity::Run32
    } else if measure_density >= 24 {
        RunDensity::Run24
    } else if measure_density >= 20 {
        RunDensity::Run20
    } else if measure_density >= 16 {
        RunDensity::Run16
    } else {
        RunDensity::Break
    }
}

fn generate_breakdown(measure_densities: &[usize]) -> String {
    let mut breakdown = String::new();

    // Find the index of the first non-break measure
    let first_non_break = measure_densities
        .iter()
        .position(|&density| categorize_measure_density(density) != RunDensity::Break);

    // Find the index of the last non-break measure
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

        // Append the last run
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
        // It's a break
        token[1..token.len() - 1].parse::<usize>().ok().map(|length| BreakdownToken {
            length,
            is_run: false,
            density: None,
            original: token.to_string(),
            run_symbol: String::new(),
        })
    } else {
        // It's a run
        let (density, length_str, run_symbol) = if let Some(number_str) = token.strip_prefix('=').and_then(|s| s.strip_suffix('=')) {
            (RunDensity::Run32, number_str, "=".to_string())
        } else if let Some(number_str) = token.strip_prefix('\\').and_then(|s| s.strip_suffix('\\')) {
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
            original: token.to_string(),
            run_symbol,
        })
    }
}

fn generate_simplified(detailed_breakdown: &str, partially: bool) -> String {
    let tokens: Vec<&str> = detailed_breakdown.split_whitespace().collect();

    // Parse tokens into BreakdownTokens
    let parsed_tokens: Vec<BreakdownToken> = tokens.iter()
        .filter_map(|&token| parse_token(token))
        .collect();

    let mut simplified_tokens = Vec::new();
    let mut current_group_length = 0;
    let mut current_group_runs = 0;
    let mut current_group_includes_breaks = false;
    let mut current_density = None;
    let mut current_density_symbol = None::<String>;

    let mut previous_token_was_run = false;
    let mut i = 0;

    while i < parsed_tokens.len() {
        let token = &parsed_tokens[i];

        if token.is_run {
            if previous_token_was_run {
                // Implied break of length 1 between consecutive runs
                let implied_break_length = 1;
                if (partially && implied_break_length <= 1) || (!partially && implied_break_length <= 4) {
                    // Include implied break in current group
                    current_group_length += implied_break_length;
                    current_group_includes_breaks = true;
                } else {
                    // Break is too long; output current group
                    if current_group_length > 0 {
                        // Output the group
                        let run_symbol = current_density_symbol.as_deref().unwrap_or("");
                        let formatted_run = if current_group_runs > 1 || current_group_includes_breaks {
                            format!("{}{}{}*", run_symbol, current_group_length, run_symbol)
                        } else {
                            format!("{}{}{}", run_symbol, current_group_length, run_symbol)
                        };
                        simplified_tokens.push(formatted_run);
                    }
                    // Append appropriate break symbol
                    simplified_tokens.push("-".to_string());
                    // Reset current group
                    current_group_length = 0;
                    current_group_runs = 0;
                    current_group_includes_breaks = false;
                    current_density = None;
                    current_density_symbol = None;
                }
            }

            // If the current group is empty, start a new group
            if current_group_runs == 0 {
                current_density = token.density.clone();
                current_density_symbol = Some(token.run_symbol.clone());
            }

            // Check if the run density matches the current group
            if token.density == current_density {
                // Add to current group
                current_group_length += token.length;
                current_group_runs += 1;
            } else {
                // Different density, but we've already handled implied breaks, so start a new group
                if current_group_length > 0 {
                    // Output the group
                    let run_symbol = current_density_symbol.as_deref().unwrap_or("");
                    let formatted_run = if current_group_runs > 1 || current_group_includes_breaks {
                        format!("{}{}{}*", run_symbol, current_group_length, run_symbol)
                    } else {
                        format!("{}{}{}", run_symbol, current_group_length, run_symbol)
                    };
                    simplified_tokens.push(formatted_run);
                }
                // Start a new group with this run
                current_group_length = token.length;
                current_group_runs = 1;
                current_group_includes_breaks = false;
                current_density = token.density.clone();
                current_density_symbol = Some(token.run_symbol.clone());
            }
            previous_token_was_run = true;
        } else {
            // It's a break
            let break_length = token.length;
            let include_break_in_group = if partially {
                break_length <= 1
            } else {
                break_length <= 4
            };

            if include_break_in_group {
                // Include break in current group
                current_group_length += break_length;
                current_group_includes_breaks = true;
            } else {
                // Break is too long, output current group
                if current_group_length > 0 {
                    let run_symbol = current_density_symbol.as_deref().unwrap_or("");
                    let formatted_run = if current_group_runs > 1 || current_group_includes_breaks {
                        format!("{}{}{}*", run_symbol, current_group_length, run_symbol)
                    } else {
                        format!("{}{}{}", run_symbol, current_group_length, run_symbol)
                    };
                    simplified_tokens.push(formatted_run);
                }
                // Append appropriate break symbol
                let break_symbol = if break_length > 32 {
                    "|".to_string()
                } else if break_length > 4 {
                    "/".to_string()
                } else {
                    "-".to_string()
                };
                simplified_tokens.push(break_symbol);
                // Reset current group
                current_group_length = 0;
                current_group_runs = 0;
                current_group_includes_breaks = false;
                current_density = None;
                current_density_symbol = None;
            }
            previous_token_was_run = false;
        }

        i += 1;
    }

    // Output any remaining group
    if current_group_length > 0 {
        let run_symbol = current_density_symbol.as_deref().unwrap_or("");
        let formatted_run = if current_group_runs > 1 || current_group_includes_breaks {
            format!("{}{}{}*", run_symbol, current_group_length, run_symbol)
        } else {
            format!("{}{}{}", run_symbol, current_group_length, run_symbol)
        };
        simplified_tokens.push(formatted_run);
    }

    simplified_tokens.join(" ")
}

fn process_chart(
    notes: &str,
) -> (
    NoteCounts,
    String,
    String,
    String,
) {
    let measures = split_measures(notes);

    let mut counts = NoteCounts {
        notes: 0,
        mines: 0,
        holds: 0,
        rolls: 0,
        jumps: 0,
        hands: 0,
    };
    let mut holding = 0;
    let mut measure_densities = Vec::new();

    for measure in &measures {
        process_measure(measure, &mut counts, &mut holding, &mut measure_densities);
    }

    let detailed_breakdown = generate_breakdown(&measure_densities);
    let partially_simplified = generate_simplified(&detailed_breakdown, true);
    let simplified = generate_simplified(&detailed_breakdown, false);

    (
        counts,
        detailed_breakdown,
        partially_simplified,
        simplified,
    )
}

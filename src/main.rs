use std::collections::HashMap;
use std::fs;
use std::path::Path;
use sha1::{Digest, Sha1};
use serde_json::json;
use clap::Parser;

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

fn parse_simfile(content: &str, strip_tags: bool) -> Result<Simfile, Box<dyn std::error::Error>> {
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

    let charts = parse_charts(&content);

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

fn parse_charts(content: &str) -> Vec<Chart> {
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
    charts
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

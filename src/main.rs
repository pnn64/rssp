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
                    let data_to_hash = format!("{}{}", notes, simfile.bpms);

                    let hash_result = Sha1::digest(data_to_hash.as_bytes());
                    let hash_hex = hex::encode(hash_result)[..16].to_string();

                    let chart_info = json!({
                        "title": simfile.metadata.get("title").cloned().unwrap_or_default(),
                        "titletranslit": simfile.metadata.get("titletranslit").cloned().unwrap_or_default(),
                        "subtitle": simfile.metadata.get("subtitle").cloned().unwrap_or_default(),
                        "subtitletranslit": simfile.metadata.get("subtitletranslit").cloned().unwrap_or_default(),
                        "artist": simfile.metadata.get("artist").cloned().unwrap_or_default(),
                        "artisttranslit": simfile.metadata.get("artisttranslit").cloned().unwrap_or_default(),
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
    s.lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                &line[..idx]
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_directives(content: &str) -> Vec<String> {
    content
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("{};", s)) // Add back the ';' at the end
        .collect()
}

fn parse_metadata(directives: &[String]) -> HashMap<String, String> {
    let mut metadata = HashMap::new();
    for directive in directives {
        for &key in METADATA_KEYS {
            if directive.to_uppercase().starts_with(key) {
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

fn parse_bpms(content: &str) -> String {
    if let Some(value) = extract_value(content, "#BPMS") {
        let bpm_data = value.replace("\n", "").replace("\r", "");
        normalize_float_digits(&bpm_data)
    } else {
        "0.000=0.000".to_string()
    }
}

fn extract_value(content: &str, key: &str) -> Option<String> {
    let upper_key = key.to_uppercase();
    let mut idx = 0;
    let content_upper = content.to_uppercase();

    while let Some(pos) = content_upper[idx..].find(&upper_key) {
        idx += pos;
        let next_char_idx = idx + key.len();
        if content_upper[next_char_idx..].trim_start().starts_with(':') {
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

fn parse_charts(content: &str) -> Vec<Chart> {
    let mut charts = Vec::new();
    let mut idx = 0;

    let content_upper = content.to_uppercase();
    while let Some(pos) = content_upper[idx..].find("#NOTES") {
        idx += pos;
        let rest = &content[idx + 6..]; // skip past "#NOTES"
        let rest = rest.trim_start();
        if rest.starts_with(':') {
            let rest = &rest[1..];
            // Now, we need to extract until the matching ';'
            if let Some(end_idx) = rest.find(';') {
                let notes_data = &rest[..end_idx];
                let normalized = normalize_line_endings(notes_data);
                let parts: Vec<String> = normalized
                    .splitn(7, ':')
                    .map(|s| s.trim().to_string())
                    .collect();

                if parts.len() >= 6 {
                    let diff_number = parts[3].parse::<i32>().unwrap_or(0);
                    charts.push(Chart {
                        steps_type: parts[0].clone(),
                        difficulty: parts[2].clone(),
                        difficulty_num: diff_number,
                        note_data: parts[5].clone(),
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
    let mut chars = title.chars().peekable();
    loop {
        // Check for '['
        if let Some(&c) = chars.peek() {
            if c == '[' {
                // Consume '['
                chars.next();
                // Read digits and possible '.'
                while let Some(&c) = chars.peek() {
                    if c.is_digit(10) || c == '.' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Expect ']'
                if chars.peek() == Some(&']') {
                    chars.next();
                    // Consume any whitespace
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    continue;
                } else {
                    // No matching ']', break
                    break;
                }
            } else if c.is_digit(10) {
                // Consume digits
                while let Some(&c) = chars.peek() {
                    if c.is_digit(10) {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Expect '-'
                if chars.peek() == Some(&'-') {
                    chars.next();
                    // Consume any whitespace
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    continue;
                } else {
                    break;
                }
            } else {
                break;
            }
        } else {
            break;
        }
    }
    chars.collect()
}

fn clean_note_data(note_data: &str) -> String {
    let note_data = remove_comments(note_data);
    let note_data = remove_whitespace(&note_data);
    minimize_chart(&note_data)
}

fn remove_whitespace(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() || *c == '\n') // Keep newlines
        .collect()
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

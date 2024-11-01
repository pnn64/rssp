use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use sha1::{Digest, Sha1};
use regex::Regex;
use serde_json::json;
use lazy_static::lazy_static;

lazy_static! {
    static ref BPM_PATTERN: Regex = Regex::new(r"(?i)(?s)#BPMS\s*:\s*(.*?);").unwrap();
    static ref NOTES_PATTERN: Regex = Regex::new(r"(?i)(?s)#NOTES\s*:(.*?);").unwrap();
    static ref COMMENT_PATTERN: Regex = Regex::new(r"//[^\n]*").unwrap();
    static ref WHITESPACE_RE: Regex = Regex::new(r"[\r\t\f\v ]+").unwrap();
    static ref METADATA_RE: HashMap<&'static str, Regex> = {
        let mut m = HashMap::new();
        m.insert("title", Regex::new(r"(?i)#TITLE\s*:\s*(.*?);").unwrap());
        m.insert("titletranslit", Regex::new(r"(?i)#TITLETRANSLIT\s*:\s*(.*?);").unwrap());
        m.insert("subtitle", Regex::new(r"(?i)#SUBTITLE\s*:\s*(.*?);").unwrap());
        m.insert("subtitletranslit", Regex::new(r"(?i)#SUBTITLETRANSLIT\s*:\s*(.*?);").unwrap());
        m.insert("artist", Regex::new(r"(?i)#ARTIST\s*:\s*(.*?);").unwrap());
        m.insert("artisttranslit", Regex::new(r"(?i)#ARTISTTRANSLIT\s*:\s*(.*?);").unwrap());
        m
    };
}

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
    let args = env::args().collect::<Vec<_>>();
    if let Some(filename) = args.get(1) {
        process_file(filename);
    } else {
        eprintln!("Usage: rssp <simfile.sm>");
    }
}

fn process_file(filename: &str) {
    if let Some(content) = get_simfile_content(filename) {
        match parse_simfile(&content) {
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

fn parse_simfile(content: &str) -> Result<Simfile, Box<dyn std::error::Error>> {
    let metadata = METADATA_RE
        .iter()
        .filter_map(|(key, pattern)| {
            get_first_match(content, pattern).map(|value| (key.to_string(), value))
        })
        .collect();

    let bpms = if let Some(caps) = BPM_PATTERN.captures(content) {
        let bpm_data = caps.get(1).unwrap().as_str().replace("\n", "").replace("\r", "");
        normalize_float_digits(&bpm_data)
    } else {
        "0.000=0.000".to_string()
    };

    let charts = NOTES_PATTERN
        .captures_iter(content)
        .filter_map(|caps| {
            let note_data = caps.get(1)?.as_str();
            let normalized = normalize_line_endings(note_data);
            let parts: Vec<String> = normalized.splitn(7, ':')
                .map(|s| s.trim().to_string())
                .collect();

            if parts.len() >= 6 {
                let diff_number = parts[3].parse::<i32>().unwrap_or(0);
                Some(Chart {
                    steps_type: parts[0].clone(),
                    difficulty: parts[2].clone(),
                    difficulty_num: diff_number,
                    note_data: parts[5].clone(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(Simfile {
        metadata,
        charts,
        bpms,
    })
}

fn clean_note_data(note_data: &str) -> String {
    let note_data = COMMENT_PATTERN.replace_all(note_data, "");
    let note_data = WHITESPACE_RE.replace_all(&note_data, "");
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
        if measure.iter().skip(1).step_by(2).all(|line| line.chars().all(|c| c == '0')) {
            measure = measure.iter().step_by(2).cloned().collect();
        } else {
            break;
        }
    }

    measure
}

fn normalize_line_endings(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
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

fn get_first_match(s: &str, pattern: &Regex) -> Option<String> {
    pattern
        .captures(s)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
}

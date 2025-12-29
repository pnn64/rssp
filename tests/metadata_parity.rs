use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::parse::{
    clean_tag,
    extract_sections,
    split_notes_fields,
    strip_title_tags,
    unescape_tag,
    unescape_trim,
};
use rssp::translate::replace_markers_in_place;

#[derive(Debug, Deserialize)]
struct GoldenMetadata {
    title: Option<String>,
    subtitle: Option<String>,
    artist: Option<String>,
    #[serde(rename = "title_translated")]
    title_translated: Option<String>,
    #[serde(rename = "subtitle_translated")]
    subtitle_translated: Option<String>,
    #[serde(rename = "artist_translated")]
    artist_translated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoldenChartStepArtist {
    #[serde(rename = "steps_type")]
    step_type: String,
    difficulty: String,
    #[serde(default)]
    description: String,
    step_artist: String,
    #[serde(default)]
    meter: Option<u32>,
}

#[derive(Debug, Clone)]
struct ChartStepArtist {
    step_type: String,
    difficulty: String,
    description: String,
    step_artist: String,
}

#[derive(Debug, Clone)]
struct TestCase {
    name: String,
    path: PathBuf,
    extension: String,
}

#[derive(Debug, Clone)]
struct Failure {
    name: String,
    message: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ExpectedMetadata {
    title: String,
    subtitle: String,
    artist: String,
    title_translated: String,
    subtitle_translated: String,
    artist_translated: String,
}

#[derive(Debug, Clone)]
struct ParsedMetadata {
    title: String,
    subtitle: String,
    artist: String,
    title_translated: String,
    subtitle_translated: String,
    artist_translated: String,
}

fn expected_metadata(
    entries: &[GoldenMetadata],
    path: &Path,
) -> Result<ExpectedMetadata, String> {
    let mut expected: Option<ExpectedMetadata> = None;

    for entry in entries {
        let title = entry.title.as_deref().ok_or_else(|| {
            format!("\n\nMISSING BASELINE FIELD\nFile: {}\nField: title\n", path.display())
        })?;
        let subtitle = entry.subtitle.as_deref().ok_or_else(|| {
            format!("\n\nMISSING BASELINE FIELD\nFile: {}\nField: subtitle\n", path.display())
        })?;
        let artist = entry.artist.as_deref().ok_or_else(|| {
            format!("\n\nMISSING BASELINE FIELD\nFile: {}\nField: artist\n", path.display())
        })?;
        let title_translated = entry.title_translated.as_deref().ok_or_else(|| {
            format!(
                "\n\nMISSING BASELINE FIELD\nFile: {}\nField: title_translated\n",
                path.display()
            )
        })?;
        let subtitle_translated = entry.subtitle_translated.as_deref().ok_or_else(|| {
            format!(
                "\n\nMISSING BASELINE FIELD\nFile: {}\nField: subtitle_translated\n",
                path.display()
            )
        })?;
        let artist_translated = entry.artist_translated.as_deref().ok_or_else(|| {
            format!(
                "\n\nMISSING BASELINE FIELD\nFile: {}\nField: artist_translated\n",
                path.display()
            )
        })?;

        let current = ExpectedMetadata {
            title: title.to_string(),
            subtitle: subtitle.to_string(),
            artist: artist.to_string(),
            title_translated: title_translated.to_string(),
            subtitle_translated: subtitle_translated.to_string(),
            artist_translated: artist_translated.to_string(),
        };
        if let Some(ref expected_value) = expected {
            if expected_value != &current {
                return Err(format!(
                    "\n\nINCONSISTENT BASELINE\nFile: {}\nExpected: {:?}\nFound: {:?}\n",
                    path.display(),
                    expected_value,
                    current
                ));
            }
        } else {
            expected = Some(current);
        }
    }

    expected.ok_or_else(|| {
        format!("\n\nMISSING BASELINE METADATA\nFile: {}\n", path.display())
    })
}

#[inline(always)]
fn normalize_step_type(raw: &str) -> String {
    raw.trim().replace('_', "-").to_ascii_lowercase()
}

fn has_hash_prefix(value: &str) -> bool {
    value.trim_start().starts_with('#')
}

fn parse_metadata(simfile_data: &[u8], extension: &str) -> Result<ParsedMetadata, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;

    const STRIP_TAGS: bool = false;

    let mut title_str = parsed_data
        .title
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(|tag| clean_tag(&unescape_tag(tag)))
        .unwrap_or_else(|| "<invalid-title>".to_string());

    if STRIP_TAGS {
        title_str = strip_title_tags(&title_str);
    }

    let mut subtitle_str = parsed_data
        .subtitle
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(unescape_tag)
        .unwrap_or_default();
    let mut artist_str = parsed_data
        .artist
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(unescape_tag)
        .unwrap_or_default();

    let mut title_translated = title_str.clone();
    let mut subtitle_translated = subtitle_str.clone();
    let mut artist_translated = artist_str.clone();

    replace_markers_in_place(&mut title_str);
    replace_markers_in_place(&mut subtitle_str);
    replace_markers_in_place(&mut artist_str);
    replace_markers_in_place(&mut title_translated);
    replace_markers_in_place(&mut subtitle_translated);
    replace_markers_in_place(&mut artist_translated);

    Ok(ParsedMetadata {
        title: title_str,
        subtitle: subtitle_str,
        artist: artist_str,
        title_translated,
        subtitle_translated,
        artist_translated,
    })
}

fn parse_step_artists(simfile_data: &[u8], extension: &str) -> Result<Vec<ChartStepArtist>, String> {
    let parsed_data = extract_sections(simfile_data, extension).map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    for entry in parsed_data.notes_list {
        let (fields, _) = split_notes_fields(&entry.notes);
        if fields.len() < 5 {
            continue;
        }

        let step_type_raw = std::str::from_utf8(fields[0]).unwrap_or("");
        let step_type_unescaped = unescape_trim(step_type_raw);
        let step_type = normalize_step_type(&step_type_unescaped);
        if step_type.is_empty() || step_type == "lights-cabinet" {
            continue;
        }

        let description_raw = std::str::from_utf8(fields[1]).unwrap_or("");
        let description = unescape_trim(description_raw);
        let difficulty_raw = std::str::from_utf8(fields[2]).unwrap_or("");
        let difficulty_unescaped = unescape_trim(difficulty_raw);
        let difficulty = rssp::normalize_difficulty_label(&difficulty_unescaped).to_ascii_lowercase();
        let step_artist = if extension.eq_ignore_ascii_case("ssc") {
            unescape_trim(std::str::from_utf8(fields[4]).unwrap_or(""))
        } else {
            description.clone()
        };

        results.push(ChartStepArtist {
            step_type,
            difficulty,
            description,
            step_artist,
        });
    }

    Ok(results)
}

fn check_file(path: &Path, extension: &str, baseline_dir: &Path) -> Result<(), String> {
    let compressed_bytes = fs::read(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let raw_bytes = zstd::decode_all(&compressed_bytes[..])
        .map_err(|e| format!("Failed to decompress simfile: {}", e))?;

    let file_hash = format!("{:x}", md5::compute(&raw_bytes));
    let subfolder = &file_hash[0..2];

    let golden_path = baseline_dir
        .join(subfolder)
        .join(format!("{}.json.zst", file_hash));

    if !golden_path.exists() {
        return Err(format!(
            "\n\nMISSING BASELINE\nFile: {}\nHash: {}\nExpected baseline: {}\n",
            path.display(),
            file_hash,
            golden_path.display()
        ));
    }

    let compressed_golden = fs::read(&golden_path)
        .map_err(|e| format!("Failed to read baseline file: {}", e))?;

    let json_bytes = zstd::decode_all(&compressed_golden[..])
        .map_err(|e| format!("Failed to decompress baseline json: {}", e))?;

    let golden_entries: Vec<GoldenMetadata> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let golden_step_entries: Vec<GoldenChartStepArtist> = serde_json::from_slice(&json_bytes)
        .map_err(|e| format!("Failed to parse baseline JSON: {}", e))?;

    let (expected_title, expected_subtitle, expected_artist) =
        expected_metadata(&golden_entries, path)?;

    let (actual_title, actual_subtitle, actual_artist) = parse_metadata(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let title_ok = actual_title == expected_title;
    let subtitle_ok = actual_subtitle == expected_subtitle
        || (expected_subtitle.is_empty() && has_hash_prefix(&actual_subtitle));
    let artist_ok = actual_artist == expected_artist
        || (expected_artist == "Unknown artist" && has_hash_prefix(&actual_artist));

    let title_status = if title_ok { "....ok" } else { "....MISMATCH" };
    let subtitle_status = if subtitle_ok { "....ok" } else { "....MISMATCH" };
    let artist_status = if artist_ok { "....ok" } else { "....MISMATCH" };

    println!("File: {}", path.display());
    println!(
        "  title: baseline: {} -> rssp: {} {}",
        expected_title, actual_title, title_status
    );
    println!(
        "  subtitle: baseline: {} -> rssp: {} {}",
        expected_subtitle, actual_subtitle, subtitle_status
    );
    println!(
        "  artist: baseline: {} -> rssp: {} {}",
        expected_artist, actual_artist, artist_status
    );

    let rssp_step_entries = parse_step_artists(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let mut golden_map: HashMap<(String, String, String), Vec<GoldenChartStepArtist>> =
        HashMap::new();
    for golden in golden_step_entries {
        let step_type = normalize_step_type(&golden.step_type);
        if step_type.is_empty() || step_type == "lights-cabinet" {
            continue;
        }
        let difficulty = rssp::normalize_difficulty_label(&golden.difficulty)
            .to_ascii_lowercase();
        let description = golden.description.trim().to_string();
        let key = (step_type, difficulty, description);
        golden_map.entry(key).or_default().push(golden);
    }

    let mut rssp_map: HashMap<(String, String, String), Vec<ChartStepArtist>> = HashMap::new();
    for chart in rssp_step_entries {
        if chart.step_type.is_empty() || chart.step_type == "lights-cabinet" {
            continue;
        }
        let key = (
            chart.step_type.clone(),
            chart.difficulty.clone(),
            chart.description.clone(),
        );
        rssp_map.entry(key).or_default().push(chart);
    }

    let mut golden_entries: Vec<_> = golden_map.into_iter().collect();
    golden_entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut step_artist_ok = true;
    let mut step_artist_errors: Vec<String> = Vec::new();

    for ((step_type, difficulty, description), expected_entries) in golden_entries {
        let Some(actual_entries) =
            rssp_map.remove(&(step_type.clone(), difficulty.clone(), description.clone()))
        else {
            step_artist_ok = false;
            let desc_label = if description.is_empty() {
                "(empty)".to_string()
            } else {
                description.clone()
            };
            println!(
                "  step_artist {} {} [{}]: baseline present, RSSP missing chart",
                step_type, difficulty, desc_label
            );
            step_artist_errors.push(format!(
                "Step artist chart missing: {} {} {:?}",
                step_type, difficulty, description
            ));
            continue;
        };

        let count = expected_entries.len().max(actual_entries.len());
        for idx in 0..count {
            let expected = expected_entries.get(idx);
            let actual = actual_entries.get(idx);
            let meter_label = expected
                .and_then(|entry| entry.meter)
                .map(|meter| meter.to_string())
                .unwrap_or_else(|| (idx + 1).to_string());
            let desc_label = if description.is_empty() {
                meter_label.clone()
            } else {
                format!("{} {}", meter_label, description)
            };

            let expected_val = expected
                .map(|e| e.step_artist.as_str())
                .unwrap_or("-");
            let actual_val = actual.map(|a| a.step_artist.as_str()).unwrap_or("-");
            let status = if expected_val == actual_val {
                "....ok"
            } else {
                "....MISMATCH"
            };

            println!(
                "  step_artist {} {} [{}]: baseline: {} -> rssp: {} {}",
                step_type, difficulty, desc_label, expected_val, actual_val, status
            );

            if status != "....ok" {
                step_artist_ok = false;
                step_artist_errors.push(format!(
                    "Step artist mismatch {} {} [{}]: RSSP step_artist: {:?}, Golden step_artist: {:?}",
                    step_type,
                    difficulty,
                    desc_label,
                    actual.map(|a| a.step_artist.as_str()),
                    expected.map(|e| e.step_artist.as_str())
                ));
            }
        }
    }

    let metadata_ok = title_ok && subtitle_ok && artist_ok;
    if metadata_ok && step_artist_ok {
        return Ok(());
    }

    let mut error_details = String::new();
    if !metadata_ok {
        error_details.push_str(&format!(
            "RSSP title:    {:?}\nGolden title:  {:?}\nRSSP subtitle: {:?}\nGolden subtitle: {:?}\nRSSP artist:   {:?}\nGolden artist: {:?}\n",
            actual_title,
            expected_title,
            actual_subtitle,
            expected_subtitle,
            actual_artist,
            expected_artist
        ));
    }
    if !step_artist_ok {
        if !error_details.is_empty() {
            error_details.push('\n');
        }
        error_details.push_str("Step artist mismatches:\n");
        for line in step_artist_errors {
            error_details.push_str(&line);
            error_details.push('\n');
        }
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\n{}\n",
        path.display(),
        error_details
    ))
}

fn main() {
    let args = Arguments::from_args();

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let packs_dir = manifest_dir.join("tests/data/packs");
    let baseline_dir = manifest_dir.join("tests/data/baseline");

    if !packs_dir.exists() {
        println!("No tests/packs directory found.");
        return;
    }

    let mut tests = Vec::new();

    for entry in WalkDir::new(&packs_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "zst" {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let inner_path = Path::new(stem);
        let inner_extension = inner_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if inner_extension != "sm" && inner_extension != "ssc" {
            continue;
        }

        let test_name = path
            .strip_prefix(&packs_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        tests.push(TestCase {
            name: test_name,
            path: path.to_path_buf(),
            extension: inner_extension,
        });
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));

    let mut tests: Vec<_> = tests
        .into_iter()
        .filter(|t| match &args.filter {
            None => true,
            Some(filter) => {
                if args.exact {
                    &t.name == filter
                } else {
                    t.name.contains(filter)
                }
            }
        })
        .filter(|t| args.skip.iter().all(|skip| !t.name.contains(skip)))
        .collect();

    if args.ignored {
        tests.clear();
    }

    if args.list {
        for t in &tests {
            println!("{}", t.name);
        }
        return;
    }

    println!("running {} tests", tests.len());

    let mut num_passed = 0u64;
    let mut num_failed = 0u64;
    let mut failures: Vec<Failure> = Vec::new();

    for test in tests {
        let TestCase {
            name,
            path,
            extension,
        } = test;

        let res = check_file(&path, &extension, &baseline_dir);
        match res {
            Ok(()) => {
                println!("test {} ... ok", name);
                num_passed += 1;
            }
            Err(msg) => {
                println!("test {} ... FAILED", name);
                failures.push(Failure {
                    name,
                    message: msg.trim().to_string(),
                });
                num_failed += 1;
            }
        }

        let _ = io::stdout().flush();
    }

    println!();
    if !failures.is_empty() {
        println!("failures:");
        for failure in &failures {
            println!("    {}", failure.name);
        }

        for failure in &failures {
            println!();
            println!("---- {} ----", failure.name);
            if !failure.message.is_empty() {
                println!("{}", failure.message);
            }
            println!();
            println!(
                "rerun: cargo test --test metadata_parity -- --exact {:?}",
                failure.name
            );
        }
        println!();
    }

    if num_failed == 0 {
        println!("test result: ok. {} passed; 0 failed", num_passed);
        return;
    }

    println!(
        "test result: FAILED. {} passed; {} failed",
        num_passed, num_failed
    );
    std::process::exit(101);
}

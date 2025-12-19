use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use libtest_mimic::Arguments;
use serde::Deserialize;
use walkdir::WalkDir;

use rssp::parse::{clean_tag, extract_sections, strip_title_tags, unescape_tag};

#[derive(Debug, Deserialize)]
struct GoldenMetadata {
    title: Option<String>,
    subtitle: Option<String>,
    artist: Option<String>,
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

fn expected_metadata(
    entries: &[GoldenMetadata],
    path: &Path,
) -> Result<(String, String, String), String> {
    let mut expected: Option<(String, String, String)> = None;

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

        let current = (title.to_string(), subtitle.to_string(), artist.to_string());
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

fn parse_metadata(simfile_data: &[u8], extension: &str) -> Result<(String, String, String), String> {
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

    let subtitle_str = parsed_data
        .subtitle
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(unescape_tag)
        .unwrap_or_default();
    let artist_str = parsed_data
        .artist
        .and_then(|b| std::str::from_utf8(b).ok())
        .map(unescape_tag)
        .unwrap_or_default();

    Ok((title_str, subtitle_str, artist_str))
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

    let (expected_title, expected_subtitle, expected_artist) =
        expected_metadata(&golden_entries, path)?;

    let (actual_title, actual_subtitle, actual_artist) = parse_metadata(&raw_bytes, extension)
        .map_err(|e| format!("RSSP Parsing Error: {}", e))?;

    let title_status = if actual_title == expected_title {
        "....ok"
    } else {
        "....MISMATCH"
    };
    let subtitle_status = if actual_subtitle == expected_subtitle {
        "....ok"
    } else {
        "....MISMATCH"
    };
    let artist_status = if actual_artist == expected_artist {
        "....ok"
    } else {
        "....MISMATCH"
    };

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

    if title_status == "....ok" && subtitle_status == "....ok" && artist_status == "....ok" {
        return Ok(());
    }

    Err(format!(
        "\n\nMISMATCH DETECTED\nFile: {}\nRSSP title:    {:?}\nGolden title:  {:?}\nRSSP subtitle: {:?}\nGolden subtitle: {:?}\nRSSP artist:   {:?}\nGolden artist: {:?}\n",
        path.display(),
        actual_title,
        expected_title,
        actual_subtitle,
        expected_subtitle,
        actual_artist,
        expected_artist
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

use clap::Parser;
use colored::*;
use colored::Colorize; // Import the Colorize trait explicitly
use serde::Deserialize;
use std::fs;
use std::io::{self};
use std::path::Path;
use std::process::Command;

/// RSSP Test Program
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the rssp binary
    #[arg(short, long, value_name = "BINARY_PATH", default_value = "/home/perfecttaste/rssp/target/release/rssp")]
    binary_path: String,

    /// Directory containing .sm and .json files
    #[arg(short, long, value_name = "SIMFILES_DIR", default_value = "/home/perfecttaste/rssp/simfiles/")]
    simfiles_dir: String,

    /// Specify a single .sm file to test
    #[arg(short = 'f', long = "test-file", value_name = "FILE_PATH")]
    test_file: Option<String>,
}

// Structs to represent the JSON output from the rssp binary
#[derive(Debug, Deserialize)]
struct RsspOutput {
    title: String,
    title_translit: Option<String>,
    subtitle: Option<String>,
    subtitle_translit: Option<String>,
    artist: String,
    artist_translit: Option<String>,
    bpms: Option<String>,
    step_type: Option<String>,
    difficulty: Option<String>,
    rating: Option<String>,
    hash_short: Option<String>,
    arrow_stats: Option<ArrowStats>,
    stream_counts: Option<StreamCounts>,
    breakdown: Option<Breakdown>,
    bpm_info: Option<BpmInfo>,
    pattern_stats: Option<PatternStats>,
    elapsed: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArrowStats {
    left: Option<u32>,
    down: Option<u32>,
    up: Option<u32>,
    right: Option<u32>,
    total_arrows: Option<u32>,
    total_steps: Option<u32>,
    jumps: Option<u32>,
    hands: Option<u32>,
    holds: Option<u32>,
    rolls: Option<u32>,
    mines: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct StreamCounts {
    run16_streams: Option<u32>,
    run20_streams: Option<u32>,
    run24_streams: Option<u32>,
    run32_streams: Option<u32>,
    total_streams: Option<u32>,
    total_breaks: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Breakdown {
    detailed: Option<String>,
    partial: Option<String>,
    simple: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BpmInfo {
    min_bpm: Option<f64>,
    max_bpm: Option<f64>,
    chart_length_s: Option<u32>,
    max_nps: Option<f64>,
    median_nps: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct PatternStats {
    left_foot_candles: Option<u32>,
    right_foot_candles: Option<u32>,
    total_candles: Option<u32>,
    candles_percent: Option<f64>,
    mono_percent: Option<f64>,
    lr_boxes: Option<u32>,
    ud_boxes: Option<u32>,
    corner_ld_boxes: Option<u32>,
    corner_lu_boxes: Option<u32>,
    corner_rd_boxes: Option<u32>,
    corner_ru_boxes: Option<u32>,
    anchor_left: Option<u32>,
    anchor_down: Option<u32>,
    anchor_up: Option<u32>,
    anchor_right: Option<u32>,
    right_dorito: Option<u32>,
    left_dorito: Option<u32>,
    inv_right_dorito: Option<u32>,
    inv_left_dorito: Option<u32>,
}

// Struct to represent the expected JSON file
#[derive(Debug, Deserialize)]
struct ExpectedJson {
    song_id: u32,
    title: String,
    subtitle: Option<String>, // Made optional
    artist: String,
    pack_id: u32,
    rating: f64,
    matrix_rating: f64,
    length: u32,
    bpm: u32,
    tier_id: u32,
    notes: u32,
    jumps: u32,
    holds: u32,
    mines: u32,
    hands: u32,
    rolls: u32,
    total_stream: u32,
    total_break: u32,
    breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
    left_foot_candles: u32,
    right_foot_candles: u32,
    total_candles: u32,
    candles_percent: f64,
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
    max_bpm: u32,
    min_bpm: u32,
    max_nps: f64,
    median_nps: f64,
    md5: String,
    sha1: String,
    has_bg: bool,
    has_bn: bool,
    created_at: String,
}

// Struct to store information about a failed test
struct FailedTest {
    file: String,
    test_name: String,
    expected: String,
    actual: String,
}

fn main() -> io::Result<()> {
    // Parse command-line arguments
    let args = Args::parse();

    // Paths configuration
    let binary_path = &args.binary_path;
    let simfiles_dir = &args.simfiles_dir;

    // Counters
    let mut pass = 0;
    let mut total = 0;

    // Vector to store failed tests
    let mut failed_tests: Vec<FailedTest> = Vec::new();

    // Check if rssp binary exists and is executable
    if !Path::new(binary_path).exists() {
        eprintln!(
            "{}",
            "Error: rssp binary not found at the specified path.".red()
        );
        std::process::exit(1);
    }

    // Determine which files to process
    let sm_files = if let Some(test_file_path) = &args.test_file {
        let path = Path::new(test_file_path);
        if path.exists() && path.extension().and_then(|s| s.to_str()) == Some("sm") {
            vec![path.to_path_buf()]
        } else {
            eprintln!(
                "{} - The specified test file does not exist or is not a .sm file: {}",
                "Error".red(),
                test_file_path
            );
            std::process::exit(1);
        }
    } else {
        // Read all .sm files in the simfiles directory
        fs::read_dir(simfiles_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "sm" {
                    Some(path)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };

    if sm_files.is_empty() {
        eprintln!(
            "{}",
            format!("No .sm files found in {}", simfiles_dir).red()
        );
        std::process::exit(1);
    }

    for sm_path in sm_files {
        let filename = sm_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let expected_hash = filename.trim_end_matches(".sm").to_string();

        // Run the rssp binary with --json flag
        let output = Command::new(binary_path)
            .arg(&sm_path)
            .arg("--json")
            .arg("--strip-tags")
            .output()
            .expect("Failed to execute rssp binary");

        if !output.status.success() {
            eprintln!(
                "{} - Failed to execute rssp for file: {}",
                "FAILED".red(),
                filename
            );
            // Assuming 17 checks per file as in the Bash script
            total += 17;
            // Record all 17 tests as failed for this file
            for i in 1..=17 {
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: format!("Test {}", i),
                    expected: "N/A".to_string(),
                    actual: "rssp execution failed".to_string(),
                });
            }
            continue;
        }

        // Parse the JSON output
        let stdout = String::from_utf8_lossy(&output.stdout);
        let rssp_json: RsspOutput = match serde_json::from_str(&stdout) {
            Ok(json) => json,
            Err(e) => {
                eprintln!(
                    "{} - Invalid JSON output for file: {}",
                    "FAILED".red(),
                    filename
                );
                eprintln!("Error: {}", e);
                eprintln!("Raw Output:\n{}", stdout);
                // Assuming 17 checks per file
                total += 17;
                // Record all 17 tests as failed due to JSON parsing error
                for i in 1..=17 {
                    failed_tests.push(FailedTest {
                        file: filename.clone(),
                        test_name: format!("Test {}", i),
                        expected: "N/A".to_string(),
                        actual: format!("Invalid JSON: {}", e),
                    });
                }
                continue;
            }
        };

        // Read the expected JSON file
        let expected_json_path = format!("{}/{}.json", simfiles_dir, expected_hash);
        let expected_json: ExpectedJson = match fs::read_to_string(&expected_json_path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(json) => json,
                Err(e) => {
                    eprintln!(
                        "{} - Failed to parse expected JSON file: {}",
                        "FAILED".red(),
                        expected_json_path
                    );
                    eprintln!("Error: {}", e);
                    // Assuming 17 checks per file
                    total += 17;
                    // Record all 17 tests as failed due to expected JSON parsing error
                    for i in 1..=17 {
                        failed_tests.push(FailedTest {
                            file: filename.clone(),
                            test_name: format!("Test {}", i),
                            expected: "N/A".to_string(),
                            actual: format!("Invalid expected JSON: {}", e),
                        });
                    }
                    continue;
                }
            },
            Err(_) => {
                eprintln!(
                    "{} - Expected JSON file not found: {}",
                    "FAILED".red(),
                    expected_json_path
                );
                // Assuming 17 checks per file
                total += 17;
                // Record all 17 tests as failed due to missing expected JSON
                for i in 1..=17 {
                    failed_tests.push(FailedTest {
                        file: filename.clone(),
                        test_name: format!("Test {}", i),
                        expected: "N/A".to_string(),
                        actual: "Expected JSON file not found".to_string(),
                    });
                }
                continue;
            }
        };

        println!("Testing file: {}", filename);

        // **Hash Verification**
        total += 1;
        if let Some(hash_short) = rssp_json.hash_short {
            if expected_hash == hash_short {
                println!(
                    "{}",
                    format!(
                        "\tHash: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_hash,
                        hash_short,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tHash: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_hash,
                        hash_short,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Hash Verification".to_string(),
                    expected: expected_hash.clone(),
                    actual: hash_short,
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tHash: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_hash,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Hash Verification".to_string(),
                expected: expected_hash.clone(),
                actual: "None".to_string(),
            });
        }

        // **Title Verification**
        total += 1;
        if expected_json.title == rssp_json.title {
            println!(
                "{}",
                format!(
                    "\tTitle: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.title,
                    rssp_json.title,
                    "PASSED".green()
                )
            );
            pass += 1;
        } else {
            println!(
                "{}",
                format!(
                    "\tTitle: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.title,
                    rssp_json.title,
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Title Verification".to_string(),
                expected: expected_json.title.clone(),
                actual: rssp_json.title.clone(),
            });
        }

        // **Artist Name Verification**
        total += 1;
        if expected_json.artist == rssp_json.artist {
            println!(
                "{}",
                format!(
                    "\tArtist: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.artist,
                    rssp_json.artist,
                    "PASSED".green()
                )
            );
            pass += 1;
        } else {
            println!(
                "{}",
                format!(
                    "\tArtist: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.artist,
                    rssp_json.artist,
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Artist Name Verification".to_string(),
                expected: expected_json.artist.clone(),
                actual: rssp_json.artist.clone(),
            });
        }

        // **Diff Number (Rating) Verification**
        total += 1;
        if let Some(rating_str) = rssp_json.rating {
            if let Ok(rating) = rating_str.parse::<f64>() {
                if expected_json.rating == rating {
                    println!(
                        "{}",
                        format!(
                            "\tRating (diff_number): Expected: \"{}\" | Actual: \"{}\" - {}",
                            expected_json.rating,
                            rating,
                            "PASSED".green()
                        )
                    );
                    pass += 1;
                } else {
                    println!(
                        "{}",
                        format!(
                            "\tRating (diff_number): Expected: \"{}\" | Actual: \"{}\" - {}",
                            expected_json.rating,
                            rating,
                            "NOT PASSED".red()
                        )
                    );
                    // Record the failed test
                    failed_tests.push(FailedTest {
                        file: filename.clone(),
                        test_name: "Diff Number (Rating) Verification".to_string(),
                        expected: expected_json.rating.to_string(),
                        actual: rating.to_string(),
                    });
                }
            } else {
                println!(
                    "{}",
                    format!(
                        "\tRating (diff_number): Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.rating,
                        rating_str,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Diff Number (Rating) Verification".to_string(),
                    expected: expected_json.rating.to_string(),
                    actual: rating_str.clone(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tRating (diff_number): Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.rating,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Diff Number (Rating) Verification".to_string(),
                expected: expected_json.rating.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Subtitle Verification**
        total += 1;
        let expected_subtitle = match &expected_json.subtitle {
            Some(s) if s == "null" => "".to_string(),
            Some(s) => s.clone(),
            None => "".to_string(),
        };
        let actual_subtitle = rssp_json.subtitle.unwrap_or_default();
        if expected_subtitle == actual_subtitle {
            println!(
                "{}",
                format!(
                    "\tSubtitle: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_subtitle,
                    actual_subtitle,
                    "PASSED".green()
                )
            );
            pass += 1;
        } else {
            println!(
                "{}",
                format!(
                    "\tSubtitle: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_subtitle,
                    actual_subtitle,
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Subtitle Verification".to_string(),
                expected: expected_subtitle.clone(),
                actual: actual_subtitle.clone(),
            });
        }

        // **Notes (Steps) Verification**
        total += 1;
        if let Some(total_steps) = rssp_json.arrow_stats.as_ref().and_then(|a| a.total_steps) {
            if expected_json.notes == total_steps {
                println!(
                    "{}",
                    format!(
                        "\tNotes (Steps): Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.notes,
                        total_steps,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tNotes (Steps): Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.notes,
                        total_steps,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Notes (Steps) Verification".to_string(),
                    expected: expected_json.notes.to_string(),
                    actual: total_steps.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tNotes (Steps): Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.notes,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Notes (Steps) Verification".to_string(),
                expected: expected_json.notes.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Jumps Verification**
        total += 1;
        if let Some(jumps) = rssp_json.arrow_stats.as_ref().and_then(|a| a.jumps) {
            if expected_json.jumps == jumps {
                println!(
                    "{}",
                    format!(
                        "\tJumps: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.jumps,
                        jumps,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tJumps: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.jumps,
                        jumps,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Jumps Verification".to_string(),
                    expected: expected_json.jumps.to_string(),
                    actual: jumps.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tJumps: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.jumps,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Jumps Verification".to_string(),
                expected: expected_json.jumps.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Holds Verification**
        total += 1;
        if let Some(holds) = rssp_json.arrow_stats.as_ref().and_then(|a| a.holds) {
            if expected_json.holds == holds {
                println!(
                    "{}",
                    format!(
                        "\tHolds: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.holds,
                        holds,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tHolds: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.holds,
                        holds,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Holds Verification".to_string(),
                    expected: expected_json.holds.to_string(),
                    actual: holds.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tHolds: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.holds,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Holds Verification".to_string(),
                expected: expected_json.holds.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Mines Verification**
        total += 1;
        if let Some(mines) = rssp_json.arrow_stats.as_ref().and_then(|a| a.mines) {
            if expected_json.mines == mines {
                println!(
                    "{}",
                    format!(
                        "\tMines: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.mines,
                        mines,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tMines: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.mines,
                        mines,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Mines Verification".to_string(),
                    expected: expected_json.mines.to_string(),
                    actual: mines.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tMines: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.mines,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Mines Verification".to_string(),
                expected: expected_json.mines.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Hands Verification**
        total += 1;
        if let Some(hands) = rssp_json.arrow_stats.as_ref().and_then(|a| a.hands) {
            if expected_json.hands == hands {
                println!(
                    "{}",
                    format!(
                        "\tHands: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.hands,
                        hands,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tHands: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.hands,
                        hands,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Hands Verification".to_string(),
                    expected: expected_json.hands.to_string(),
                    actual: hands.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tHands: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.hands,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Hands Verification".to_string(),
                expected: expected_json.hands.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Rolls Verification**
        total += 1;
        if let Some(rolls) = rssp_json.arrow_stats.as_ref().and_then(|a| a.rolls) {
            if expected_json.rolls == rolls {
                println!(
                    "{}",
                    format!(
                        "\tRolls: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.rolls,
                        rolls,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tRolls: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.rolls,
                        rolls,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Rolls Verification".to_string(),
                    expected: expected_json.rolls.to_string(),
                    actual: rolls.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tRolls: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.rolls,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Rolls Verification".to_string(),
                expected: expected_json.rolls.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Detailed Breakdown Verification**
        total += 1;
        if let Some(detailed_breakdown) = rssp_json.breakdown.as_ref().and_then(|b| b.detailed.as_ref()) {
            if expected_json.breakdown == *detailed_breakdown {
                println!(
                    "{}",
                    format!(
                        "\tDetailed Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.breakdown,
                        detailed_breakdown,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tDetailed Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.breakdown,
                        detailed_breakdown,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Detailed Breakdown Verification".to_string(),
                    expected: expected_json.breakdown.clone(),
                    actual: detailed_breakdown.clone(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tDetailed Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.breakdown,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Detailed Breakdown Verification".to_string(),
                expected: expected_json.breakdown.clone(),
                actual: "None".to_string(),
            });
        }

        // **Partially Simplified Breakdown Verification**
        total += 1;
        let normalized_expected_partial = normalize_string(&expected_json.partial_breakdown);
        let normalized_actual_partial = rssp_json
            .breakdown
            .as_ref()
            .and_then(|b| b.partial.as_ref())
            .map(|s| normalize_string(s))
            .unwrap_or_default();
        if normalized_expected_partial == normalized_actual_partial {
            println!(
                "{}",
                format!(
                    "\tPartially Simplified Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                    normalized_expected_partial, // Use normalized expected
                    normalized_actual_partial,
                    "PASSED".green()
                )
            );
            pass += 1;
        } else {
            println!(
                "{}",
                format!(
                    "\tPartially Simplified Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                    normalized_expected_partial, // Use normalized expected
                    normalized_actual_partial,
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Partially Simplified Breakdown Verification".to_string(),
                expected: normalized_expected_partial,
                actual: normalized_actual_partial,
            });
        }

        // **Simplified Breakdown Verification**
        total += 1;
        let normalized_expected_simple = normalize_string(&expected_json.simple_breakdown);
        let normalized_actual_simple = rssp_json
            .breakdown
            .as_ref()
            .and_then(|b| b.simple.as_ref())
            .map(|s| normalize_string(s))
            .unwrap_or_default();
        if normalized_expected_simple == normalized_actual_simple {
            println!(
                "{}",
                format!(
                    "\tSimplified Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                    normalized_expected_simple, // Use normalized expected
                    normalized_actual_simple,
                    "PASSED".green()
                )
            );
            pass += 1;
        } else {
            println!(
                "{}",
                format!(
                    "\tSimplified Breakdown: Expected: \"{}\" | Actual: \"{}\" - {}",
                    normalized_expected_simple, // Use normalized expected
                    normalized_actual_simple,
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Simplified Breakdown Verification".to_string(),
                expected: normalized_expected_simple,
                actual: normalized_actual_simple,
            });
        }

        // **Length Verification**
        total += 1;
        if let Some(chart_length_s) = rssp_json.bpm_info.as_ref().and_then(|b| b.chart_length_s) {
            if expected_json.length == chart_length_s {
                println!(
                    "{}",
                    format!(
                        "\tLength: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.length,
                        chart_length_s,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tLength: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.length,
                        chart_length_s,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Length Verification".to_string(),
                    expected: expected_json.length.to_string(),
                    actual: chart_length_s.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tLength: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.length,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Length Verification".to_string(),
                expected: expected_json.length.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Streams Total Verification**
        total += 1;
        if let Some(total_stream) = rssp_json.stream_counts.as_ref().and_then(|s| s.total_streams) {
            if expected_json.total_stream == total_stream {
                println!(
                    "{}",
                    format!(
                        "\tStreams Total: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.total_stream,
                        total_stream,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tStreams Total: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.total_stream,
                        total_stream,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Streams Total Verification".to_string(),
                    expected: expected_json.total_stream.to_string(),
                    actual: total_stream.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tStreams Total: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.total_stream,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Streams Total Verification".to_string(),
                expected: expected_json.total_stream.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Total Break Verification**
        total += 1;
        if let Some(total_breaks) = rssp_json.stream_counts.as_ref().and_then(|s| s.total_breaks) {
            if expected_json.total_break == total_breaks {
                println!(
                    "{}",
                    format!(
                        "\tTotal Break: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.total_break,
                        total_breaks,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tTotal Break: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.total_break,
                        total_breaks,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Total Break Verification".to_string(),
                    expected: expected_json.total_break.to_string(),
                    actual: total_breaks.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tTotal Break: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.total_break,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Total Break Verification".to_string(),
                expected: expected_json.total_break.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Max BPM Verification**
        total += 1;
        if let Some(max_bpm) = rssp_json.bpm_info.as_ref().and_then(|b| b.max_bpm) {
            if expected_json.max_bpm == max_bpm as u32 {
                println!(
                    "{}",
                    format!(
                        "\tMax BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.max_bpm,
                        max_bpm,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tMax BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.max_bpm,
                        max_bpm,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Max BPM Verification".to_string(),
                    expected: expected_json.max_bpm.to_string(),
                    actual: max_bpm.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tMax BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.max_bpm,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Max BPM Verification".to_string(),
                expected: expected_json.max_bpm.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Min BPM Verification**
        total += 1;
        if let Some(min_bpm) = rssp_json.bpm_info.as_ref().and_then(|b| b.min_bpm) {
            if expected_json.min_bpm == min_bpm as u32 {
                println!(
                    "{}",
                    format!(
                        "\tMin BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.min_bpm,
                        min_bpm,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tMin BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                        expected_json.min_bpm,
                        min_bpm,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Min BPM Verification".to_string(),
                    expected: expected_json.min_bpm.to_string(),
                    actual: min_bpm.to_string(),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tMin BPM: Expected: \"{}\" | Actual: \"{}\" - {}",
                    expected_json.min_bpm,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Min BPM Verification".to_string(),
                expected: expected_json.min_bpm.to_string(),
                actual: "None".to_string(),
            });
        }

        // **Max NPS Verification**
        total += 1;
        if let Some(max_nps) = rssp_json.bpm_info.as_ref().and_then(|b| b.max_nps) {
            if float_compare(expected_json.max_nps, max_nps) {
                println!(
                    "{}",
                    format!(
                        "\tMax NPS: Expected: \"{:.2}\" | Actual: \"{:.2}\" - {}",
                        expected_json.max_nps,
                        max_nps,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tMax NPS: Expected: \"{:.2}\" | Actual: \"{:.2}\" - {}",
                        expected_json.max_nps,
                        max_nps,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Max NPS Verification".to_string(),
                    expected: format!("{:.2}", expected_json.max_nps),
                    actual: format!("{:.2}", max_nps),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tMax NPS: Expected: \"{:.2}\" | Actual: \"{}\" - {}",
                    expected_json.max_nps,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Max NPS Verification".to_string(),
                expected: format!("{:.2}", expected_json.max_nps),
                actual: "None".to_string(),
            });
        }

        // **Median NPS Verification**
        total += 1;
        if let Some(median_nps) = rssp_json.bpm_info.as_ref().and_then(|b| b.median_nps) {
            if float_compare(expected_json.median_nps, median_nps) {
                println!(
                    "{}",
                    format!(
                        "\tMedian NPS: Expected: \"{:.2}\" | Actual: \"{:.2}\" - {}",
                        expected_json.median_nps,
                        median_nps,
                        "PASSED".green()
                    )
                );
                pass += 1;
            } else {
                println!(
                    "{}",
                    format!(
                        "\tMedian NPS: Expected: \"{:.2}\" | Actual: \"{:.2}\" - {}",
                        expected_json.median_nps,
                        median_nps,
                        "NOT PASSED".red()
                    )
                );
                // Record the failed test
                failed_tests.push(FailedTest {
                    file: filename.clone(),
                    test_name: "Median NPS Verification".to_string(),
                    expected: format!("{:.2}", expected_json.median_nps),
                    actual: format!("{:.2}", median_nps),
                });
            }
        } else {
            println!(
                "{}",
                format!(
                    "\tMedian NPS: Expected: \"{:.2}\" | Actual: \"{}\" - {}",
                    expected_json.median_nps,
                    "None",
                    "NOT PASSED".red()
                )
            );
            // Record the failed test
            failed_tests.push(FailedTest {
                file: filename.clone(),
                test_name: "Median NPS Verification".to_string(),
                expected: format!("{:.2}", expected_json.median_nps),
                actual: "None".to_string(),
            });
        }

        println!(); // Blank line between files
    }

    // Print the summary
    println!("\n{}", "Summary:".bold());
    println!(
        "{} out of {} passed the test.",
        pass.to_string().green(),
        total.to_string().green()
    );

    // If there are any failed tests, list them
    if !failed_tests.is_empty() {
        println!("\n{}", "Failed Tests:".bold().red());
        for failure in failed_tests {
            println!(
                "- File: {}\n  Test: {}\n  Expected: {}\n  Actual: {}\n",
                failure.file, failure.test_name, failure.expected, failure.actual
            );
        }
    } else {
        println!("{}", "\nAll tests passed successfully!".green());
    }

    Ok(())
}

// Function to normalize strings by replacing backslashes with 'x'
fn normalize_string(input: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;

    for c in input.chars() {
        if c == '\\' {
            if !in_escape {
                result.push('x');
                in_escape = true;
            }
            // If already in escape, skip adding another 'x'
        } else {
            result.push(c);
            in_escape = false;
        }
    }

    result
}

// Function to compare floating-point numbers with precision
fn float_compare(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

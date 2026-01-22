use std::env::args;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use rssp::AnalysisOptions;
use rssp::analyze;
use rssp::graph::{ColorScheme, generate_density_graph_png};
use rssp::matrix::get_difficulty;
use rssp::report::{OutputMode, SimfileSummary, write_course_reports, write_reports};

/// Analyzes a single simfile and returns the summary
fn analyze_simfile(
    path: &Path,
    options: &AnalysisOptions,
) -> io::Result<rssp::report::SimfileSummary> {
    let sim = rssp::simfile::open(path)?;
    analyze(&sim.data, sim.extension, options.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn print_minimized_notes(simfile: &SimfileSummary) {
    for chart in &simfile.charts {
        let artists = if chart.step_artist_str.is_empty() {
            String::new()
        } else {
            format!(" by {}", chart.step_artist_str)
        };

        eprintln!(
            "\n--- Debug: {} - {} {}{} ---",
            simfile.title_str, chart.difficulty_str, chart.rating_str, artists
        );
        eprintln!("{}", String::from_utf8_lossy(&chart.minimized_note_data));
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = args().collect();

    // --- Matrix Calculation Mode ---
    if args.iter().any(|a| a == "--matrix") {
        let mut bpm_opt: Option<f64> = None;
        let mut measures_opt: Option<f64> = None;

        if let Some(pos) = args.iter().position(|arg| arg == "-b" || arg == "--bpm") {
            bpm_opt = args.get(pos + 1).and_then(|s| s.parse().ok());
        }
        if let Some(pos) = args
            .iter()
            .position(|arg| arg == "-m" || arg == "--measures")
        {
            measures_opt = args.get(pos + 1).and_then(|s| s.parse().ok());
        }

        if let (Some(bpm), Some(measures)) = (bpm_opt, measures_opt) {
            let rating = get_difficulty(bpm, measures);
            println!(
                "Matrix rating of {measures} measures @ {bpm} BPM is {rating:.4}"
            );
            return Ok(());
        }
        eprintln!(
            "Usage: {} --matrix --bpm <BPM> --measures <MEASURES>",
            args[0]
        );
        eprintln!("   (Short flags -b and -m are also accepted)");
        std::process::exit(1);
    }

    // --- Simfile Analysis Mode ---
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_or_folder_path> [OPTIONS]", args[0]);
        eprintln!("   or: {} --matrix -b <BPM> -m <MEASURES>", args[0]);
        eprintln!("\nOptions for simfile/folder analysis:");
        eprintln!("  --full          Full output mode");
        eprintln!("  --png           Generate density graph PNG (default colors)");
        eprintln!("  --png-alt       Generate density graph PNG (alternative colors)");
        eprintln!("  --json          JSON output format");
        eprintln!("  --csv           CSV output format");
        eprintln!("  --strip-tags    Strip title tags from output");
        eprintln!("  --debug         Print minimized chart note data to stderr");
        eprintln!("  --skip-slow     Skip step parity and pattern variant analysis");
        eprintln!("  --skip-tech     Skip tech count analysis");
        eprintln!("  --mono-threshold <value>  Set mono threshold (default: 6)");
        eprintln!("  --custom-pattern <pattern>  Count a custom LRUDN pattern (e.g. DULDUDLR)");
        eprintln!("\nFolder analysis:");
        eprintln!("  When a folder path is provided, rssp will recursively scan for");
        eprintln!("  simfiles, preferring .ssc files over .sm files when both exist.");
        std::process::exit(1);
    }

    let simfile_path = &args[1];

    // --- Parse flags ---
    let debug_output = args.iter().any(|a| a == "--debug");
    let generate_png = args.iter().any(|a| a == "--png");
    let generate_png_alt = args.iter().any(|a| a == "--png-alt");
    let skip_slow = args.iter().any(|a| a == "--skip-slow");
    let skip_tech = skip_slow || args.iter().any(|a| a == "--skip-tech");
    let skip_patterns = skip_slow;

    let mut mono_threshold = 6;
    if let Some(pos) = args.iter().position(|arg| arg == "--mono-threshold") {
        if let Some(val_str) = args.get(pos + 1) {
            if let Ok(value) = val_str.parse::<usize>() {
                mono_threshold = value;
            } else {
                eprintln!("Error: Invalid value for --mono-threshold. Must be a positive integer.");
                std::process::exit(1);
            }
        } else {
            eprintln!("Error: Missing value for --mono-threshold.");
            std::process::exit(1);
        }
    }

    let mut custom_patterns: Vec<String> = Vec::new();
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--custom-pattern" {
            if let Some(pattern_str) = args.get(i + 1) {
                if pattern_str.is_empty() {
                    eprintln!("Error: Empty value for --custom-pattern.");
                    std::process::exit(1);
                }
                if !pattern_str
                    .chars()
                    .all(|c| matches!(c, 'L' | 'l' | 'D' | 'd' | 'U' | 'u' | 'R' | 'r' | 'N' | 'n'))
                {
                    eprintln!(
                        "Error: Invalid character in custom pattern '{pattern_str}'. Allowed characters: L, D, U, R, N."
                    );
                    std::process::exit(1);
                }
                custom_patterns.push(pattern_str.to_uppercase());
                i += 2;
                continue;
            }
            eprintln!("Error: Missing value for --custom-pattern.");
            std::process::exit(1);
        }
        i += 1;
    }

    let options = AnalysisOptions {
        strip_tags: args.iter().any(|a| a == "--strip-tags"),
        mono_threshold,
        custom_patterns,
        compute_tech_counts: !skip_tech,
        compute_pattern_counts: !skip_patterns,
        translate_markers: false,
    };

    // --- Course flags (only used for .crs input) ---
    let songs_dir: Option<PathBuf> = args
        .iter()
        .position(|a| a == "--songs-dir")
        .and_then(|pos| args.get(pos + 1))
        .map(PathBuf::from);
    let course_difficulty = args
        .iter()
        .position(|a| a == "--course-difficulty" || a == "--course-diff")
        .and_then(|pos| args.get(pos + 1))
        .cloned()
        .unwrap_or_else(|| "Medium".to_string());
    let steps_type = args
        .iter()
        .position(|a| a == "--steps-type" || a == "--stepstype")
        .and_then(|pos| args.get(pos + 1))
        .cloned()
        .unwrap_or_else(|| "dance-single".to_string());

    // --- Determine output mode ---
    let mode = if args.iter().any(|a| a == "--csv") {
        OutputMode::CSV
    } else if args.iter().any(|a| a == "--json") {
        OutputMode::JSON
    } else if args.iter().any(|a| a == "--full") {
        OutputMode::Full
    } else {
        OutputMode::Pretty
    };

    // --- Determine if path is file or folder ---
    let path = Path::new(simfile_path);

    if !path.exists() {
        eprintln!("Error: Path does not exist: {}", path.display());
        std::process::exit(1);
    }

    // --- Course Analysis Mode (.crs) ---
    if path.is_file()
        && path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("crs"))
    {
        let course = rssp::course::analyze_crs_path(
            path,
            songs_dir.as_deref(),
            &steps_type,
            &course_difficulty,
            options,
        )
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write_course_reports(&course, mode, &mut handle)?;
        return Ok(());
    }

    let simfiles = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        let files = rssp::pack::find_simfiles(path, rssp::pack::ScanOpt::default());
        if files.is_empty() {
            eprintln!("No simfiles found in directory: {}", path.display());
            std::process::exit(1);
        }
        eprintln!("Found {} simfile(s) to analyze", files.len());
        files
    } else {
        eprintln!("Error: Path is neither a file nor a directory");
        std::process::exit(1);
    };

    // --- Process simfiles ---
    for (idx, simfile_path) in simfiles.iter().enumerate() {
        if simfiles.len() > 1 {
            eprintln!(
                "Analyzing [{}/{}]: {}",
                idx + 1,
                simfiles.len(),
                simfile_path.display()
            );
        }

        let simfile = match analyze_simfile(simfile_path, &options) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error analyzing {}: {}", simfile_path.display(), e);
                continue;
            }
        };

        // --- Print reports ---
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        write_reports(&simfile, mode, &mut handle)?;
        if debug_output {
            // Debug output goes to stderr to avoid polluting structured stdout formats.
            print_minimized_notes(&simfile);
        }

        // --- Generate PNG graphs if requested ---
        if generate_png || generate_png_alt {
            let color_scheme = if generate_png_alt {
                ColorScheme::Alternative
            } else {
                ColorScheme::Default
            };

            for chart_summary in &simfile.charts {
                generate_density_graph_png(
                    &chart_summary.measure_nps_vec,
                    chart_summary.max_nps,
                    &chart_summary.short_hash,
                    &color_scheme,
                )?;
            }
        }
    }

    Ok(())
}

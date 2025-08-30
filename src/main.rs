use std::env::args;
use std::fs::File;
use std::io::{self, Read};

use rssp::analyze; // <-- Use the new library function
use rssp::graph::{generate_density_graph_png, ColorScheme};
use rssp::matrix::get_difficulty;
use rssp::report::{print_reports, OutputMode};
use rssp::AnalysisOptions; // <-- Use the new options struct

fn main() -> io::Result<()> {
    let args: Vec<String> = args().collect();

    // --- Matrix Calculation Mode (no changes needed) ---
    if args.iter().any(|a| a == "--matrix") {
        let mut bpm_opt: Option<f64> = None;
        let mut measures_opt: Option<f64> = None;

        if let Some(pos) = args.iter().position(|arg| arg == "-b" || arg == "--bpm") {
            bpm_opt = args.get(pos + 1).and_then(|s| s.parse().ok());
        }
        if let Some(pos) = args.iter().position(|arg| arg == "-m" || arg == "--measures") {
            measures_opt = args.get(pos + 1).and_then(|s| s.parse().ok());
        }

        if let (Some(bpm), Some(measures)) = (bpm_opt, measures_opt) {
            let rating = get_difficulty(bpm, measures);
            println!("Matrix rating of {} measures @ {} BPM is {:.4}", measures, bpm, rating);
            return Ok(());
        } else {
            eprintln!("Usage: {} --matrix --bpm <BPM> --measures <MEASURES>", args[0]);
            eprintln!("   (Short flags -b and -m are also accepted)");
            std::process::exit(1);
        }
    }

    // --- Simfile Analysis Mode ---
    if args.len() < 2 {
        eprintln!("Usage: {} <simfile_path> [OPTIONS]", args[0]);
        eprintln!("   or: {} --matrix -b <BPM> -m <MEASURES>", args[0]);
        eprintln!("\nRun with a simfile path to analyze a file. Options for analysis:");
        eprintln!("  --full, --png, --png-alt, --json, --csv, --strip-tags, --mono-threshold <value>");
        std::process::exit(1);
    }

    let simfile_path = &args[1];

    // --- Read file and extension ---
    let mut file = File::open(simfile_path)?;
    let mut simfile_data = Vec::new();
    file.read_to_end(&mut simfile_data)?;
    let extension = simfile_path.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");

    // --- Build options from CLI args ---
    let generate_png = args.iter().any(|a| a == "--png");
    let generate_png_alt = args.iter().any(|a| a == "--png-alt");

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

    let options = AnalysisOptions {
        strip_tags: args.iter().any(|a| a == "--strip-tags"),
        mono_threshold,
    };

    // --- Call the library's main analysis function ---
    let simfile = match analyze(&simfile_data, extension, options) {
        Ok(summary) => summary,
        Err(e) => {
            eprintln!("Error analyzing simfile: {}", e);
            std::process::exit(1);
        }
    };

    // --- Handle reporting and image generation ---
    let mode = if args.iter().any(|a| a == "--csv") {
        OutputMode::CSV
    } else if args.iter().any(|a| a == "--json") {
        OutputMode::JSON
    } else if args.iter().any(|a| a == "--full") {
        OutputMode::Full
    } else {
        OutputMode::Pretty
    };

    print_reports(&simfile, mode);

    if generate_png || generate_png_alt {
        let color_scheme = if generate_png_alt { ColorScheme::Alternative } else { ColorScheme::Default };
        for chart_summary in &simfile.charts {
            generate_density_graph_png(
                &chart_summary.measure_nps_vec,
                chart_summary.max_nps,
                &chart_summary.short_hash,
                &color_scheme,
            )?;
        }
    }

    Ok(())
}
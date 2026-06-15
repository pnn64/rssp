//! Dump per-row StepParity annotations for a simfile.
//!
//! Companion to the `note_annotations_parity` test: prints the same per-row
//! data rssp feeds the parity harness (beat, foot-bearing columns, per-column
//! foot identity, foot count, full per-row tech counts), so you
//! can eyeball rssp's output or diff it against the ITGmania
//! `Steps:GetNoteAnnotations()` dump.
//!
//! Usage:
//!   cargo run --example dump_annotations -- <path-to.sm|.ssc> [--json]
//!
//! Without `--json` it prints a compact per-chart summary; with `--json` it
//! emits `[{steps_type, difficulty, note_annotations:[...]}]` matching the
//! golden baseline schema.

use std::path::Path;

use rssp::{AnalysisOptions, RowAnnotation, TechCounts, analyze};

fn columns_of(a: &RowAnnotation) -> Vec<u8> {
    let mut cols = Vec::new();
    let mut mask = a.column_mask;
    while mask != 0 {
        cols.push(mask.trailing_zeros() as u8);
        mask &= mask - 1;
    }
    cols
}

/// Foot id assigned to each foot-bearing column, parallel to `columns_of`.
fn feet_of(a: &RowAnnotation) -> Vec<u8> {
    columns_of(a)
        .into_iter()
        .map(|c| a.foot(c as usize) as u8)
        .collect()
}

fn tech_counts_json(t: &TechCounts) -> serde_json::Value {
    serde_json::json!({
        "crossovers": t.crossovers,
        "half_crossovers": t.half_crossovers,
        "full_crossovers": t.full_crossovers,
        "footswitches": t.footswitches,
        "up_footswitches": t.up_footswitches,
        "down_footswitches": t.down_footswitches,
        "sideswitches": t.sideswitches,
        "jacks": t.jacks,
        "brackets": t.brackets,
        "doublesteps": t.doublesteps,
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path_arg) = args.next() else {
        eprintln!("usage: dump_annotations <path-to.sm|.ssc> [--json]");
        std::process::exit(2);
    };
    let as_json = args.any(|a| a == "--json");

    let path = Path::new(&path_arg);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
        .unwrap_or_default();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("failed to read {}: {e}", path.display());
            std::process::exit(1);
        }
    };

    let options = AnalysisOptions {
        mono_threshold: 6,
        compute_note_annotations: true,
        ..AnalysisOptions::default()
    };
    let summary = match analyze(&bytes, &ext, &options) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("analyze error: {e}");
            std::process::exit(1);
        }
    };

    if as_json {
        let charts: Vec<_> = summary
            .charts
            .iter()
            .map(|c| {
                let rows: Vec<_> = c
                    .note_annotations
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "beat": a.beat,
                            "columns": columns_of(a),
                            "feet": feet_of(a),
                            "note_count": a.foot_count(),
                            "tech_counts": tech_counts_json(&a.row_tech),
                        })
                    })
                    .collect();
                serde_json::json!({
                    "steps_type": c.step_type_str,
                    "difficulty": c.difficulty_str,
                    "note_annotations": rows,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&charts).unwrap());
    } else {
        for c in &summary.charts {
            let annotations = c.note_annotations.as_deref().unwrap_or(&[]);
            let crossovers = annotations
                .iter()
                .filter(|a| a.row_tech.crossovers > 0)
                .count();
            println!(
                "{} {} [{}]: {} annotated rows, {} crossovers (tech_counts.crossovers = {})",
                c.step_type_str,
                c.difficulty_str,
                c.rating_str,
                annotations.len(),
                crossovers,
                c.tech_counts.crossovers,
            );
        }
    }
}

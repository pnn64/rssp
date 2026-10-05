use super::{black_box, measure};
use std::fmt::Write as _;

fn timing(count: usize, kind: &str) -> (String, String) {
    let mut pairs = String::new();
    let mut speeds = String::new();
    for index in 0..count {
        if index != 0 {
            pairs.push(',');
            speeds.push(',');
        }
        let beat = match kind {
            "duplicates" => index / 2,
            "reverse" => count - index,
            _ => index,
        } * 4;
        write!(pairs, "{beat}={}", index % 7 + 1).expect("String write");
        write!(speeds, "{beat}={}=0.5=0", index % 7 + 1).expect("String write");
    }
    (pairs, speeds)
}

fn radar(count: usize, kind: &str, ext: &str) -> Vec<u8> {
    let mut values = String::new();
    for index in 0..count {
        if index != 0 {
            values.push(',');
        }
        let pad = if kind == "dirty" { " \u{b}" } else { "" };
        write!(values, "{pad}{}{pad}", index % 100).expect("String write");
    }
    if ext == "sm" {
        format!("#BPMS:0=120;#NOTES:dance-single::Hard:8:{values}:1000\n0100\n0010\n0001;")
            .into_bytes()
    } else {
        format!("#VERSION:0.83;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:8;#RADARVALUES:{values};#NOTES:1000\n0100\n0010\n0001;").into_bytes()
    }
}

fn segments(pairs: &str, speeds: &str) -> rssp::timing::TimingSegments {
    rssp::timing::compute_timing_segments(
        None,
        "0=120",
        None,
        pairs,
        None,
        pairs,
        None,
        pairs,
        None,
        speeds,
        None,
        pairs,
        None,
        pairs,
        rssp::timing::TimingFormat::Ssc,
        true,
    )
}

pub fn cases(iters: usize) {
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    let mut scratch = rssp::AnalysisScratch::default();
    for count in [0, 1, 32, 4096] {
        for kind in ["ordered", "duplicates", "reverse"] {
            let (pairs, speeds) = timing(count, kind);
            measure(
                &format!("timing_cleanup/{count}_{kind}"),
                count,
                iters,
                || {
                    black_box(segments(black_box(&pairs), black_box(&speeds)));
                },
            );
        }
    }
    for count in [14, 28, 4096] {
        for kind in ["plain", "dirty"] {
            for ext in ["sm", "ssc"] {
                let data = radar(count, kind, ext);
                measure(
                    &format!("radar_load/{count}_{kind}_{ext}"),
                    count,
                    iters,
                    || {
                        black_box(
                            rssp::analyze_with_scratch(
                                black_box(&data),
                                ext,
                                &options,
                                &mut scratch,
                            )
                            .expect("valid radar fixture"),
                        );
                    },
                );
            }
        }
    }
}

pub fn verify() {
    for count in [0, 1, 32, 4096] {
        for kind in ["ordered", "duplicates", "reverse"] {
            let (pairs, speeds) = timing(count, kind);
            println!(
                "timing-cleanup {count} {kind} {:?}",
                segments(&pairs, &speeds)
            );
        }
    }
    for count in [0, 13, 14, 27, 28, 4096] {
        for kind in ["plain", "dirty"] {
            for ext in ["sm", "ssc"] {
                let data = radar(count, kind, ext);
                let summary = rssp::analyze(&data, ext, &rssp::AnalysisOptions::default())
                    .expect("valid radar fixture");
                let mut output = Vec::new();
                rssp::report::write_reports(&summary, rssp::report::OutputMode::JSON, &mut output)
                    .expect("Vec write");
                println!(
                    "radar-load {count} {kind} {ext} {}",
                    String::from_utf8(output).expect("JSON is UTF-8")
                );
            }
        }
    }
}

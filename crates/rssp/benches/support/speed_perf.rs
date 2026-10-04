use super::{black_box, measure};
use std::fmt::Write as _;

fn fixture(count: usize, kind: &str) -> Vec<u8> {
    let mut raw = String::from("#VERSION:0.83;#BPMS:0=120;#SPEEDS:");
    for index in 0..count {
        if index != 0 {
            raw.push(',');
        }
        let dirty = match kind {
            "clean" => false,
            "early" => index == 0,
            "late" => index + 1 == count,
            _ => unreachable!("fixture kind"),
        };
        let pad = if dirty { " \u{b}" } else { "" };
        write!(raw, "{pad}{}=1.25=0.5=0{pad}", index * 4).expect("String write");
    }
    raw.push(';');
    for _ in 0..4 {
        raw.push_str("#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:8;#NOTES:1000\n0100\n0010\n0001;");
    }
    raw.into_bytes()
}

pub fn cases(iters: usize) {
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    let mut scratch = rssp::AnalysisScratch::default();
    for count in [1, 128, 4096] {
        for kind in ["clean", "early", "late"] {
            let data = fixture(count, kind);
            measure(&format!("speed_load/{count}_{kind}"), count, iters, || {
                black_box(
                    rssp::analyze_with_scratch(
                        black_box(&data),
                        "ssc",
                        black_box(&options),
                        &mut scratch,
                    )
                    .expect("valid speed fixture"),
                );
            });
        }
    }
}

pub fn verify() {
    for count in [0, 1, 128, 4096] {
        for kind in ["clean", "early", "late"] {
            let data = fixture(count, kind);
            let summary = rssp::analyze(&data, "ssc", &rssp::AnalysisOptions::default())
                .expect("valid speed fixture");
            println!(
                "speed-load {count} {kind} {:?} {:?}",
                summary.normalized_speeds, summary.global_timing_segments.speeds
            );
            for chart in &summary.charts {
                println!(
                    "speed-chart {:?} {:?} {:?}",
                    chart.short_hash,
                    chart.bpm_neutral_hash,
                    rssp::report::build_timing_snapshot(chart, &summary)
                );
            }
        }
    }
}

use super::{black_box, measure};

#[path = "course_fixtures.rs"]
mod fixtures;

pub fn cases(iters: usize) {
    for count in [1, 32] {
        for length in [16, 4096] {
            for escaped in [false, true] {
                let data = fixtures::course(count, length, escaped);
                measure(
                    &format!("course_parse/{count}_{length}_{escaped}"),
                    count,
                    iters,
                    || {
                        black_box(rssp::course::parse_crs(black_box(&data)).expect("valid course"));
                    },
                );
            }
        }
    }
    let mut summary = rssp::analyze(
        b"#VERSION:0.83;#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:8;#NOTES:1000;",
        "ssc", &rssp::AnalysisOptions::default()).expect("valid SSC");
    summary.charts[0].chart_has_own_timing = false;
    for count in [0, 1, 32, 256] {
        for kind in ["plain", "padded", "invalid"] {
            summary.normalized_tickcounts = fixtures::numeric(count, kind, false);
            summary.normalized_time_signatures = fixtures::numeric(count, kind, true);
            summary.normalized_combos = fixtures::numeric(count, kind, true);
            measure(
                &format!("numeric_snapshot/{count}_{kind}"),
                count,
                iters,
                || {
                    black_box(rssp::report::build_timing_snapshot(
                        black_box(&summary.charts[0]),
                        black_box(&summary),
                    ));
                },
            );
        }
    }
}

pub fn verify() {
    for count in [1, 32] {
        for length in [16, 4096] {
            for escaped in [false, true] {
                println!(
                    "course {count} {length} {escaped} {:?}",
                    rssp::course::parse_crs(&fixtures::course(count, length, escaped))
                );
            }
        }
    }
    for kind in ["plain", "escaped", "cp1252", "cp_escape"] {
        let (data, _) = fixtures::title(16, kind);
        let parsed = rssp::parse::extract_sections(&data, "sm").expect("valid SM");
        println!(
            "course title {kind} {:?} {:?}",
            parsed.title, parsed.subtitle
        );
    }
}

use crate::perf::measure;
use std::hint::black_box;

use crate::perf::fixtures;

#[path = "course_reports.rs"]
mod course_reports;

#[path = "csv_edges.rs"]
mod csv_edges;

#[test]
fn course_report_edges() {
    use std::io::{self, Write};
    struct Limited {
        output: Vec<u8>,
        limit: usize,
        errors: usize,
    }
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let len = bytes.len().min(self.limit - self.output.len());
            if len == 0 {
                self.errors += 1;
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            self.output.extend_from_slice(&bytes[..len]);
            Ok(len)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut course = course_reports::summary(16);
    course.course = "A\"\\\n歌".into();
    let refs = std::sync::Arc::strong_count(&course.chart.timing_segments);
    for mode in [
        super::OutputMode::Full,
        super::OutputMode::Pretty,
        super::OutputMode::JSON,
        super::OutputMode::CSV,
    ] {
        let mut expected = Vec::new();
        super::write_course_reports(&course, mode, &mut expected).expect("Vec write");
        if matches!(mode, super::OutputMode::JSON) {
            let json: serde_json::Value = serde_json::from_slice(&expected).expect("valid JSON");
            assert_eq!(json["course"], course.course);
            assert_eq!(json["course_difficulty"], course.course_difficulty);
            assert_eq!(json["step_type"], course.step_type);
            assert!(json["chart"]["pattern_counts"].is_object());
            assert!(json["chart"]["tech_counts"].is_object());
            assert_eq!(
                json["chart"]["timing"]["bpms"]
                    .as_array()
                    .expect("BPM array")
                    .len(),
                2
            );
        }
        for limit in [0, 1, 32, expected.len() / 2, expected.len() - 1] {
            let mut writer = Limited {
                output: Vec::new(),
                limit,
                errors: 0,
            };
            let error = super::write_course_reports(&course, mode, &mut writer)
                .expect_err("limited writer");
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(writer.errors, 1);
            assert_eq!(writer.output, expected[..limit]);
        }
        assert_eq!(
            std::sync::Arc::strong_count(&course.chart.timing_segments),
            refs
        );
    }
}

#[test]
#[ignore = "explicit course report benchmark"]
fn course_hotpath() {
    let mut output = Vec::with_capacity(65536);
    for length in [0, 16, 4096] {
        let course = course_reports::summary(length);
        measure(&format!("course_dummy/{length}"), 1, || {
            black_box(super::dummy_simfile_for_course(black_box(&course)));
        });
        for mode in [
            super::OutputMode::Full,
            super::OutputMode::Pretty,
            super::OutputMode::JSON,
            super::OutputMode::CSV,
        ] {
            measure(&format!("course_report/{length}_{mode:?}"), 1, || {
                output.clear();
                super::write_course_reports(black_box(&course), mode, black_box(&mut output))
                    .expect("Vec write");
                black_box(&output);
            });
        }
    }
}

#[test]
#[ignore = "explicit course output comparison"]
fn course_trace() {
    for length in [0, 16, 4096] {
        let course = course_reports::summary(length);
        for mode in [
            super::OutputMode::Full,
            super::OutputMode::Pretty,
            super::OutputMode::JSON,
            super::OutputMode::CSV,
        ] {
            let mut output = Vec::new();
            super::write_course_reports(&course, mode, &mut output).expect("Vec write");
            println!("course-report {length} {mode:?} {output:?}");
        }
    }
}

#[test]
fn numeric_trim_edges() {
    let padded = Some(
        "\u{2003}, \t8 \u{2003}= \u{2003}3 = 4 \u{2003}=ignored,0=5=4,4=3=4,4=7=8,12=7=8,broken=bad=bad, ",
    );
    assert_eq!(super::parse_tickcounts(padded), [(0.0, 5), (4.0, 7)]);
    assert_eq!(super::parse_combos(padded), [(0.0, 5, 4), (4.0, 7, 8)]);
    assert_eq!(
        super::parse_time_signatures(padded),
        super::parse_combos(padded)
    );
    for empty in [None, Some(""), Some("\u{2003},=,==,bad=bad")] {
        assert_eq!(super::parse_tickcounts(empty), [(0.0, 4)]);
        assert_eq!(super::parse_combos(empty), [(0.0, 1, 1)]);
        assert_eq!(super::parse_time_signatures(empty), [(0.0, 4, 4)]);
    }
    assert_eq!(
        super::parse_tickcounts(Some("-0=3=extra"))[0].0.to_bits(),
        0.0f64.to_bits()
    );
    assert_eq!(
        super::parse_time_signatures(Some("8=3=4")),
        [(0.0, 4, 4), (8.0, 3, 4)]
    );
}

#[test]
#[ignore = "explicit numeric parser benchmark"]
fn numeric_hotpath() {
    for count in [0, 1, 32, 256] {
        for kind in ["plain", "padded", "invalid"] {
            let pair = fixtures::numeric(count, kind, false);
            let triple = fixtures::numeric(count, kind, true);
            measure(&format!("numeric/ticks/{count}_{kind}"), count, || {
                black_box(super::parse_tickcounts(black_box(Some(&pair))));
            });
            measure(&format!("numeric/signatures/{count}_{kind}"), count, || {
                black_box(super::parse_time_signatures(black_box(Some(&triple))));
            });
            measure(&format!("numeric/combos/{count}_{kind}"), count, || {
                black_box(super::parse_combos(black_box(Some(&triple))));
            });
        }
    }
}

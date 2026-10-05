use super::*;
use crate::perf::measure;
use std::hint::black_box;

fn fixture(len: usize, kind: &str) -> Vec<usize> {
    let pattern = match kind {
        "mixed" => &[0, 16, 16, 0, 20, 20, 0, 0, 32, 32, 32, 0, 0, 0, 24][..],
        "empty" => &[0, 15],
        "single" | "leading" | "late" | "trailing" => &[16],
        "alternating" => &[16, 0],
        _ => unreachable!("fixture kind"),
    };
    (0..len)
        .map(|i| {
            if (kind == "leading" && i + 8 < len)
                || (kind == "trailing" && i >= 8)
                || (kind == "late" && i + 8 == len)
            {
                0
            } else {
                pattern[i % pattern.len()]
            }
        })
        .collect()
}

#[test]
fn stream_gap_edges() {
    let levels = [
        StreamBreakdownLevel::Detailed,
        StreamBreakdownLevel::Partial,
        StreamBreakdownLevel::Simple,
    ];
    for (input, expected) in [
        (&[][..], ["No Streams!", "No Streams!", "No Streams!"]),
        (&[0, 0, 16, 16, 0, 0][..], ["2", "2", "2"]),
        (&[16, 0, 20][..], ["1-1", "1-1", "3*"]),
        (&[16, 0, 0, 20][..], ["1 (2) 1", "1-1", "1-1"]),
    ] {
        let got = stream_breakdowns(input);
        assert_eq!([got.0.as_str(), got.1.as_str(), got.2.as_str()], expected);
        for (level, expected) in levels.into_iter().zip(expected) {
            assert_eq!(stream_breakdown(input, level), expected);
        }
    }
}

#[test]
#[ignore = "explicit standalone stream benchmark"]
fn streams_hotpath() {
    for len in [0, 1, 32, 4096] {
        for kind in [
            "empty",
            "single",
            "mixed",
            "leading",
            "alternating",
            "late",
            "trailing",
        ] {
            let data = fixture(len, kind);
            measure(&format!("stream/{len}_{kind}_visit"), len, || {
                let mut count = 0;
                visit_stream_sequences(black_box(&data), |_| {
                    count += 1;
                    Ok::<(), std::convert::Infallible>(())
                })
                .expect("infallible visitor");
                black_box(count);
            });
            for (level, name) in [
                (StreamBreakdownLevel::Detailed, "detailed"),
                (StreamBreakdownLevel::Partial, "partial"),
                (StreamBreakdownLevel::Simple, "simple"),
                (StreamBreakdownLevel::Total, "total"),
            ] {
                measure(&format!("stream/{len}_{kind}_{name}"), len, || {
                    black_box(stream_breakdown(black_box(&data), level));
                });
            }
            measure(&format!("stream/{len}_{kind}_three"), len, || {
                black_box(stream_breakdowns(black_box(&data)));
            });
        }
    }
    for n in [1, 123, 1_234_567, usize::MAX] {
        measure(&format!("stream_run/{n}"), 1, || {
            black_box(format_run_symbol(RunDensity::Run32, black_box(n), true));
        });
    }
}

#[test]
#[ignore = "original/optimized exact stream transcript"]
fn streams_trace() {
    for len in [0, 1, 32, 4096] {
        for kind in [
            "empty",
            "single",
            "mixed",
            "leading",
            "alternating",
            "late",
            "trailing",
        ] {
            let data = fixture(len, kind);
            println!(
                "stream-output {len} {kind} {:?} {:?}",
                stream_breakdowns(&data),
                compute_stream_outputs(&data)
            );
            for level in [
                StreamBreakdownLevel::Detailed,
                StreamBreakdownLevel::Partial,
                StreamBreakdownLevel::Simple,
                StreamBreakdownLevel::Total,
            ] {
                println!(
                    "stream-output {len} {kind} {level:?} {:?}",
                    stream_breakdown(&data, level)
                );
            }
        }
    }
    // Exhaust every short stream/break pattern, including leading/trailing gaps.
    for mask in 0..4096usize {
        let data: Vec<_> = (0..12)
            .map(|i| if mask & (1 << i) == 0 { 0 } else { 16 })
            .collect();
        println!("stream-output mask{mask} {:?}", stream_breakdowns(&data));
    }
}

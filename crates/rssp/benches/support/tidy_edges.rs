use super::*;
use crate::perf::{measure, measure_prepared};
use std::hint::black_box;

#[test]
fn canonical_rows_are_exact() {
    for row in [
        i32::MIN,
        i32::MIN + 1,
        -49,
        -48,
        -1,
        0,
        1,
        47,
        48,
        i32::MAX - 1,
        i32::MAX,
    ] {
        assert_eq!(beat_to_note_row(note_row_to_beat(row)), row);
        if row < i32::MAX {
            assert!(note_row_to_beat(row) < note_row_to_beat(row + 1));
        }
    }
    for index in 0..10_000_i64 {
        let row = (i64::from(i32::MIN) + index * 429_497) as i32;
        assert_eq!(beat_to_note_row(note_row_to_beat(row)), row);
    }
}

#[test]
fn tidy_quantized_edges() {
    let mut input: Vec<_> = [
        f64::NEG_INFINITY,
        -44_739_242.0,
        -4.01,
        -0.01,
        -0.0,
        0.0,
        0.01,
        0.010_416_666_666_666_666,
        0.010_416_666_666_666_668,
        4.01,
        44_739_242.0,
        f64::INFINITY,
        f64::NAN,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, beat)| Segment {
        beat,
        value: [0.0, -0.0, 1.0, 2.0, f64::NAN][index % 5],
    })
    .collect();
    for reverse in [false, true] {
        if reverse {
            input.reverse();
        }
        let mut expected = input.clone();
        for segment in &mut expected {
            segment.beat = note_row_to_beat(beat_to_note_row(segment.beat));
        }
        expected.sort_by_key(segment_row);
        expected.dedup_by(|later, earlier| {
            if segment_row(later) == segment_row(earlier) {
                *earlier = *later;
                true
            } else {
                false
            }
        });
        tests::assert_segment_bits_eq(&tidy_row_segments(input.clone()), &expected);
        tests::assert_segment_bits_eq(
            &tidy_scroll_segments(input.clone()),
            &tests::tidy_scroll_segments_slow(input.clone()),
        );
        let speeds: Vec<_> = input
            .iter()
            .enumerate()
            .map(|(index, segment)| SpeedSegment {
                beat: segment.beat,
                ratio: segment.value,
                delay: index as f64 % 3.0,
                unit: if index % 2 == 0 {
                    SpeedUnit::Beats
                } else {
                    SpeedUnit::Seconds
                },
            })
            .collect();
        tests::assert_speed_bits_eq(
            &tidy_speed_segments(speeds.clone()),
            &tests::tidy_speed_segments_slow(speeds),
        );
    }
}

fn segments(count: usize, kind: &str) -> Vec<Segment> {
    (0..count)
        .map(|index| Segment {
            beat: match kind {
                "duplicates" => (index / 2) as f64 * 4.0,
                "reverse" => (count - index) as f64 * 4.0,
                _ => index as f64 * 4.0,
            },
            value: (index % 7 + 1) as f64,
        })
        .collect()
}

#[test]
#[ignore = "explicit timing cleanup benchmark"]
fn tidy_hotpath() {
    for count in [0, 1, 32, 4096] {
        for kind in ["ordered", "duplicates", "reverse"] {
            let input = segments(count, kind);
            for (name, tidy) in [
                (
                    "rows",
                    tidy_row_segments as fn(Vec<Segment>) -> Vec<Segment>,
                ),
                ("scrolls", tidy_scroll_segments),
            ] {
                measure_prepared(
                    &format!("tidy/{name}/{count}_{kind}"),
                    count,
                    || input.clone(),
                    |input| {
                        *input = tidy(black_box(std::mem::take(input)));
                        black_box(&*input);
                    },
                );
            }
            let speeds: Vec<_> = input
                .iter()
                .map(|segment| SpeedSegment {
                    beat: segment.beat,
                    ratio: segment.value,
                    delay: 0.5,
                    unit: SpeedUnit::Beats,
                })
                .collect();
            measure_prepared(
                &format!("tidy/speeds/{count}_{kind}"),
                count,
                || speeds.clone(),
                |input| {
                    *input = tidy_speed_segments(black_box(std::mem::take(input)));
                    black_box(&*input);
                },
            );
            if kind != "reverse" {
                measure_prepared(
                    &format!("compact/{count}_{kind}"),
                    count,
                    || input.clone(),
                    |input| {
                        *input = compact_row_segments(black_box(std::mem::take(input)));
                        black_box(&*input);
                    },
                );
            }
        }
    }
    // Unchanged row conversion control.
    measure("row_convert/control", 1, || {
        black_box(beat_to_note_row(black_box(4.125)));
    });
}

#[test]
#[ignore = "explicit timing cleanup output comparison"]
fn tidy_trace() {
    for count in [0, 1, 32, 4096] {
        for kind in ["ordered", "duplicates", "reverse"] {
            let input = segments(count, kind);
            println!(
                "tidy-output {count} {kind} rows {:?} scrolls {:?}",
                tidy_row_segments(input.clone()),
                tidy_scroll_segments(input.clone())
            );
            let speeds: Vec<_> = input
                .iter()
                .map(|segment| SpeedSegment {
                    beat: segment.beat,
                    ratio: segment.value,
                    delay: 0.5,
                    unit: SpeedUnit::Beats,
                })
                .collect();
            println!(
                "tidy-output {count} {kind} speeds {:?}",
                tidy_speed_segments(speeds)
            );
        }
    }
}

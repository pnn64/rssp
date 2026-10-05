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
fn unordered_insert_edges() {
    for (values, beat, value, expected) in [
        (
            [1.0, 2.0, 3.0],
            2.0,
            4.0,
            vec![(0.0, 1.0), (2.0, 4.0), (4.0, 2.0), (8.0, 3.0)],
        ),
        (
            [1.0, 2.0, 3.0],
            12.0,
            4.0,
            vec![(0.0, 1.0), (4.0, 2.0), (8.0, 3.0), (12.0, 4.0)],
        ),
        (
            [1.0, 2.0, 3.0],
            4.0,
            4.0,
            vec![(0.0, 1.0), (4.0, 4.0), (8.0, 3.0)],
        ),
        ([1.0, 2.0, 3.0], 4.0, 1.0, vec![(0.0, 1.0), (8.0, 3.0)]),
        ([1.0, 2.0, 3.0], 4.0, 3.0, vec![(0.0, 1.0), (4.0, 3.0)]),
        ([1.0, 2.0, 1.0], 4.0, 1.0, vec![(0.0, 1.0)]),
        (
            [1.0, 2.0, 3.0],
            -4.0,
            4.0,
            vec![(-4.0, 4.0), (0.0, 1.0), (4.0, 2.0), (8.0, 3.0)],
        ),
    ] {
        let mut out: Vec<_> = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| Segment {
                beat: index as f64 * 4.0,
                value,
            })
            .collect();
        let speed = |segment: &Segment| SpeedSegment {
            beat: segment.beat,
            ratio: segment.value,
            delay: 0.5,
            unit: SpeedUnit::Beats,
        };
        let mut speeds: Vec<_> = out.iter().map(speed).collect();
        let seg = Segment { beat, value };
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(beat, value)| Segment { beat, value })
            .collect();
        add_scroll_segment_slow(&mut out, seg);
        add_speed_segment_slow(&mut speeds, speed(&seg));
        tests::assert_segment_bits_eq(&out, &expected);
        tests::assert_speed_bits_eq(&speeds, &expected.iter().map(speed).collect::<Vec<_>>());
    }
}

#[test]
#[ignore = "explicit unordered timing insertion benchmark"]
fn insert_hotpath() {
    for count in [1, 32, 4096] {
        for kind in ["append", "middle", "replace"] {
            let input = segments(count, "ordered");
            let beat = match kind {
                "append" => count as f64 * 4.0,
                "middle" => (count / 2) as f64 * 4.0 - 2.0,
                _ => (count / 2) as f64 * 4.0,
            };
            let seg = Segment { beat, value: 9.0 };
            measure_prepared(
                &format!("insert/scrolls/{count}_{kind}"),
                count,
                || {
                    let mut out = Vec::with_capacity(count + 1);
                    out.extend_from_slice(&input);
                    out
                },
                |out| {
                    add_scroll_segment_slow(black_box(out), black_box(seg));
                },
            );
            let speeds: Vec<_> = input
                .iter()
                .map(|s| SpeedSegment {
                    beat: s.beat,
                    ratio: s.value,
                    delay: 0.5,
                    unit: SpeedUnit::Beats,
                })
                .collect();
            let seg = SpeedSegment {
                beat,
                ratio: 9.0,
                delay: 0.5,
                unit: SpeedUnit::Beats,
            };
            measure_prepared(
                &format!("insert/speeds/{count}_{kind}"),
                count,
                || {
                    let mut out = Vec::with_capacity(count + 1);
                    out.extend_from_slice(&speeds);
                    out
                },
                |out| {
                    add_speed_segment_slow(black_box(out), black_box(seg));
                },
            );
        }
    }
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

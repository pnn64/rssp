use super::*;
use crate::perf::{measure, measure_prepared};
use std::hint::black_box;

fn fixture(len: usize, kind: &str) -> Vec<f64> {
    (0..len)
        .map(|i| match kind {
            "uniform" => 8.0,
            "sparse" if i % 8 != 0 => 0.0,
            "negative" => -((i % 13) as f64) - 1.0,
            "special" => [f64::NAN, f64::INFINITY, -0.0, 4.0][i % 4],
            _ => ((i * 37) % 23) as f64 / 3.0,
        })
        .collect()
}

#[test]
fn scratch_nps_edges() {
    for (values, expected) in [
        (&[][..], (0.0, 0.0)),
        (&[8.0][..], (8.0, 8.0)),
        (&[8.0, 2.0, 4.0, 16.0][..], (16.0, 6.0)),
        (&[-9.0, -1.0, -5.0][..], (0.0, -5.0)),
    ] {
        for capacity in [0, 2, 64, 128] {
            let mut scratch = Vec::with_capacity(capacity);
            scratch.push(99.0);
            assert_eq!(get_nps_stats_with_scratch(values, &mut scratch), expected);
            assert!(scratch.capacity() >= capacity);
        }
    }
    for len in [64, 65, 128, 129] {
        for kind in ["uniform", "dense", "sparse", "negative", "special"] {
            let data = fixture(len, kind);
            let expected = get_nps_stats_with_scratch(&data, &mut Vec::new());
            let actual = get_nps_stats(&data);
            assert_eq!(actual.0.to_bits(), expected.0.to_bits());
            assert_eq!(actual.1.to_bits(), expected.1.to_bits());
        }
    }
}

#[test]
#[ignore = "explicit NPS summary allocation benchmark"]
fn summary_hotpath() {
    for len in [0, 1, 2, 8, 32, 64, 65, 128, 129, 4096] {
        for kind in ["uniform", "dense", "sparse", "negative", "special"] {
            let data = fixture(len, kind);
            measure(&format!("nps_summary/{len}_{kind}_owned"), len, || {
                black_box(get_nps_stats(black_box(&data)));
            });
            measure_prepared(
                &format!("nps_summary/{len}_{kind}_cold"),
                len,
                Vec::new,
                |scratch| {
                    black_box(get_nps_stats_with_scratch(black_box(&data), scratch));
                },
            );
            let mut scratch = Vec::with_capacity(len);
            measure(&format!("nps_summary/{len}_{kind}_warm"), len, || {
                black_box(get_nps_stats_with_scratch(
                    black_box(&data),
                    black_box(&mut scratch),
                ));
            });
            measure_prepared(
                &format!("nps_summary/{len}_{kind}_in_place"),
                len,
                || data.clone(),
                |values| {
                    black_box(get_nps_stats_in_place(black_box(values)));
                },
            );
        }
    }
}

#[test]
#[ignore = "exact original/optimized NPS transcript"]
fn summary_trace() {
    for len in [0, 1, 2, 8, 32, 64, 65, 128, 129, 4096] {
        for kind in ["uniform", "dense", "sparse", "negative", "special"] {
            let data = fixture(len, kind);
            let owned = get_nps_stats(&data);
            for capacity in [0, 2, 64, 4096] {
                let mut scratch = Vec::with_capacity(capacity);
                let actual = get_nps_stats_with_scratch(&data, &mut scratch);
                let selected: Vec<_> = scratch.iter().map(|value| value.to_bits()).collect();
                println!(
                    "nps-summary {len} {kind} {capacity} {:x?} {:x?} {selected:x?}",
                    [owned.0.to_bits(), owned.1.to_bits()],
                    [actual.0.to_bits(), actual.1.to_bits()]
                );
            }
        }
    }
}

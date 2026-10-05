use super::*;
use crate::perf::measure;
use std::hint::black_box;

#[test]
fn decimal_boundaries() {
    for n in [
        0,
        1,
        9,
        10,
        99,
        100,
        999,
        1_000,
        999_999,
        1_000_000,
        u64::MAX,
    ] {
        let mut out = String::from("prefix:");
        push_u64(&mut out, n);
        assert_eq!(out, format!("prefix:{n}"));
    }
}

#[test]
#[ignore = "explicit integer and decimal formatting benchmark"]
fn number_hotpath() {
    for n in [0, 7, 123, 1_234_567, u64::MAX] {
        let mut out = String::with_capacity(32);
        measure(&format!("uint/{n}"), 1, || {
            out.clear();
            push_u64(black_box(&mut out), black_box(n));
            black_box(&out);
        });
    }
    for (name, value) in [("zero", 0.0), ("bpm", 123.456), ("negative", -9_876_543.21)] {
        let mut out = String::with_capacity(64);
        measure(&format!("decimal/{name}"), 1, || {
            out.clear();
            push_dec3_half_up(black_box(&mut out), black_box(value));
            black_box(&out);
        });
    }
}

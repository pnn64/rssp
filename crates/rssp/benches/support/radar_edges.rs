use super::{RADAR_CATEGORY_COUNT, parse_radar_values_bytes, parse_radar_values_str};
use crate::perf::measure;
use std::hint::black_box;

fn values(count: usize, kind: &str) -> String {
    (0..count)
        .map(|index| match kind {
            "dirty" => format!(" \u{b}{} \u{2003}", index % 100),
            "sparse" => format!(",bad,{}", index % 100),
            _ => (index % 100).to_string(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn radar_prefix_edges() {
    let expected = std::array::from_fn(|index| index as f32);
    for split in [false, true] {
        let needed = RADAR_CATEGORY_COUNT * if split { 2 } else { 1 };
        for kind in ["plain", "dirty", "sparse"] {
            let raw = values(needed, kind);
            assert_eq!(parse_radar_values_str(&raw, split), Some(expected));
            assert_eq!(
                parse_radar_values_str(&format!("{raw},-1,NaN,inf,bad,,\u{b}"), split),
                Some(expected)
            );
            assert!(parse_radar_values_str(&values(needed - 1, kind), split).is_none());
        }
        let mut parts: Vec<_> = (0..needed).map(|index| index.to_string()).collect();
        for first in ["NaN", "inf", "-inf", "-0"] {
            parts[0] = first.into();
            assert!(parse_radar_values_str(&parts.join(","), split).is_some());
        }
        for invalid in ["NaN", "inf", "-inf", "-0.1"] {
            parts[5] = invalid.into();
            assert!(parse_radar_values_str(&parts.join(","), split).is_none());
        }
        parts[5] = "-0".into();
        let result =
            parse_radar_values_str(&parts.join(","), split).expect("negative zero is valid");
        assert_eq!(result[5].to_bits(), (-0.0f32).to_bits());
    }
    for raw in ["", " ,bad,, ", "\u{b}"] {
        assert!(parse_radar_values_str(raw, false).is_none());
    }
    assert!(parse_radar_values_bytes(None, false).is_none());
    assert!(parse_radar_values_bytes(Some(&[0xff]), false).is_none());
}

#[test]
#[ignore = "explicit radar parser benchmark"]
fn radar_hotpath() {
    for count in [0, 14, 28, 128, 4096] {
        for kind in ["plain", "dirty", "sparse"] {
            let raw = values(count, kind);
            for split in [false, true] {
                measure(&format!("radar/{count}_{kind}_{split}"), count, || {
                    black_box(parse_radar_values_str(black_box(&raw), black_box(split)));
                });
            }
        }
    }
}

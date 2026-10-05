use super::*;
use crate::perf::{measure, measure_prepared};
use std::hint::black_box;

#[path = "map_fixtures.rs"]
mod fixtures;

#[test]
fn timing_tag_edges() {
    for raw in [
        None,
        Some(b"".as_slice()),
        Some(b" \t\r\n\x0b\x0c"),
        Some(b"\xff"),
        Some("\u{2003}".as_bytes()),
        Some(b", ,"),
        Some(b"0=120"),
        Some(b" \x0b0=\t120\x0b "),
        Some("0=120,4=\u{85}150".as_bytes()),
    ] {
        let expected = raw
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(clean_timing_map)
            .filter(|value| !value.is_empty());
        assert_eq!(chart_timing_tag_raw(raw), expected);
        assert_eq!(chart_timing_tag_cow(raw).as_deref(), expected.as_deref());
    }
    assert_eq!(
        chart_timing_tag_raw(Some(b" \x0b0=\t120\x0b ")).as_deref(),
        Some("0=120")
    );
    for count in [0, 1, 128, 4096] {
        let data = fixtures::blank_file(count);
        let empty = fixtures::blank_file(0);
        assert_eq!(
            chart_bpm_snapshots(&data, "ssc"),
            chart_bpm_snapshots(&empty, "ssc")
        );
    }
}

#[test]
#[ignore = "explicit timing tag benchmark"]
fn tag_hotpath() {
    for count in [0, 1, 128, 4096] {
        for kind in ["blank", "clean", "first", "last"] {
            let raw = fixtures::tag(count, kind);
            measure(
                &format!("timing_tag/owned/{count}_{kind}"),
                raw.len(),
                || {
                    black_box(chart_timing_tag_raw(Some(black_box(&raw))));
                },
            );
            measure(&format!("timing_tag/cow/{count}_{kind}"), raw.len(), || {
                black_box(chart_timing_tag_cow(Some(black_box(&raw))));
            });
        }
    }
}

#[test]
fn cleanup_prefix_edges() {
    for (raw, expected, borrowed) in [
        ("", "", true),
        ("0=120,4=150", "0=120,4=150", true),
        (",0=120", "0=120", false),
        ("0=120,", "0=120", false),
        ("0=120,,4=150", "0=120,4=150", false),
        ("0=120, 4=150 ", "0=120,4=150", false),
        ("α=β,\u{2003} γ=δ\u{2003}", "α=β,γ=δ", false),
        ("0=120,\u{b}\u{85}\t4=150\r\u{b}", "0=120,4=150", false),
        ("0=120,4=\u{85}150", "0=120,4=\u{85}150", true),
        ("0=120,4=\t150", "0=120,4=\t150", true),
        (", ,\u{b},", "", false),
    ] {
        let cleaned = clean_timing_map_cow(raw);
        assert_eq!(cleaned, expected);
        assert_eq!(cleaned, clean_timing_map(raw));
        assert_eq!(matches!(cleaned, Cow::Borrowed(_)), borrowed);
        if let Cow::Owned(cleaned) = cleaned {
            assert_eq!(cleaned.capacity(), raw.len());
        }
    }
    for count in [1, 128, 4096] {
        for kind in ["clean", "first", "middle", "last"] {
            let raw = fixtures::map(count, kind);
            assert_eq!(clean_timing_map_cow(&raw), clean_timing_map(&raw));
        }
    }
}

#[test]
fn snapshot_precision_edges() {
    for fmt in [TimingFormat::Sm, TimingFormat::Ssc] {
        for raw in [
            "",
            "0=120.125",
            "0=120,4=150,4=180,8=90",
            "-0=NaN,1=inf,2=-inf",
            "0=-0,4=0",
            "0=1e40,4=3.4028235e38",
            "0.010416666666666668=123.456789,44739242=9999.9999",
            "8=90,0=-120,4=120",
            "0=0.000001,4=1000000",
        ] {
            for stops in ["", "0=0.125,2=-0.5,6=0.25"] {
                let global = [Cow::Borrowed(raw), Cow::Borrowed(stops)];
                let chart = [Some(Cow::Borrowed(raw)), Some(Cow::Borrowed(stops))];
                for local in [false, true] {
                    let timing = bpm_snapshot_timing(&chart, &global, fmt, local);
                    let (bpms, _, _, _) = parse_bpm_stops(None, raw, None, stops, fmt, true);
                    let native: Vec<_> = bpms
                        .into_iter()
                        .map(|(b, v)| (b as f32, v as f32))
                        .collect();
                    assert_eq!(
                        timing.bpms_formatted,
                        crate::timing::format_bpm_segments_f32_like_itg(&native)
                    );
                    let range = actual_bpm_range_raw_f32(&native);
                    assert_eq!(timing.bpm_min_raw.to_bits(), range.0.to_bits());
                    assert_eq!(timing.bpm_max_raw.to_bits(), range.1.to_bits());
                }
            }
        }
    }
}

#[test]
fn snapshot_cache_edges() {
    for ext in ["sm", "ssc"] {
        for count in [0, 1, 32] {
            let raw = fixtures::map(count, "clean");
            let data = fixtures::simfile(&raw, "2=-0.5,6=0.125", ext, false);
            let snapshots = chart_bpm_snapshots(&data, ext).expect("valid inherited charts");
            assert_eq!(snapshots.len(), 2);
            assert_eq!(snapshots[0].bpms_formatted, snapshots[1].bpms_formatted);
            assert_eq!(
                snapshots[0].bpm_min.to_bits(),
                snapshots[1].bpm_min.to_bits()
            );
            assert_eq!(
                snapshots[0].bpm_max.to_bits(),
                snapshots[1].bpm_max.to_bits()
            );
            let single = &data[..data
                .windows(7)
                .rposition(|s| s == b"#NOTES:")
                .expect("notes tag")];
            // Parse a complete single-chart file, without the final chart header.
            let end = if ext == "sm" {
                single.len()
            } else {
                single
                    .windows(10)
                    .rposition(|s| s == b"#NOTEDATA:")
                    .expect("chart header")
            };
            let one = chart_bpm_snapshots(&data[..end], ext).expect("valid single chart");
            assert_eq!(one.len(), 1);
            assert_eq!(one[0], snapshots[0]);
        }
    }
}

#[test]
#[ignore = "explicit cached chart snapshot benchmark"]
fn cache_hotpath() {
    for count in [0, 1, 32, 4096] {
        let raw = fixtures::map(count, "clean");
        let norm = normalize_float_digits(&raw);
        for (ext, fmt) in [("sm", TimingFormat::Sm), ("ssc", TimingFormat::Ssc)] {
            let data = fixtures::simfile(&raw, "", ext, false);
            let parsed = extract_sections(&data, ext).expect("valid inherited charts");
            let global = [Cow::Borrowed(raw.as_str()), Cow::Borrowed("")];
            for kind in ["cached_final", "cached_more", "first_final"] {
                measure_prepared(
                    &format!("bpm_cache/{count}_{ext}_{kind}"),
                    count,
                    || {
                        let cache = if kind == "first_final" {
                            None
                        } else {
                            Some(bpm_snapshot_timing(&[None, None], &global, fmt, false))
                        };
                        (cache, None)
                    },
                    |(cache, output)| {
                        *output = chart_bpm_snapshot(
                            black_box(&parsed.notes_list[1]),
                            black_box(&global),
                            black_box(&norm),
                            fmt,
                            true,
                            cache,
                            kind != "cached_more",
                        );
                        black_box(&*output);
                    },
                );
            }
        }
    }
}

#[test]
#[ignore = "explicit raw timing cleanup and snapshot benchmark"]
fn bpm_hotpath() {
    for count in [0, 1, 128, 4096] {
        for kind in ["clean", "first", "middle", "last"] {
            let raw = fixtures::map(count, kind);
            measure(&format!("raw_map/{count}_{kind}"), count, || {
                black_box(clean_timing_map_cow(black_box(&raw)));
            });
        }
    }
    for count in [0, 1, 32, 4096] {
        let raw = fixtures::map(count, "clean");
        for (ext, fmt) in [("sm", TimingFormat::Sm), ("ssc", TimingFormat::Ssc)] {
            for stops in ["", "2=-0.5,6=0.125"] {
                let global = [Cow::Borrowed(raw.as_str()), Cow::Borrowed(stops)];
                let chart = [None, None];
                let kind = if stops.is_empty() { "plain" } else { "stops" };
                measure(&format!("bpm_snapshot/{count}_{ext}_{kind}"), count, || {
                    let timing =
                        bpm_snapshot_timing(black_box(&chart), black_box(&global), fmt, false);
                    black_box((
                        timing.bpms_formatted,
                        timing.bpm_min_raw,
                        timing.bpm_max_raw,
                    ));
                });
            }
        }
    }
}

#[test]
#[ignore = "explicit snapshot output comparison"]
fn bpm_trace() {
    for count in [0, 1, 32, 4096] {
        let raw = fixtures::map(count, "clean");
        for fmt in [TimingFormat::Sm, TimingFormat::Ssc] {
            for stops in ["", "2=-0.5,6=0.125"] {
                let global = [Cow::Borrowed(raw.as_str()), Cow::Borrowed(stops)];
                let timing = bpm_snapshot_timing(&[None, None], &global, fmt, false);
                println!(
                    "snapshot-output {count} {fmt:?} {stops:?} {:?} {} {}",
                    timing.bpms_formatted,
                    timing.bpm_min_raw.to_bits(),
                    timing.bpm_max_raw.to_bits()
                );
            }
        }
    }
}

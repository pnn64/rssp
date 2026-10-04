#![expect(
    clippy::float_cmp,
    reason = "regressions must preserve exact numeric outputs"
)]

use rssp::bpm::{chart_bpm_snapshots, compute_tier_bpm};
use rssp::nps::compute_chart_peak_nps;

#[test]
fn unescape_prefix_bytes() {
    use std::borrow::Cow;
    for (input, expected) in [
        ("", ""),
        ("plain\u{e9}", "plain\u{e9}"),
        ("\\", "\\"),
        ("\\:", ":"),
        ("prefix\\", "prefix\\"),
        ("prefix\\\\", "prefix\\"),
        ("prefix\\\u{1f600}", "prefix\u{1f600}"),
        (
            "\u{65e5}\u{e9}\\:\u{1f600}\\\\\\x\\",
            "\u{65e5}\u{e9}:\u{1f600}\\x\\",
        ),
        ("a\\\0b\\\n", "a\0b\n"),
    ] {
        let actual = rssp::parse::unescape_tag(input);
        assert_eq!(actual, expected);
        assert_eq!(matches!(actual, Cow::Borrowed(_)), !input.contains('\\'));
        assert_eq!(rssp::parse::decode_unescape(input.as_bytes()), expected);
    }
}

#[test]
fn translated_prefix_bytes() {
    for (input, expected) in [
        ("", ""),
        ("plain\u{e9}", "plain\u{e9}"),
        ("&", "&"),
        ("prefix&", "prefix&"),
        ("prefix&unknown;", "prefix&unknown;"),
        ("prefix&bad&ha;", "prefix&bad\u{3042}"),
        ("prefix&&#65;", "prefix&A"),
        ("\u{65e5}\u{1f600}&#x266F;", "\u{65e5}\u{1f600}\u{266f}"),
        ("&unknown;&#0;&ha;&", "&unknown;\0\u{3042}&"),
        ("&#xD800;&#65536;", "\u{fffd}\u{fffd}"),
    ] {
        let mut text = String::with_capacity(128);
        text.push_str(input);
        let pointer = text.as_ptr();
        let capacity = text.capacity();
        rssp::translate::replace_markers_in_place(&mut text);
        assert_eq!(text, expected);
        assert_eq!(text.as_ptr(), pointer);
        assert_eq!(text.capacity(), capacity);
        assert_eq!(rssp::translate::replace_markers(input), expected);
    }
}

#[test]
fn stripped_title_bytes() {
    for (input, stripped, plain) in [
        (" [Pack] [01] 2.5- Name ", "Name", "[Pack] [01] 2.5- Name"),
        (
            "\u{2003}[Pack]\u{2003}2- \u{65e5}\u{2003}",
            "\u{65e5}",
            "[Pack]\u{2003}2- \u{65e5}",
        ),
        (" [Unclosed ", "[Unclosed", "[Unclosed"),
        (" [Done] ", "", "[Done]"),
        ("   ", "", ""),
        ("&#65; Song", "&#65; Song", "&#65; Song"),
        ("[P]\\:&#65; Song", ":&#65; Song", "[P]:&#65; Song"),
    ] {
        for strip_tags in [false, true] {
            for translate_markers in [false, true] {
                let data = format!(
                    "#TITLE:{};#BPMS:0=120;#NOTES:dance-single::Hard:8::1000;",
                    input.replace(';', "\\;")
                );
                let options = rssp::AnalysisOptions {
                    strip_tags,
                    translate_markers,
                    compute_tech_counts: false,
                    compute_pattern_counts: false,
                    ..Default::default()
                };
                let summary =
                    rssp::analyze(data.as_bytes(), "sm", &options).expect("valid fixture");
                let expected = if strip_tags { stripped } else { plain };
                if translate_markers {
                    assert_eq!(
                        summary.title_str,
                        rssp::translate::replace_markers(expected)
                    );
                } else {
                    assert_eq!(summary.title_str, expected);
                }
            }
        }
    }
}

#[test]
fn composed_stream_prefixes() {
    use rssp::streams::{
        StreamCounts, Token, compute_stream_counts, compute_stream_outputs,
        compute_stream_outputs_with_scratch, generate_breakdowns, stream_breakdowns,
    };
    let mut tokens = vec![Token::Break(123)];
    for prefix in [0, 1, 2, 4000] {
        for tail in [0, 1, 2, 4096] {
            let mut densities = vec![0; prefix];
            densities.extend_from_slice(&[16, 16, 0, 20, 0, 0, 32, 32, 24]);
            densities.resize(densities.len() + tail, 15);
            let expected = (
                compute_stream_counts(&densities),
                generate_breakdowns(&densities),
                stream_breakdowns(&densities),
            );
            assert_eq!(compute_stream_outputs(&densities), expected);
            assert_eq!(
                compute_stream_outputs_with_scratch(&densities, &mut tokens),
                expected
            );
        }
    }
    let empty = compute_stream_outputs_with_scratch(&[0, 15, 0], &mut tokens);
    assert_eq!(empty.0, StreamCounts::default());
    assert_eq!(empty.1, (String::new(), String::new(), String::new()));
    assert_eq!(
        empty.2,
        (
            "No Streams!".into(),
            "No Streams!".into(),
            "No Streams!".into()
        )
    );
    assert!(tokens.is_empty());
}

#[test]
fn peak_empty_notes() {
    for version in ["0.6", "0.83"] {
        for (kind, lanes) in [
            ("dance-single", 4),
            ("pump-single", 5),
            ("dance-double", 8),
            ("pump-double", 10),
        ] {
            for tags in [
                "",
                "#BPMS:NaN;#STOPS:0=2;#DELAYS:0=1;#WARPS:1=2;#OFFSET:-100000;",
                "#SPEEDS:;#FAKES:0=4;",
            ] {
                let mut notes = format!("{}\n,\n", "0".repeat(lanes));
                for object in ['M', 'F', 'L', 'K', '3'] {
                    notes.push(object);
                    notes.push_str(&"0".repeat(lanes - 1));
                    notes.push('\n');
                }
                let data = format!(
                    "#VERSION:{version};#BPMS:0=120;#STOPS:0=1;#NOTEDATA:;#STEPSTYPE:{kind};#DIFFICULTY:Easy;#METER:3;{tags}#NOTES:\n{notes};"
                );
                let charts = compute_chart_peak_nps(data.as_bytes(), "ssc").expect("valid fixture");
                assert_eq!(charts.len(), 1);
                assert_eq!(charts[0].step_type, kind);
                assert_eq!(charts[0].difficulty, "Easy");
                assert_eq!(charts[0].peak_nps.to_bits(), 0.0f64.to_bits());
            }
        }
    }
    // A note at beat zero still has nonzero measure density, unlike duration.
    let sm = b"#BPMS:0=120;#NOTES:dance-single::Easy:3::\n1000\n;";
    assert_eq!(
        compute_chart_peak_nps(sm, "sm").expect("valid SM")[0].peak_nps,
        0.5
    );
}

#[test]
fn peak_cache_transitions() {
    let header = "#VERSION:0.83;#OFFSET:0.25;#BPMS:0=120;#STOPS:2=1;";
    let tags = [
        "#BPMS: \u{1}0=180, ;#STOPS: 2=0.25, ;#DELAYS: 1=0.125, ;#WARPS: 3=0.5, ;",
        "#BPMS: \u{1}0=180, ;#STOPS: 2=0.25, ;#DELAYS: 1=0.125, ;#WARPS: 3=0.5, ;#FAKES:0=8;",
        "",
        "#BPMS: \u{1}0=180, ;#STOPS: 2=0.25, ;#DELAYS: 1=0.125, ;#WARPS: 3=0.5, ;",
        "#OFFSET:100000;#BPMS:0=180;#STOPS:2=0.25;#DELAYS:1=0.125;#WARPS:3=0.5;",
        "#BPMS:0=180;#STOPS:2=0.5;#DELAYS:1=0.125;#WARPS:3=0.5;",
        "#BPMS:0=180;#STOPS:2=0.5;#DELAYS:1=0.25;#WARPS:3=0.5;",
        "#BPMS:0=180;#STOPS:2=0.5;#DELAYS:1=0.25;#WARPS:3=1;",
        "#BPMS:0=240;#STOPS:2=0.5;#DELAYS:1=0.25;#WARPS:3=1;",
        "#SPEEDS:;",
        "#LABELS:0=x;",
    ];
    let charts: Vec<_> = tags.iter().enumerate().map(|(i, tags)| {
        let notes = if i == 2 { "0000\n" } else { "1000\n0100\n0010\n0001\n,\n1000\n1000\n" };
        format!("#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:9;{tags}#NOTES:\n{notes};\n")
    }).collect();
    let skipped = concat!(
        "#NOTEDATA:;#STEPSTYPE:lights-cabinet;#DIFFICULTY:Hard;#METER:9;",
        "#BPMS:0=999;#NOTES:\n11111111\n;\n",
    );
    let batch = format!("{header}{skipped}{}", charts.join(skipped));
    let actual = compute_chart_peak_nps(batch.as_bytes(), "ssc").expect("valid batch");
    assert_eq!(actual.len(), charts.len());
    for (chart, isolated) in actual.iter().zip(&charts) {
        let isolated = format!("{header}{isolated}");
        let expected = compute_chart_peak_nps(isolated.as_bytes(), "ssc").expect("valid chart");
        assert_eq!(chart.step_type, expected[0].step_type);
        assert_eq!(chart.difficulty, expected[0].difficulty);
        assert_eq!(chart.peak_nps.to_bits(), expected[0].peak_nps.to_bits());
    }
}

#[test]
fn zero_duration_cache() {
    use rssp::{TimingOffsets, compute_chart_durations};
    let header = "#VERSION:0.83;\n#OFFSET:0.25;\n#BPMS:0=120;\n";
    let local = concat!(
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:10;\n",
        "#OFFSET:1;\n#BPMS:0=180;\n#STOPS:0=0.25;\n#NOTES:\n0000\n1000\n;\n",
    );
    let first = concat!(
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Easy;\n#METER:3;\n",
        "#OFFSET:-2;\n#BPMS:0=60;\n#STOPS:0=1;\n#DELAYS:0=0.5;\n#NOTES:\n1000\n;\n",
    );
    let empty = concat!(
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Beginner;\n#METER:1;\n",
        "#NOTES:\n0000\n0000\n;\n",
    );
    let global = concat!(
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Medium;\n#METER:5;\n",
        "#NOTES:\n0000\n1000\n;\n",
    );
    for offsets in [
        TimingOffsets::default(),
        TimingOffsets {
            global_offset_seconds: 100.0,
            group_offset_seconds: -250.0,
        },
    ] {
        let data = format!("{header}{local}{first}{local}{empty}{global}{first}");
        let actual =
            compute_chart_durations(data.as_bytes(), "ssc", offsets).expect("valid fixture");
        assert_eq!(actual.len(), 6);
        for (chart, isolated) in actual
            .iter()
            .zip([local, first, local, empty, global, first])
        {
            let isolated = format!("{header}{isolated}");
            let expected = compute_chart_durations(isolated.as_bytes(), "ssc", offsets)
                .expect("valid fixture");
            assert_eq!(chart.step_type, expected[0].step_type);
            assert_eq!(chart.difficulty, expected[0].difficulty);
            assert_eq!(
                chart.duration_seconds.to_bits(),
                expected[0].duration_seconds.to_bits()
            );
        }
        for i in [1, 3, 5] {
            assert_eq!(actual[i].duration_seconds.to_bits(), 0.0f64.to_bits());
        }
    }
    let sm = b"#BPMS:0=120;#STOPS:0=1;#NOTES:dance-single::Easy:1::\n1000\n;";
    let actual =
        compute_chart_durations(sm, "sm", TimingOffsets::default()).expect("valid fixture");
    assert_eq!(actual[0].duration_seconds.to_bits(), 0.0f64.to_bits());
}

#[test]
fn small_bpm_keeps_bits() {
    use rssp::bpm::{
        compute_bpm_map_stats, compute_bpm_range_and_stats_with_scratch, compute_bpm_stats,
    };
    for len in [1, 2, 3, 8, 31, 32, 33, 64] {
        for source in [
            &[120.0, 180.0, 90.0, 10_000.0, 0.0, -0.0][..],
            &[0.0, -0.0, -120.0, f64::MAX, f64::MIN][..],
            &[
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
                f64::from_bits(0x7ff0_0000_0000_1234),
            ][..],
            &[f64::from_bits(0x7ff8_0000_0000_1234), -0.0, 0.0][..],
        ] {
            let values: Vec<_> = (0..len).map(|i| source[i % source.len()]).collect();
            let map: Vec<_> = values.iter().map(|&v| (0.0, v)).collect();
            let expected = std::panic::catch_unwind(|| {
                compute_bpm_range_and_stats_with_scratch(&map, &mut Vec::new())
            });
            for actual in [
                std::panic::catch_unwind(|| compute_bpm_stats(&values)),
                std::panic::catch_unwind(|| compute_bpm_map_stats(&map)),
            ] {
                match (expected.as_ref(), actual) {
                    (Ok(expected), Ok(actual)) => {
                        assert_eq!(actual.0.to_bits(), expected.2.to_bits(), "median len={len}");
                        assert_eq!(
                            actual.1.to_bits(),
                            expected.3.to_bits(),
                            "average len={len}"
                        );
                    }
                    // The original NaN comparator can reject non-total ordering.
                    (Err(_), Err(_)) => {}
                    _ => panic!("BPM sort behavior changed for len={len}"),
                }
            }
        }
    }
}

#[test]
fn stream_gaps_match() {
    use rssp::streams::{StreamSegment, stream_sequences, visit_stream_sequences};
    assert!(stream_sequences(&[]).is_empty());
    assert!(stream_sequences(&[0, 15, 0, 8]).is_empty());
    assert_eq!(
        stream_sequences(&[0, 0, 16, 0, 32, 0, 0]),
        vec![
            StreamSegment {
                start: 0,
                end: 2,
                is_break: true
            },
            StreamSegment {
                start: 2,
                end: 3,
                is_break: false
            },
            StreamSegment {
                start: 4,
                end: 5,
                is_break: false
            },
            StreamSegment {
                start: 5,
                end: 7,
                is_break: true
            },
        ]
    );
    for len in [0, 1, 2, 8, 32, 4096] {
        let measures: Vec<_> = (0..len).map(|i| [0, 16, 32, 0, 15][i % 5]).collect();
        let mut visited = Vec::new();
        visit_stream_sequences(&measures, |segment| {
            visited.push(segment);
            Ok::<(), std::convert::Infallible>(())
        })
        .expect("infallible visitor");
        assert_eq!(stream_sequences(&measures), visited);
    }
}

#[test]
fn stream_visitor_stops() {
    use rssp::streams::{stream_sequences, visit_stream_sequences};
    let mut late = vec![0; 4096];
    late.push(16);
    let segments = stream_sequences(&late);
    assert_eq!(segments.len(), 2);
    assert_eq!(
        (segments[0].start, segments[0].end, segments[0].is_break),
        (0, 4096, true)
    );
    assert_eq!(
        (segments[1].start, segments[1].end, segments[1].is_break),
        (4096, 4097, false)
    );
    let mut calls = 0;
    assert_eq!(
        visit_stream_sequences(&late, |_| {
            calls += 1;
            Err("stop")
        }),
        Err("stop")
    );
    assert_eq!(calls, 1);
}

#[test]
fn custom_owned_matches_reuse() {
    use rssp::patterns::{
        compile_custom_patterns, detect_custom_patterns, detect_custom_patterns_compiled,
    };
    for patterns in [
        Vec::new(),
        vec![String::new()],
        ["", "l", "L", "ld", "LD", "du", "?", "é", "É"]
            .map(str::to_owned)
            .to_vec(),
    ] {
        let compiled = compile_custom_patterns(&patterns);
        for masks in [&[][..], &[1, 2, 4, 8, 1, 2, 0, 0, 17, 18, 31][..]] {
            let expected = detect_custom_patterns_compiled(masks, &compiled);
            assert_eq!(detect_custom_patterns(masks, &patterns), expected);
            assert_eq!(detect_custom_patterns_compiled(masks, &compiled), expected);
        }
    }
    let patterns = ["l", "L", "ld", "u"].map(str::to_owned);
    let actual = detect_custom_patterns(&[1, 2, 1, 2, 4], &patterns);
    assert_eq!(
        actual
            .iter()
            .map(|p| (p.pattern.as_str(), p.count))
            .collect::<Vec<_>>(),
        [("L", 2), ("LD", 2), ("U", 1)]
    );
    let patterns: Vec<_> = (1..=64).map(|len| "L".repeat(len)).collect();
    let compiled = compile_custom_patterns(&patterns);
    let actual = detect_custom_patterns_compiled(&[1; 256], &compiled);
    assert_eq!(detect_custom_patterns(&[1; 256], &patterns), actual);
    for (len, summary) in (1u32..=64).zip(actual) {
        assert_eq!(summary.count, 257 - len); // Every overlapping suffix matches.
    }
}

#[test]
fn single_bpm_preserves_bits() {
    use rssp::bpm::{
        compute_bpm_map_stats, compute_bpm_range, compute_bpm_range_and_stats,
        compute_bpm_range_and_stats_with_scratch, compute_bpm_stats,
    };
    for bpm in [
        120.0,
        0.0,
        -0.0,
        -120.0,
        10_000.0,
        f64::MAX,
        f64::MIN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0x7ff8_0000_0000_1234),
    ] {
        let map = [(0.0, bpm)];
        let expected_range = compute_bpm_range(&map);
        let actual = compute_bpm_stats(&[bpm]);
        assert_eq!(actual.0.to_bits(), bpm.to_bits());
        assert_eq!(actual.1.to_bits(), bpm.to_bits());
        let mapped = compute_bpm_map_stats(&map);
        assert_eq!(mapped.0.to_bits(), actual.0.to_bits());
        assert_eq!(mapped.1.to_bits(), actual.1.to_bits());
        let combined = compute_bpm_range_and_stats(&map);
        assert_eq!((combined.0, combined.1), expected_range);
        assert_eq!(combined.2.to_bits(), actual.0.to_bits());
        assert_eq!(combined.3.to_bits(), actual.1.to_bits());
        let mut scratch = vec![180.0; 8];
        let capacity = scratch.capacity();
        let warm = compute_bpm_range_and_stats_with_scratch(&map, &mut scratch);
        assert_eq!((warm.0, warm.1), expected_range);
        assert_eq!(warm.2.to_bits(), actual.0.to_bits());
        assert_eq!(warm.3.to_bits(), actual.1.to_bits());
        assert_eq!(scratch.len(), 1);
        assert_eq!(scratch[0].to_bits(), bpm.to_bits());
        assert_eq!(scratch.capacity(), capacity);
    }
    // The scratch API retains the original arithmetic for signaling NaNs.
    let bpm = f64::from_bits(0x7ff0_0000_0000_1234);
    let map = [(0.0, bpm)];
    let expected = compute_bpm_range_and_stats_with_scratch(&map, &mut Vec::new());
    for actual in [compute_bpm_stats(&[bpm]), compute_bpm_map_stats(&map)] {
        assert_eq!(actual.0.to_bits(), expected.2.to_bits());
        assert_eq!(actual.1.to_bits(), expected.3.to_bits());
    }
    let actual = compute_bpm_range_and_stats(&map);
    assert_eq!((actual.0, actual.1), (expected.0, expected.1));
    assert_eq!(actual.2.to_bits(), expected.2.to_bits());
    assert_eq!(actual.3.to_bits(), expected.3.to_bits());
}

#[test]
fn cleanup_preserves_prefixes() {
    use rssp::bpm::{clean_norm_map_cow, clean_norm_speeds_cow};
    for prefix in ["", "0=120,", "0=\u{b}120,", "bad,0=120,", "0=NaN,4=inf,"] {
        for tail in ["", " 8=180 ", ",8=180,", "8=1\u{1}80", "\u{a0}8=180\u{a0}"] {
            let raw = format!("{prefix}{tail}");
            let expected = rssp::bpm::clean_and_normalize_float_digits(&raw);
            let actual = clean_norm_map_cow(&raw);
            assert_eq!(actual.0.as_ref(), expected.0);
            assert_eq!(actual.1, expected.1);
        }
    }
    for prefix in ["", "0=1=0=0,", "0=1=0=0=ignored,", "bad,0=1=0=0,"] {
        for tail in [
            "",
            " 8=2=1=1 ",
            ",8=2=1=1,",
            "8=1\u{1}5=0=0",
            "8=1=0=\u{b}0",
        ] {
            let raw = format!("{prefix}{tail}");
            let expected = rssp::bpm::clean_and_normalize_speeds_float_digits(&raw);
            let actual = clean_norm_speeds_cow(&raw);
            assert_eq!(actual.0.as_ref(), expected.0);
            assert_eq!(actual.1, expected.1);
        }
    }
    assert!(matches!(
        clean_norm_map_cow("0=\u{b}120"),
        (std::borrow::Cow::Borrowed(_), _)
    ));
    assert!(matches!(
        clean_norm_speeds_cow("0=1=0=0"),
        (std::borrow::Cow::Borrowed(_), _)
    ));
}

#[test]
fn spacing_matches_minimization() {
    for lanes in [0, 4, 5, 8, 10] {
        let width = if lanes == 0 { 4 } else { lanes };
        for rows in [0usize, 1, 2, 3, 4, 7, 16, 63, 64, 65, 129, 256] {
            for step in [1, 2, 4, 7, 16] {
                for object in *b"1243MX0" {
                    let mut data = b"// ignored\n  ,\r\n".to_vec();
                    for i in 0..rows {
                        data.extend_from_slice(b" \t");
                        let start = data.len();
                        data.resize(start + width, b'0');
                        if i % step == 0 {
                            data[start + i % width] = object;
                        }
                        data.extend_from_slice(b" trailing\r\n");
                    }
                    data.extend_from_slice(b",\n,\n;\n1000\n");
                    let minimized = rssp::stats::minimize_chart_for_hash(&data, lanes);
                    let mut expected = Vec::new();
                    rssp::stats::visit_measure_spacing(&minimized, lanes, |value| {
                        expected.push(value);
                        Ok::<_, std::convert::Infallible>(())
                    })
                    .expect("infallible visitor");
                    assert_eq!(rssp::nps::measure_equally_spaced(&data, lanes), expected);
                }
            }
        }
    }
}

#[test]
fn nps_small_medians() {
    for len in [0, 1, 2, 3, 8, 32, 63, 64, 65, 128] {
        let values: Vec<_> = (0..len).map(|i| f64::from((i * 37) % 23) / 3.0).collect();
        let mut sorted = values.clone();
        let expected = rssp::nps::get_nps_stats_in_place(&mut sorted);
        let mut scratch = Vec::new();
        assert_eq!(rssp::nps::get_nps_stats(&values), expected);
        assert_eq!(
            rssp::nps::get_nps_stats_with_scratch(&values, &mut scratch),
            expected
        );
    }
    for values in [
        vec![-0.0],
        vec![f64::NAN],
        vec![-0.0, 0.0],
        vec![0.0, -0.0],
        vec![f64::NAN, 1.0],
        vec![1.0, f64::NAN],
        vec![f64::NAN, f64::NAN],
        vec![f64::NEG_INFINITY, f64::INFINITY],
        vec![-0.0, 0.0, -0.0],
        vec![f64::NAN, 1.0, 2.0],
        vec![f64::NEG_INFINITY, 0.0, f64::INFINITY],
    ] {
        let mut sorted = values.clone();
        let expected = rssp::nps::get_nps_stats_in_place(&mut sorted);
        let actual = rssp::nps::get_nps_stats(&values);
        assert_eq!(actual.0.to_bits(), expected.0.to_bits());
        assert_eq!(actual.1.to_bits(), expected.1.to_bits());
    }
}

#[test]
fn breakdown_gap_boundaries() {
    use rssp::streams::{BreakdownMode, StreamBreakdownLevel};
    for gap in [0, 1, 2, 4, 5, 31, 32, 33, 128] {
        for same_category in [true, false] {
            let mut measures = vec![0; 3];
            measures.extend([16, 16]);
            measures.resize(measures.len() + gap, 0);
            measures.extend([if same_category { 16 } else { 32 }; 3]);
            measures.extend([0; 3]);
            let (counts, sn, standard) = rssp::streams::compute_stream_outputs(&measures);
            assert_eq!(counts, rssp::streams::compute_stream_counts(&measures));
            assert_eq!(sn, rssp::streams::generate_breakdowns(&measures));
            assert_eq!(standard, rssp::streams::stream_breakdowns(&measures));
            for (mode, expected) in [
                (BreakdownMode::Detailed, &sn.0),
                (BreakdownMode::Partial, &sn.1),
                (BreakdownMode::Simplified, &sn.2),
            ] {
                assert_eq!(
                    &rssp::streams::generate_breakdown(&measures, mode),
                    expected
                );
            }
            for (level, expected) in [
                (StreamBreakdownLevel::Detailed, &standard.0),
                (StreamBreakdownLevel::Partial, &standard.1),
                (StreamBreakdownLevel::Simple, &standard.2),
            ] {
                assert_eq!(&rssp::streams::stream_breakdown(&measures, level), expected);
            }
            assert_eq!(
                rssp::streams::stream_breakdown(&measures, StreamBreakdownLevel::Total),
                "5 Total"
            );
        }
    }
    let measures = [0, 0, 16, 16, 0, 32, 0, 0, 24, 24, 0, 0];
    assert_eq!(
        rssp::streams::generate_breakdowns(&measures),
        (
            "2 =1= (2) \\2\\".into(),
            "2 =1= - \\2\\".into(),
            "2 =3=* \\2\\".into()
        )
    );
    assert_eq!(
        rssp::streams::stream_breakdowns(&measures),
        ("2-1 (2) 2".into(), "2-1-2".into(), "4*-2".into())
    );
}

#[test]
fn peak_aux_overrides() {
    for version in ["0.6", "0.83"] {
        for tag in ["SPEEDS:0=2=1=0", "SCROLLS:0=0.5", "FAKES:0=4", "SPEEDS:"] {
            let data = format!(
                "#VERSION:{version};\n#BPMS:0=120;\n#SPEEDS:0=3=2=1;\n\
                 #SCROLLS:0=0.5;\n#FAKES:0=8;\n#NOTEDATA:;\n\
                 #STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n\
                 #{tag};\n#NOTES:\n1000\n0100\n0010\n0001\n;\n"
            );
            let charts = compute_chart_peak_nps(data.as_bytes(), "ssc").expect("valid SSC");
            assert_eq!(charts.len(), 1);
            assert_eq!(charts[0].step_type, "dance-single");
            assert_eq!(charts[0].difficulty, "Hard");
            // Modern local timing suppresses the song BPM, even for a blank tag.
            assert_eq!(
                charts[0].peak_nps,
                if version == "0.83" { 1.0 } else { 2.0 }
            );
        }
    }
}

#[test]
fn peak_time_segments() {
    for (tags, peak) in [
        ("#BPMS:0=120;", 2.0),
        ("#BPMS:0=120;\n#STOPS:2=2;", 1.0),
        ("#BPMS:0=120;\n#DELAYS:2=2;", 1.0),
        ("#BPMS:0=120;\n#WARPS:1=1;", 4.0 / 1.5),
    ] {
        let data = format!(
            "#VERSION:0.83;\n{tags}\n#SPEEDS:0=2=4=0;\n#SCROLLS:0=0;\n\
             #FAKES:0=4;\n#NOTEDATA:;\n#STEPSTYPE:pump-single;\n\
             #DIFFICULTY:Hard;\n#METER:8;\n#NOTES:\n10000\n01000\n00100\n00010\n;\n"
        );
        let charts = compute_chart_peak_nps(data.as_bytes(), "ssc").expect("valid SSC");
        assert_eq!(charts.len(), 1);
        assert_eq!(charts[0].peak_nps, peak);
    }
}

#[test]
fn snapshot_tag_fallbacks() {
    for (version, bpm_tag, hash, formatted, range) in [
        (
            "0.83",
            "0=240",
            "0.000=240.000",
            "0.000000=240.000000",
            (240.0, 240.0),
        ),
        (
            "0.83",
            " \u{1}0=240,, ",
            "0.000=240.000",
            "0.000000=240.000000",
            (240.0, 240.0),
        ),
        (
            "0.83",
            "\u{1} , ,",
            "0.000=120.000,4.000=180.000",
            "0.000000=120.000000,4.000000=180.000000",
            (120.0, 180.0),
        ),
        (
            "0.6",
            "0=240",
            "0.000=240.000",
            "0.000000=120.000000,4.000000=180.000000",
            (120.0, 180.0),
        ),
    ] {
        let data = format!(
            "#VERSION:{version};\n#BPMS: \u{1}0=120,,4=180, ;\n#NOTEDATA:;\n\
             #STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:10;\n\
             #BPMS:{bpm_tag};\n#DISPLAYBPM:150:300;\n#NOTES:\n1000\n;\n"
        );
        let charts = chart_bpm_snapshots(data.as_bytes(), "ssc").expect("valid SSC");
        assert_eq!(charts.len(), 1);
        let chart = &charts[0];
        assert_eq!(chart.step_type, "dance-single");
        assert_eq!(chart.difficulty, "Challenge");
        assert_eq!(chart.hash_bpms, hash);
        assert_eq!(chart.bpms_formatted, formatted);
        assert_eq!((chart.bpm_min, chart.bpm_max), range);
        assert_eq!(chart.display_bpm, "150 - 300");
        assert_eq!(
            (chart.display_bpm_min, chart.display_bpm_max),
            (150.0, 300.0)
        );
    }
}

#[test]
fn tier_run_boundary() {
    let map = [(0.0, 137.125)];
    for (densities, expected) in [
        (&[32, 128, 32][..], 137.125),
        (&[32, 128, 32, 32][..], 1097.0),
        (&[24, 31, 24, 24, 0, 32, 128, 32][..], 265.679_687_5),
        (&[19, 19, 19, 20, 20, 20, 20][..], 171.40625),
        (&[16, 16, 16, 16, 32, 128, 32, 32, 0][..], 1097.0),
    ] {
        assert_eq!(compute_tier_bpm(densities, &map, 4.0), expected);
    }
}

#[test]
fn fixed_tier_matches() {
    let densities: Vec<_> = (0..512)
        .map(|i| [0, 16, 19, 16, 19, 20, 23, 20, 23, 32, usize::MAX, 32, 256][i % 13])
        .collect();
    for bpm in [
        f64::from_bits(1),
        0.000_001,
        137.125,
        9999.999,
        10_000.0,
        -120.0,
        0.0,
    ] {
        let fixed = compute_tier_bpm(&densities, &[(0.0, bpm)], 4.0);
        let variable = compute_tier_bpm(&densities, &[(0.0, bpm), (4096.0, bpm)], 4.0);
        assert_eq!(fixed.to_bits(), variable.to_bits());
    }
    for bpm in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            compute_tier_bpm(&densities, &[(0.0, bpm)], 4.0).to_bits(),
            bpm.to_bits()
        );
    }
}

#[test]
fn density_preserves_counts() {
    for lanes in [4, 5, 8, 10] {
        let mut data = Vec::new();
        for (rows, spacing) in [(64, 16), (9, 2), (32, 1)] {
            for i in 0..rows {
                let start = data.len();
                data.resize(start + lanes, b'0');
                if i % spacing == 0 {
                    data[start] = b"124"[i % 3];
                    data[start + lanes - 1] = b'1'; // Jumps still count once.
                }
                data.extend_from_slice(b"\r\n");
            }
            data.extend_from_slice(b",\n");
        }
        data.extend_from_slice(b"// comment\n\t0000\n;\n1111111111\n");
        assert_eq!(rssp::stats::measure_densities(&data, lanes), [4, 5, 32, 0]);
    }
    for (data, expected) in [
        (&b""[..], &[0][..]),
        (&b",\n,\n;"[..], &[0, 0, 0][..]),
        (&b"  1111\n0000\n"[..], &[1][..]),
        (
            &b"M000\nF000\nL000\n3000\n2000\n4000\n1000\n;"[..],
            &[3][..],
        ),
    ] {
        assert_eq!(rssp::stats::measure_densities(data, 4), expected);
    }
    assert_eq!(rssp::stats::measure_densities(b"00001\n1000\n;", 99), [1]);
}

#[test]
fn duration_cache_keys() {
    let data = concat!(
        "#VERSION:0.83;\n#OFFSET:0.5;\n#BPMS:0=120;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Easy;\n#METER:4;\n",
        "#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Medium;\n#METER:4;\n",
        "#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:4;\n",
        "#OFFSET:1;\n#BPMS: \u{1}0=120, ;\n#STOPS: 2=0.5, ;\n#DELAYS: 1=0.25, ;\n#WARPS: 1=0.5, ;\n",
        "#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        "#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:4;\n",
        "#BPMS:0=240;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n",
    );
    let charts =
        rssp::compute_chart_durations(data.as_bytes(), "ssc", rssp::TimingOffsets::default())
            .expect("valid SSC");
    assert_eq!(charts.len(), 4);
    for (chart, (difficulty, duration)) in charts.iter().zip([
        ("Easy", 1.5),
        ("Medium", 1.5),
        ("Hard", 1.0),
        ("Challenge", 0.25),
    ]) {
        assert_eq!(chart.step_type, "dance-single");
        assert_eq!(chart.difficulty, difficulty);
        assert_eq!(chart.duration_seconds, duration);
    }
}

#[test]
fn snapshot_aux_maps() {
    for version in ["0.6", "0.83"] {
        for tag in [
            "SPEEDS: \u{1}0=2=1=0, ",
            "SCROLLS:0=-1",
            "FAKES:0=4",
            "DELAYS:0=4",
            "WARPS:0=4",
            "SPEEDS:",
        ] {
            let data = format!(
                "#VERSION:{version};\n#BPMS:0=120,4=180;\n#SPEEDS:0=2=1=0;\n\
                 #SCROLLS:0=-1;\n#FAKES:0=4;\n#NOTEDATA:;\n#STEPSTYPE:dance-single;\n\
                 #DIFFICULTY:Hard;\n#METER:8;\n#{tag};\n#NOTES:\n1000\n;\n"
            );
            let charts = chart_bpm_snapshots(data.as_bytes(), "ssc").expect("valid SSC");
            assert_eq!(charts.len(), 1);
            assert_eq!(charts[0].hash_bpms, "0.000=120.000,4.000=180.000");
            assert_eq!(
                charts[0].bpms_formatted,
                "0.000000=120.000000,4.000000=180.000000"
            );
            assert_eq!((charts[0].bpm_min, charts[0].bpm_max), (120.0, 180.0));
            assert_eq!(charts[0].display_bpm, "120 - 180");
        }
    }
}
#[test]
fn tidy_bpm_fallback() {
    for beat in [
        "0", "-0", "1.2345", "1e308", "-1e308", "inf", "-inf", "NaN", "bad", " 1\u{1} ",
    ] {
        for bpm in ["120", "0", "-0", "-120", "1e308", "inf", "NaN", "bad"] {
            let input = format!("{beat}={bpm},0=180,4=240");
            assert_eq!(rssp::bpm::normalize_and_tidy_bpms(&input), "0.000=60.000");
        }
    }
    for input in ["", ",", "no pairs", "0=120", "0=120,4=180"] {
        assert_eq!(rssp::bpm::normalize_and_tidy_bpms(input), "0.000=60.000");
    }
}

#[test]
fn hash_bpm_transitions() {
    let header = b"#VERSION:0.83;\n#BPMS:0=120,4=180;\n";
    let maps: [&[u8]; 12] = [
        b"#BPMS:0=120,4=180;",
        b"#BPMS:0=160,4=200;",
        b"#BPMS:0=160,4=200;",
        b"",
        b"#BPMS:0=160,4=200;",
        b"#BPMS:;",
        b"#BPMS:;",
        b"#BPMS:\xff;",
        b"#BPMS:\xff;",
        b"#BPMS: 0=160,4=200 ;",
        b"#BPMS: 0=160,4=200 ;",
        b"#BPMS:0=120,4=180;",
    ];
    let mut batch = header.to_vec();
    let mut expected = Vec::new();
    for map in maps {
        let mut chart =
            b"#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n#METER:8;\n".to_vec();
        chart.extend_from_slice(map);
        chart.extend_from_slice(b"\n#NOTES:\n1000\n0100\n0010\n0001\n;\n");
        batch.extend_from_slice(&chart);
        let mut single = header.to_vec();
        single.extend_from_slice(&chart);
        let isolated = rssp::compute_all_hashes(&single, "ssc").expect("valid single chart");
        assert_eq!(isolated.len(), 1);
        expected.push(isolated[0].hash.clone());
    }
    let actual = rssp::compute_all_hashes(&batch, "ssc").expect("valid batch");
    assert_eq!(actual.len(), expected.len());
    for (chart, hash) in actual.iter().zip(expected) {
        assert_eq!(chart.step_type, "dance-single");
        assert_eq!(chart.difficulty, "Hard");
        assert_eq!(chart.hash, hash);
    }
}

#[test]
fn credit_owned_output() {
    let options = rssp::AnalysisOptions {
        compute_tech_counts: false,
        compute_pattern_counts: false,
        ..Default::default()
    };
    for bytes in [
        &b" plain author "[..],
        &b"escaped\\:author "[..],
        &b" \x93author\x94 "[..],
        &b" \x93author\\:\x94 "[..],
        &b"\xff\\\\author\\"[..],
        &b"\xc3\xa9\\:\xc3\xb1 "[..],
    ] {
        let decoded = rssp::parse::decode_bytes(bytes);
        let expected = rssp::parse::unescape_tag(decoded.as_ref());
        for version in ["0.6", "0.83"] {
            let mut data = format!("#VERSION:{version};#BPMS:0=120;#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:Hard;#METER:8;#CREDIT:").into_bytes();
            data.extend_from_slice(bytes);
            // A terminator must not be escaped by the final input byte.
            data.extend_from_slice(b" end;#NOTES:\n1000\n;\n");
            let mut source = bytes.to_vec();
            source.extend_from_slice(b" end");
            let decoded = rssp::parse::decode_bytes(&source);
            let complete = rssp::parse::unescape_tag(decoded.as_ref());
            let summary = rssp::analyze(&data, "ssc", &options).expect("valid credit fixture");
            assert_eq!(summary.charts.len(), 1);
            assert_eq!(summary.charts[0].step_artist_str, complete.as_ref());
            assert_eq!(
                summary.charts[0].tech_notation_str,
                rssp::tech::parse_tech_notation(complete.as_ref(), "")
            );
        }
        assert_eq!(
            rssp::parse::decode_unescape(bytes).as_ref(),
            expected.as_ref()
        );
    }
}

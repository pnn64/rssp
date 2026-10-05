use super::*;
use crate::perf::measure;
use std::hint::black_box;

#[test]
fn escaped_runs_edges() {
    for (raw, expected) in [
        ("", ""),
        ("plain é🙂", "plain é🙂"),
        ("\\", "\\"),
        ("\\\\", "\\"),
        ("\\\\\\", "\\\\"),
        ("\\é\\🙂tail", "é🙂tail"),
        ("é\\🙂\\\\end\\", "é🙂\\end\\"),
        ("\\a\\b\\c", "abc"),
        ("\\\0\\\u{85}", "\0\u{85}"),
    ] {
        let value = unescape_tag(raw);
        assert_eq!(value, expected);
        assert_eq!(matches!(value, Cow::Owned(_)), raw.contains('\\'));
        for padding in [1, 64, 4096] {
            let prefix = "é🙂a".repeat(padding);
            let text = format!("{prefix}{raw}");
            assert_eq!(unescape_tag(&text), format!("{prefix}{expected}"));
        }
    }
    // Long runs after an early escape exercise bulk copying, including multiple
    // UTF-8 boundaries, escaped backslashes and a final unpaired backslash.
    for padding in [1, 64, 4096] {
        let literal = "日é🙂a".repeat(padding);
        let raw = format!("\\é{literal}\\🙂{literal}\\\\{literal}\\");
        let expected = format!("é{literal}🙂{literal}\\{literal}\\");
        let value = unescape_tag(&raw);
        assert_eq!(value, expected);
        assert!(matches!(value, Cow::Owned(_)));
    }
}

#[test]
#[ignore = "explicit escaped-run parity transcript"]
fn escape_trace() {
    let atoms = ["a", "é", "🙂", "\\", "\0", "\u{85}"];
    for length in 0..=5u32 {
        for index in 0..atoms.len().pow(length) {
            let mut rest = index;
            let mut text = String::new();
            for _ in 0..length {
                text.push_str(atoms[rest % atoms.len()]);
                rest /= atoms.len();
            }
            let output = unescape_tag(&text);
            println!(
                "escaped-runs {length} {index} {:?} {}",
                output,
                matches!(output, Cow::Owned(_))
            );
        }
    }
}

#[test]
#[ignore = "explicit escaped-run benchmark"]
fn escape_hotpath() {
    for length in [0, 16, 4096] {
        let clean = "a".repeat(length);
        for (kind, text) in [
            ("clean", clean.clone()),
            ("early", format!("\\:{clean}")),
            ("late", format!("{clean}\\:")),
            ("dense", "\\:".repeat(length / 2)),
            ("unicode", format!("\\🙂{}\\é", "日".repeat(length / 3))),
            ("trailing", format!("{clean}\\")),
        ] {
            measure(&format!("escape_runs/{length}_{kind}"), length, || {
                black_box(unescape_tag(black_box(&text)));
            });
        }
    }
}

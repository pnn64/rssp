use super::{black_box, measure, measure_prepared};

fn escapes(length: usize) -> Vec<(&'static str, String)> {
    let clean = "a".repeat(length);
    vec![
        ("clean", clean.clone()),
        ("early", format!("\\:{clean}")),
        ("late", format!("{clean}\\:")),
        ("dense", "\\:".repeat(length / 2)),
        (
            "unicode",
            format!("{}\u{1f600}\\\u{e9}", "\u{65e5}".repeat(length / 3)),
        ),
        ("trailing", format!("{clean}\\")),
    ]
}

fn markers(length: usize) -> Vec<(&'static str, String)> {
    let clean = "a".repeat(length);
    vec![
        ("clean", clean.clone()),
        ("early", format!("&#65;{clean}")),
        ("late", format!("{clean}&#65;")),
        ("dense", "&#65;".repeat(length / 5)),
        ("unknown", format!("{clean}&unknown;")),
        ("nested", format!("{clean}&bad&ha;")),
        (
            "unicode",
            format!("{}\u{1f600}&#x266F;", "\u{65e5}".repeat(length / 3)),
        ),
        ("trailing", format!("{clean}&")),
    ]
}

fn titles(length: usize) -> Vec<(&'static str, String)> {
    let clean = "a".repeat(length);
    vec![
        ("clean", clean.clone()),
        ("tagged", format!(" [Pack] [01] 2.5- {clean} ")),
        ("spaces", format!("\u{2003}{clean}\u{2003}")),
        (
            "unicode",
            format!(" [Pack] 2- {} ", "\u{65e5}".repeat(length / 3)),
        ),
    ]
}

fn fixture(title: &str) -> Vec<u8> {
    format!(
        "#VERSION:0.83;\n#TITLE:{};\n#ARTIST:Author;\n#BPMS:0=120;\n\
         #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Hard;\n\
         #METER:8;\n#NOTES:\n1000\n0100\n0010\n0001\n;\n",
        title.replace(';', "\\;")
    )
    .into_bytes()
}

pub fn cases(iters: usize) {
    for length in [16, 4096] {
        for (kind, text) in escapes(length) {
            measure(&format!("unescape/{length}_{kind}"), length, iters, || {
                black_box(rssp::parse::unescape_tag(black_box(&text)));
            });
            measure(
                &format!("decode_escape/{length}_{kind}"),
                length,
                iters,
                || {
                    black_box(rssp::parse::decode_unescape(black_box(text.as_bytes())));
                },
            );
        }
        for (kind, text) in markers(length) {
            measure_prepared(
                &format!("markers/{length}_{kind}"),
                length,
                iters,
                || text.clone(),
                |input| {
                    rssp::translate::replace_markers_in_place(black_box(input));
                },
            );
            measure(
                &format!("markers_owned/{length}_{kind}"),
                length,
                iters,
                || {
                    black_box(rssp::translate::replace_markers(black_box(&text)));
                },
            );
        }
        for (group, values, strip_tags, translate_markers) in [
            ("title", titles(length), true, false),
            ("escape", escapes(length), false, false),
            ("marker", markers(length), false, true),
        ] {
            let options = rssp::AnalysisOptions {
                strip_tags,
                translate_markers,
                compute_tech_counts: false,
                compute_pattern_counts: false,
                ..Default::default()
            };
            let mut scratch = rssp::AnalysisScratch::default();
            for (kind, title) in values {
                // A final unescaped slash would consume the simfile terminator.
                let title = if group == "escape" {
                    format!("{title} end")
                } else {
                    title
                };
                let data = fixture(&title);
                measure(
                    &format!("metadata/{group}_{length}_{kind}"),
                    1,
                    iters,
                    || {
                        black_box(
                            rssp::analyze_with_scratch(
                                black_box(&data),
                                "ssc",
                                black_box(&options),
                                &mut scratch,
                            )
                            .expect("valid metadata fixture"),
                        );
                    },
                );
            }
        }
    }
}

pub fn verify() {
    for length in [0, 1, 16, 4096] {
        for (kind, text) in escapes(length) {
            println!(
                "escape {length} {kind} {:?}",
                rssp::parse::unescape_tag(&text)
            );
            println!(
                "decode {length} {kind} {:?}",
                rssp::parse::decode_unescape(text.as_bytes())
            );
        }
        for (kind, text) in markers(length) {
            let mut inplace = text.clone();
            let pointer = inplace.as_ptr();
            let capacity = inplace.capacity();
            rssp::translate::replace_markers_in_place(&mut inplace);
            assert_eq!(inplace, rssp::translate::replace_markers(&text));
            assert_eq!(inplace.as_ptr(), pointer);
            assert_eq!(inplace.capacity(), capacity);
            println!("marker {length} {kind} {inplace:?}");
        }
        for strip_tags in [false, true] {
            for translate_markers in [false, true] {
                let options = rssp::AnalysisOptions {
                    strip_tags,
                    translate_markers,
                    compute_tech_counts: false,
                    compute_pattern_counts: false,
                    ..Default::default()
                };
                for (kind, title) in titles(length) {
                    let data = fixture(&title);
                    let summary = rssp::analyze(&data, "ssc", &options).expect("valid fixture");
                    println!(
                        "title {length} {kind} {strip_tags} {translate_markers} {:?}",
                        summary.title_str
                    );
                }
            }
        }
    }
}

use crate::perf::measure;
use std::hint::black_box;

use crate::perf::fixtures;

#[test]
fn title_match_edges() {
    for kind in [
        "plain",
        "escaped",
        "cp1252",
        "cp_escape",
        "controls",
        "cp_controls",
    ] {
        let (data, expected) = fixtures::title(4, kind);
        assert_eq!(
            super::simfile_translit_title_eq(&data, "sm", &expected),
            Some(true)
        );
        assert_eq!(
            super::simfile_translit_title_eq(&data, "sm", "other"),
            Some(false)
        );
    }
    for (data, expected) in [
        (
            b"#TITLE:Wrong;#TITLETRANSLIT: \tRi\\ght\r ;#SUBTITLE:Wrong;#SUBTITLETRANSLIT: \\Mix ;"
                .as_slice(),
            "right Mix",
        ),
        (b"#TITLE: Name ;#SUBTITLE: ;".as_slice(), "name"),
        (b"#TITLE: ;#SUBTITLE: Mix;".as_slice(), " Mix"),
        (b"#TITLE:Caf\xe9;".as_slice(), "Café"),
    ] {
        assert_eq!(
            super::simfile_translit_title_eq(data, "sm", expected),
            Some(true)
        );
    }
    assert_eq!(
        super::simfile_translit_title_eq(b"#TITLE:A;", "bad", "A"),
        None
    );
}

#[test]
fn course_scan_edges() {
    for (data, expected) in [
        (b"".as_slice(), None),
        (b"abc".as_slice(), None),
        (b";".as_slice(), Some((0, 1))),
        (b"abc;tail".as_slice(), Some((3, 4))),
        (b"a\\;b;tail".as_slice(), Some((4, 5))),
        (b"a\\\\;tail".as_slice(), Some((3, 4))),
        (b"a\\\\\\;b;tail".as_slice(), Some((6, 7))),
        (b"a\\;b\\;".as_slice(), None),
    ] {
        assert_eq!(super::scan_term(data), expected);
    }
    let parsed = super::parse_crs(b"noise #COURSE:Name\\;Part;#DESCRIPTION:Text\\\\;#UNKNOWN:skip;#LIVES: 7 ;#REPEAT:YES;#SONG:Group/Song:Hard:1.5x;#BANNER:tail").expect("valid course");
    assert_eq!(parsed.name, "Name\\;Part");
    assert_eq!(parsed.description, "Text\\\\");
    assert_eq!(parsed.banner, "tail");
    assert_eq!(parsed.lives, 7);
    assert!(parsed.repeat);
    assert_eq!(parsed.entries.len(), 1);
    assert!(super::parse_crs(b"#missing-colon").is_err());
}

#[test]
#[ignore = "explicit loader benchmark"]
fn loader_hotpath() {
    for length in [16, 4096] {
        for kind in [
            "plain",
            "escaped",
            "cp1252",
            "cp_escape",
            "controls",
            "cp_controls",
        ] {
            let (data, expected) = fixtures::title(length, kind);
            measure(&format!("title_match/{length}_{kind}"), 1, || {
                black_box(super::simfile_translit_title_eq(
                    black_box(&data),
                    "sm",
                    black_box(&expected),
                ));
            });
        }
        for kind in ["plain", "escaped", "unterminated", "dense"] {
            let mut data = if kind == "dense" {
                b"a\\;".repeat(length / 3)
            } else {
                vec![b'a'; length]
            };
            if kind == "escaped" {
                data.extend_from_slice(b"\\;x");
            }
            if kind != "unterminated" {
                data.push(b';');
            }
            measure(&format!("course_scan/{length}_{kind}"), length, || {
                black_box(super::scan_term(black_box(&data)));
            });
        }
    }
}

#[test]
#[ignore = "explicit course title parity transcript"]
fn title_trace() {
    for length in [0, 1, 16, 4096] {
        for kind in [
            "plain",
            "escaped",
            "cp1252",
            "cp_escape",
            "controls",
            "cp_controls",
        ] {
            let (data, expected) = fixtures::title(length, kind);
            for extension in ["sm", "ssc", "bad"] {
                for value in [&expected, "other"] {
                    println!(
                        "course-title {length} {kind} {extension} {value:?} {:?}",
                        super::simfile_translit_title_eq(&data, extension, value)
                    );
                }
            }
        }
    }
}

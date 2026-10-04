#![allow(dead_code)] // Shared fixtures are included by several focused benchmarks.

use std::fmt::Write as _;

pub fn course(count: usize, length: usize, escaped: bool) -> Vec<u8> {
    let padding = "a".repeat(length);
    let value = if escaped {
        format!("{padding}\\;tail")
    } else {
        padding
    };
    let mut data = format!("{value}\n#COURSE:Test;#DESCRIPTION:{value};\n");
    for index in 0..count {
        writeln!(data, "#UNKNOWN:{value};#SONG:Group/Song {index}:Hard:1.5x;")
            .expect("String write");
    }
    data.into_bytes()
}

pub fn title(length: usize, kind: &str) -> (Vec<u8>, String) {
    let (raw, decoded): (&[u8], &str) = match kind {
        "plain" => (b"a", "a"),
        "escaped" => (b"\\a", "a"),
        "cp1252" => (&[0xe9], "é"),
        "cp_escape" => (&[b'\\', 0xe9], "é"),
        _ => unreachable!("known fixture kind"),
    };
    let mut data = b"#TITLE:".to_vec();
    data.extend(raw.repeat(length));
    data.extend_from_slice(b";#SUBTITLE:");
    data.extend(raw.repeat(length));
    data.extend_from_slice(b";#BPMS:0=120;#NOTES:dance-single::Hard:8::1000;");
    (
        data,
        format!("{} {}", decoded.repeat(length), decoded.repeat(length)),
    )
}

pub fn numeric(count: usize, kind: &str, triple: bool) -> String {
    let mut text = String::new();
    for index in 0..count {
        if index != 0 {
            text.push(',');
        }
        let beat = index * 4;
        let value = index % 7 + 1;
        let segment = if kind == "invalid" {
            "bad=bad".to_owned()
        } else if triple {
            format!("{beat}={value}=4")
        } else {
            format!("{beat}={value}")
        };
        if kind == "padded" {
            write!(text, "\u{2003} {segment} \t\u{2003}")
        } else {
            write!(text, "{segment}")
        }
        .expect("String write");
    }
    text
}

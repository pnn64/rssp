use std::fmt::Write as _;

pub fn map(count: usize, kind: &str) -> String {
    let mut out = String::new();
    for index in 0..count {
        if index != 0 {
            out.push(',');
        }
        let dirty = match kind {
            "first" => index == 0,
            "middle" => index == count / 2,
            "last" => index + 1 == count,
            _ => false,
        };
        let pad = if dirty { " \u{b}" } else { "" };
        write!(out, "{pad}{}={}.125{pad}", index * 4, index % 7 + 120).expect("String write");
    }
    out
}

pub fn simfile(bpms: &str, stops: &str, ext: &str, local: bool) -> Vec<u8> {
    let mut out = format!("#VERSION:0.83;#BPMS:{bpms};#STOPS:{stops};");
    for difficulty in ["Easy", "Hard"] {
        if ext == "sm" {
            write!(out, "#NOTES:dance-single:Map:{difficulty}:8:0,0,0,0,0:").expect("String write");
        } else {
            write!(
                out,
                "#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:{difficulty};#METER:8;"
            )
            .expect("String write");
            if local {
                write!(out, "#BPMS:{bpms};#STOPS:{stops};").expect("String write");
            }
            out.push_str("#NOTES:");
        }
        out.push_str("1000\n0100\n0010\n0001;");
    }
    out.into_bytes()
}

pub fn tag(count: usize, kind: &str) -> Vec<u8> {
    if kind == "blank" {
        " \t\r\n\u{b}\u{c}".repeat(count).into_bytes()
    } else {
        map(count, kind).into_bytes()
    }
}

pub fn blank_file(count: usize) -> Vec<u8> {
    let blank = String::from_utf8(tag(count, "blank")).expect("ASCII fixture");
    let mut out = String::from("#VERSION:0.83;#BPMS:0=120;");
    for difficulty in ["Easy", "Hard"] {
        write!(
            out,
            "#NOTEDATA:;#STEPSTYPE:dance-single;#DIFFICULTY:{difficulty};#METER:8;"
        )
        .expect("String write");
        for key in [
            "BPMS", "STOPS", "DELAYS", "WARPS", "SPEEDS", "SCROLLS", "FAKES",
        ] {
            write!(out, "#{key}:{blank};").expect("String write");
        }
        out.push_str("#NOTES:1000\n0100\n0010\n0001;");
    }
    out.into_bytes()
}

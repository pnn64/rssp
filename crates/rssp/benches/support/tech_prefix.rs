use std::cmp::Reverse;
use std::sync::OnceLock;

const INLINE_TECH_PARTS: usize = 4;
const KNOWN_TECH_LIST: &[&str] = &[
    "24ths", "32nds", "br", "BR", "BR+", "BR-", "BT", "BT+", "BT-", "bu", "BU", "BU+", "BU-",
    "BXF", "BXF+", "BXF-", "bXF", "bXF+", "bXF-", "BxF", "BXf", "BxF+", "BxF-", "bXf", "bXf+",
    "bXf-", "bxF", "bxF+", "bxF-", "B+XF", "BX-F", "BX-F+", "BX+F+", "B+X-F", "B-X-F-", "B-XF+",
    "ds", "DS", "DS++", "DS+", "DS-", "dr", "DR", "DR+", "DR-", "dt", "dt-", "DT", "DT+", "DT-",
    "FL", "FL+", "FL-", "fs", "FS", "FS+", "FS-", "FX", "FX+", "FX-", "GH", "GH+", "GH-", "HA",
    "HA+", "HA-", "HS", "HS+", "HS-", "ITL+", "ja", "ja-", "JA", "JA+", "JA-", "ju", "ju-", "JU",
    "JU+", "JU-", "JUMPS", "JUMPS+", "JUMPS-", "KS", "KS+", "KS-", "KT", "KT+", "KT-", "LOL", "ma",
    "ma-", "MA", "MA+", "MA-", "MD", "MD+", "MD-", "rh", "rh-", "RH", "RH+", "RH-", "Rolls-", "RS",
    "RS+", "RS-", "SC", "SC+", "SC-", "SDS", "SDS+", "SDS-", "SJ", "SJ+", "SJ-", "SK", "SK+",
    "SK-", "SS", "SS+", "SS-", "SKT", "SKT+", "SKT-", "SPD", "SPD+", "SPD-", "STR", "STR+", "STR-",
    "TR", "TR+", "TR-", "WA", "WA+", "WA-", "XMOD", "XMOD+", "XMOD-", "xo", "XO", "XO+", "XO-",
];

const TECH_BUCKET_CAPS: [u8; 256] = {
    let mut caps = [0; 256];
    let mut index = 0;
    while index < KNOWN_TECH_LIST.len() {
        caps[KNOWN_TECH_LIST[index].as_bytes()[0] as usize] += 1;
        index += 1;
    }
    caps
};

type TechTable = [Vec<&'static str>; 256];

pub const REPEAT_COUNT: usize = 64;

pub fn valid_input() -> (String, String) {
    (
        "BR+ FS- 24ths XO+ SKT- ".repeat(REPEAT_COUNT),
        "32nds DS++ JA- WA+ BXF- No Tech ".repeat(REPEAT_COUNT),
    )
}

pub fn invalid_input() -> String {
    "BR+garbage Hard unknown ".repeat(REPEAT_COUNT)
}

#[inline(always)]
fn is_measure_data(input: &str) -> bool {
    let mut has_symbol = false;
    for &byte in input.as_bytes() {
        match byte {
            b'0'..=b'9' => {}
            b'/' | b'-' | b'*' | b'|' | b'~' | b'.' | b'\'' => has_symbol = true,
            _ => return false,
        }
    }
    has_symbol
}

#[inline(always)]
fn best_prefix(remainder: &str, table: &TechTable) -> Option<&'static str> {
    let bytes = remainder.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    table[bytes[0] as usize]
        .iter()
        .copied()
        .find(|pattern| remainder.starts_with(pattern))
}

#[inline(always)]
fn tech_prefixes() -> &'static TechTable {
    static TABLE: OnceLock<TechTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table =
            std::array::from_fn(|key| Vec::with_capacity(TECH_BUCKET_CAPS[key] as usize));
        for &pattern in KNOWN_TECH_LIST {
            table[pattern.as_bytes()[0] as usize].push(pattern);
        }
        for list in &mut table {
            list.sort_unstable_by_key(|pattern| Reverse(pattern.len()));
        }
        table
    })
}

#[inline(always)]
fn push_tech(out: &mut String, tech: &str, reserve: usize) {
    if out.capacity() == 0 {
        out.reserve(reserve);
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(tech);
}

#[inline(always)]
fn append_chunk(chunk: &str, out: &mut String, reserve: usize, table: &TechTable) {
    let Some(first) = best_prefix(chunk, table) else {
        return;
    };
    let mut remainder = &chunk[first.len()..];
    if remainder.is_empty() {
        push_tech(out, first, reserve);
        return;
    }

    let mut parts = [""; INLINE_TECH_PARTS];
    parts[0] = first;
    let mut part_count = 1usize;
    let mut overflow = None;
    while !remainder.is_empty() {
        let Some(best) = best_prefix(remainder, table) else {
            return;
        };
        if part_count < INLINE_TECH_PARTS {
            parts[part_count] = best;
            part_count += 1;
        } else if overflow.is_none() {
            overflow = Some(remainder);
        }
        remainder = &remainder[best.len()..];
    }

    for tech in &parts[..part_count] {
        push_tech(out, tech, reserve);
    }
    if let Some(mut remainder) = overflow {
        while !remainder.is_empty() {
            let best = best_prefix(remainder, table).expect("validated tech prefix");
            push_tech(out, best, reserve);
            remainder = &remainder[best.len()..];
        }
    }
}

#[inline(always)]
fn append_single(input: &str, out: &mut String, reserve: usize, table: &TechTable) {
    let mut chunks = input
        .split(|character: char| character.is_whitespace() || character == ',')
        .filter(|chunk| !chunk.is_empty())
        .peekable();
    while let Some(chunk) = chunks.next() {
        if chunk == "No" && chunks.peek() == Some(&"Tech") {
            let _ = chunks.next();
            continue;
        }
        if is_measure_data(chunk) {
            continue;
        }
        append_chunk(chunk, out, reserve, table);
    }
}

fn parse_legacy(credit: &str, description: &str) -> String {
    let reserve = credit
        .len()
        .saturating_add(description.len())
        .saturating_add(1);
    let mut out = String::new();
    let table = tech_prefixes();
    append_single(credit, &mut out, reserve, table);
    append_single(description, &mut out, reserve, table);
    out
}

pub fn parse(credit: &str, description: &str, legacy: bool) -> String {
    if legacy {
        parse_legacy(credit, description)
    } else {
        rssp::tech::parse_tech_notation(credit, description)
    }
}

fn assert_pair(credit: &str, description: &str) {
    assert_eq!(
        parse(credit, description, false),
        parse(credit, description, true),
        "tech prefix index changed parsed notation"
    );
}

pub fn assert_behavior() {
    for &(credit, description) in &[
        ("", ""),
        ("BR+ FS- 24ths", "XO+ SKT-"),
        ("No Tech 16/24 BR+garbage", "32nds,DS++ JA-"),
        ("BXF-BR+ 1.2.3", "WA+ unknown B+X-F"),
        ("BR+FS-XO+SKT-WA+BXF-", ""),
    ] {
        assert_pair(credit, description);
    }
    for &pattern in KNOWN_TECH_LIST {
        assert_pair(pattern, "");
    }
    let (credit, description) = valid_input();
    assert_pair(&credit, &description);
    assert_pair(&invalid_input(), "");
}

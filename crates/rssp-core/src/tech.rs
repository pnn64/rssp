use std::sync::OnceLock;

const INLINE_TECH_PARTS: usize = 4;

type TechTable = [&'static [&'static str]; 256];

// Prefixes within each byte bucket stay longest-first so concatenated notation
// resolves identically without a runtime sort.
static TECH_PREFIXES: TechTable = {
    let mut table: TechTable = [&[]; 256];
    table[b'2' as usize] = &["24ths"];
    table[b'3' as usize] = &["32nds"];
    table[b'B' as usize] = &[
        "B-X-F-", "BX-F+", "BX+F+", "B+X-F", "B-XF+", "BXF+", "BXF-", "BxF+", "BxF-", "B+XF",
        "BX-F", "BR+", "BR-", "BT+", "BT-", "BU+", "BU-", "BXF", "BxF", "BXf", "BR", "BT", "BU",
    ];
    table[b'D' as usize] = &[
        "DS++", "DS+", "DS-", "DR+", "DR-", "DT+", "DT-", "DS", "DR", "DT",
    ];
    table[b'F' as usize] = &["FL+", "FL-", "FS+", "FS-", "FX+", "FX-", "FL", "FS", "FX"];
    table[b'G' as usize] = &["GH+", "GH-", "GH"];
    table[b'H' as usize] = &["HA+", "HA-", "HS+", "HS-", "HA", "HS"];
    table[b'I' as usize] = &["ITL+"];
    table[b'J' as usize] = &[
        "JUMPS+", "JUMPS-", "JUMPS", "JA+", "JA-", "JU+", "JU-", "JA", "JU",
    ];
    table[b'K' as usize] = &["KS+", "KS-", "KT+", "KT-", "KS", "KT"];
    table[b'L' as usize] = &["LOL"];
    table[b'M' as usize] = &["MA+", "MA-", "MD+", "MD-", "MA", "MD"];
    table[b'R' as usize] = &["Rolls-", "RH+", "RH-", "RS+", "RS-", "RH", "RS"];
    table[b'S' as usize] = &[
        "SDS+", "SDS-", "SKT+", "SKT-", "SPD+", "SPD-", "STR+", "STR-", "SC+", "SC-", "SDS", "SJ+",
        "SJ-", "SK+", "SK-", "SS+", "SS-", "SKT", "SPD", "STR", "SC", "SJ", "SK", "SS",
    ];
    table[b'T' as usize] = &["TR+", "TR-", "TR"];
    table[b'W' as usize] = &["WA+", "WA-", "WA"];
    table[b'X' as usize] = &["XMOD+", "XMOD-", "XMOD", "XO+", "XO-", "XO"];
    table[b'b' as usize] = &[
        "bXF+", "bXF-", "bXf+", "bXf-", "bxF+", "bxF-", "bXF", "bXf", "bxF", "br", "bu",
    ];
    table[b'd' as usize] = &["dt-", "ds", "dr", "dt"];
    table[b'f' as usize] = &["fs"];
    table[b'j' as usize] = &["ja-", "ju-", "ja", "ju"];
    table[b'm' as usize] = &["ma-", "ma"];
    table[b'r' as usize] = &["rh-", "rh"];
    table[b'x' as usize] = &["xo"];
    table
};

#[inline(always)]
fn tech_prefixes() -> &'static TechTable {
    // Keep the table behind one opaque pointer so fat LTO retains the compact
    // lookup loop instead of specializing the static contents into hot callers.
    static INDEX: OnceLock<&'static TechTable> = OnceLock::new();
    INDEX.get_or_init(|| &TECH_PREFIXES)
}

/// Checks if a chunk resembles measure data (contains symbols like / - * | ~ . ' but no letters).
#[inline(always)]
fn is_measure_data(chunk: &str) -> bool {
    let mut has_symbol = false;
    for &b in chunk.as_bytes() {
        match b {
            b'0'..=b'9' => {}
            b'/' | b'-' | b'*' | b'|' | b'~' | b'.' | b'\'' => has_symbol = true,
            _ => return false,
        }
    }
    has_symbol
}

/// Finds the longest tech prefix that matches the remainder.
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
fn append_chunk_as_tech(chunk: &str, out: &mut String, reserve: usize, table: &TechTable) {
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
fn append_single_unicode(input: &str, out: &mut String, reserve: usize, table: &TechTable) {
    let mut chunks = input
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .peekable();

    while let Some(chunk) = chunks.next() {
        if chunk == "No" && chunks.peek() == Some(&"Tech") {
            let _ = chunks.next(); // Skip "Tech"
            continue;
        }

        if is_measure_data(chunk) {
            continue;
        }

        append_chunk_as_tech(chunk, out, reserve, table);
    }
}

#[inline(always)]
fn next_ascii_chunk<'a>(input: &'a str, offset: &mut usize) -> Option<&'a str> {
    let bytes = input.as_bytes();
    while *offset < bytes.len() && (bytes[*offset].is_ascii_whitespace() || bytes[*offset] == b',')
    {
        *offset += 1;
    }
    let start = *offset;
    while *offset < bytes.len() && !bytes[*offset].is_ascii_whitespace() && bytes[*offset] != b',' {
        *offset += 1;
    }
    (start != *offset).then(|| &input[start..*offset])
}

#[inline(always)]
fn append_single_ascii(input: &str, out: &mut String, reserve: usize, table: &TechTable) {
    let mut offset = 0usize;
    while let Some(chunk) = next_ascii_chunk(input, &mut offset) {
        if chunk == "No" {
            let saved = offset;
            if next_ascii_chunk(input, &mut offset) == Some("Tech") {
                continue;
            }
            offset = saved;
        }
        if !is_measure_data(chunk) {
            append_chunk_as_tech(chunk, out, reserve, table);
        }
    }
}

#[inline(always)]
fn append_single_tech(input: &str, out: &mut String, reserve: usize, table: &TechTable) {
    if input.is_ascii() {
        append_single_ascii(input, out, reserve, table);
    } else {
        append_single_unicode(input, out, reserve, table);
    }
}

/// Parses credit and description into a formatted tech notation string.
#[must_use]
pub fn parse_tech_notation(credit: &str, description: &str) -> String {
    let reserve = credit
        .len()
        .saturating_add(description.len())
        .saturating_add(1);
    let mut out = String::new();
    let table = tech_prefixes();
    append_single_tech(credit, &mut out, reserve, table);
    append_single_tech(description, &mut out, reserve, table);
    out
}

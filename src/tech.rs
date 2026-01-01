const KNOWN_TECH_LIST: &[&str] = &[
    "24ths", "32nds", "br", "BR", "BR+", "BR-", "BT", "BT+", "BT-", "bu", "BU", "BU+", "BU-",
    "BXF", "BXF+", "BXF-", "bXF", "bXF+", "bXF-", "BxF", "BXf", "BxF+", "BxF-", "bXf", "bXf+",
    "bXf-", "bxF", "bxF+", "bxF-", "B+XF", "BX-F", "BX-F+", "BX+F+", "B+X-F", "B-X-F-",
    "B-XF+", "ds", "DS", "DS++", "DS+", "DS-", "dr", "DR", "DR+", "DR-", "dt", "dt-", "DT",
    "DT+", "DT-", "FL", "FL+", "FL-", "fs", "FS", "FS+", "FS-", "FX", "FX+", "FX-", "GH",
    "GH+", "GH-", "HA", "HA+", "HA-", "HS", "HS+", "HS-", "ITL+", "ja", "ja-", "JA", "JA+",
    "JA-", "ju", "ju-", "JU", "JU+", "JU-", "JUMPS", "JUMPS+", "JUMPS-", "KS", "KS+", "KS-",
    "KT", "KT+", "KT-", "LOL", "ma", "ma-", "MA", "MA+", "MA-", "MD", "MD+", "MD-", "rh",
    "rh-", "RH", "RH+", "RH-", "Rolls-", "RS", "RS+", "RS-", "SC", "SC+", "SC-", "SDS", "SDS+",
    "SDS-", "SJ", "SJ+", "SJ-", "SK", "SK+", "SK-", "SS", "SS+", "SS-", "SKT", "SKT+", "SKT-",
    "SPD", "SPD+", "SPD-", "STR", "STR+", "STR-", "TR", "TR+", "TR-", "WA", "WA+", "WA-",
    "XMOD", "XMOD+", "XMOD-", "xo", "XO", "XO+", "XO-",
];

/// Checks if a chunk resembles measure data (contains symbols like / - * | ~ . ' but no letters).
#[inline(always)]
fn is_measure_data(chunk: &str) -> bool {
    let mut has_symbol = false;
    for &b in chunk.as_bytes() {
        match b {
            b'0'..=b'9' => {}
            b'/' | b'-' | b'*' | b'|' | b'~' | b'.' | b'\'' => has_symbol = true,
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => return false,
            _ => return false,
        }
    }
    has_symbol
}

/// Finds the longest tech prefix that matches the remainder.
#[inline(always)]
fn best_prefix(remainder: &str) -> Option<&'static str> {
    let mut best = None;
    let mut best_len = 0usize;
    for &pat in KNOWN_TECH_LIST {
        if remainder.starts_with(pat) {
            let len = pat.len();
            if len > best_len {
                best = Some(pat);
                best_len = len;
            }
        }
    }
    best
}

/// Parses a chunk into a sequence of known tech notations using greedy longest prefix matching.
#[inline(always)]
fn parse_chunk_as_tech(chunk: &str) -> Option<Vec<&'static str>> {
    let mut remainder = chunk;
    let mut results = Vec::new();

    while !remainder.is_empty() {
        let Some(best) = best_prefix(remainder) else {
            return None;
        };
        results.push(best);
        remainder = &remainder[best.len()..];
    }

    Some(results)
}

/// Parses a single input string into tech notations, skipping measure data and "No Tech".
#[inline(always)]
fn parse_single_tech(input: &str) -> Vec<&'static str> {
    let mut tech_notations = Vec::new();
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

        if let Some(parsed_list) = parse_chunk_as_tech(chunk) {
            tech_notations.extend(parsed_list);
        }
    }

    tech_notations
}

/// Parses credit and description into a formatted tech notation string.
pub fn parse_tech_notation(credit: &str, description: &str) -> String {
    let mut tech_notations = parse_single_tech(credit);
    tech_notations.extend(parse_single_tech(description));
    tech_notations.join(" ")
}

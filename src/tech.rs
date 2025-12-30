use std::sync::LazyLock;

pub static KNOWN_TECH_LIST: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
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
    ]
});

/// Checks if a chunk resembles measure data (contains symbols like / - * | ~ . ' but no letters).
#[inline(always)]
fn is_measure_data(chunk: &str) -> bool {
    if chunk.chars().any(|c| c.is_ascii_alphabetic() || c == '_') {
        return false;
    }
    let has_measure_symbol = chunk
        .chars()
        .any(|c| matches!(c, '/' | '-' | '*' | '|' | '~' | '.' | '\''));
    if !has_measure_symbol {
        return false;
    }
    chunk
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '/' | '-' | '*' | '|' | '~' | '.' | '\''))
}

/// Parses a chunk into a sequence of known tech notations using greedy longest prefix matching.
#[inline(always)]
fn parse_chunk_as_tech(chunk: &str) -> Option<Vec<String>> {
    let mut remainder = chunk;
    let mut results = Vec::new();

    while !remainder.is_empty() {
        let prefix_matches: Vec<&str> = KNOWN_TECH_LIST
            .iter()
            .copied()
            .filter(|pat| remainder.starts_with(*pat))
            .collect();

        if prefix_matches.is_empty() {
            return None;
        }

        let best = prefix_matches.iter().max_by_key(|p| p.len()).copied()?;
        results.push(best.to_string());
        remainder = &remainder[best.len()..];
    }

    Some(results)
}

/// Parses a single input string into tech notations, skipping measure data and "No Tech".
#[inline(always)]
fn parse_single_tech(input: &str) -> Vec<String> {
    let cleaned = input.trim().replace(',', " ");
    let mut tech_notations = Vec::new();
    let mut chunks = cleaned.split_whitespace().peekable();

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

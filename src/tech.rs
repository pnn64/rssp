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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechNotation(pub String);

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

/// Splits a combined artist string into individual artists, omitting various separators.
#[inline(always)]
fn split_artists(artist_str: &str) -> Vec<String> {
    let mut normalized = artist_str.to_string();
    normalized = normalized.replace(" vs. ", " & ");
    normalized = normalized.replace(" and ", " & ");
    normalized = normalized.replace(" x ", " & ");
    normalized = normalized.replace(" + ", " & ");
    normalized
        .split('&')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parses input into step artists and tech notations, skipping measure data and "No Tech".
pub fn parse_step_artist_and_tech(input: &str) -> (Vec<String>, Vec<TechNotation>) {
    let mut artist_builder = String::new();
    let mut tech_notations = Vec::new();
    let mut chunks = input.split_whitespace().peekable();

    while let Some(chunk) = chunks.next() {
        if chunk == "No" && chunks.peek() == Some(&"Tech") {
            let _ = chunks.next(); // Skip "Tech"
            continue;
        }

        if is_measure_data(chunk) {
            continue;
        }

        if let Some(parsed_list) = parse_chunk_as_tech(chunk) {
            tech_notations.extend(parsed_list.into_iter().map(TechNotation));
        } else {
            if !artist_builder.is_empty() {
                artist_builder.push(' ');
            }
            artist_builder.push_str(chunk);
        }
    }

    let artists = split_artists(&artist_builder);
    (artists, tech_notations)
}

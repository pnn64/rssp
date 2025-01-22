pub static KNOWN_TECH_LIST: &[&str] = &[
    "BR", "BR+", "BR-",
    "BT", "BT+", "BT-",
    "BU", "BU+", "BU-",
    "BXF", "BXF+", "BXF-",
    "bXF", "bXF+", "bXF-",
    "BxF", "BxF+", "BxF-",
    "bXf", "bXf+", "bXf-",
    "bxF", "bxF+", "bxF-",
    "DS", "DS+", "DS-",
    "DR", "DR+", "DR-",
    "DT", "DT+", "DT-",
    "FL", "FL+", "FL-",
    "FS", "FS+", "FS-",
    "GH", "GH+", "GH-",
    "HS", "HS+", "HS-",
    "JA", "JA+", "JA-",
    "JUMPS", "JUMPS+", "JUMPS-",
    "KS", "KS+", "KS-",
    "KT", "KT+", "KT-",
    "MA", "MA+", "MA-",
    "MD", "MD+", "MD-",
    "RH", "RH+", "RH-",
    "SC", "SC+", "SC-",
    "SDS", "SDS+", "SDS-",
    "SJ", "SJ+", "SJ-",
    "SK", "SK+", "SK-",
    "SS", "SS+", "SS-",
    "SKT", "SKT+", "SKT-",
    "STR", "STR+", "STR-",
    "TR", "TR+", "TR-",
    "WA", "WA+", "WA-",
    "XMOD", "XMOD+", "XMOD-",
    "XO", "XO+", "XO-",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechNotation(pub String);

fn is_measure_data(s: &str) -> bool {
    // Example rule: must contain slash/dash/star, no letters
    // so plain numeric chunk remains a name
    if s.chars().any(|c| c.is_ascii_alphabetic() || c == '_') {
        return false;
    }
    // Must have at least one slash/dash/star to be measure data
    // so "4199" remains a step artist name.
    let has_measure_symbol = s.chars().any(|c| c == '/' || c == '-' || c == '*');
    if !has_measure_symbol {
        return false;
    }
    // Now ensure every character is digit or slash/dash/star
    s.chars().all(|c| c.is_ascii_digit() || c == '/' || c == '-' || c == '*')
}

/// Attempts to parse a chunk as a full sequence of known tech notations with no leftover.
/// If successful, returns Some(vector_of_notations), otherwise None.
fn parse_chunk_as_tech(chunk: &str) -> Option<Vec<String>> {
    let mut remainder = chunk;
    let mut results = Vec::new();

    while !remainder.is_empty() {
        // All known patterns that match the *start* of remainder
        let prefix_matches: Vec<&str> = KNOWN_TECH_LIST
            .iter()
            .copied()
            .filter(|pat| remainder.starts_with(*pat))
            .collect();

        if prefix_matches.is_empty() {
            return None;
        }

        // If multiple matches, pick the longest (e.g. "FS+" vs "FS")
        let best = prefix_matches
            .iter()
            .max_by_key(|p| p.len())
            .unwrap();

        results.push((*best).to_string());

        // Remove that prefix
        remainder = &remainder[best.len()..];
    }

    Some(results)
}

pub fn parse_step_artist_and_tech(input: &str) -> (String, Vec<TechNotation>) {
    let mut step_artist = String::new();
    let mut tech_notations = Vec::new();

    // For each whitespace chunk
    for chunk in input.split_whitespace() {
        // Check measure data
        if is_measure_data(chunk) {
            // It's purely measure info => skip
            continue;
        }

        // Attempt to parse the entire chunk as tech notations
        if let Some(parsed_list) = parse_chunk_as_tech(chunk) {
            for pat in parsed_list {
                tech_notations.push(TechNotation(pat));
            }
        } else {
            // Fallback => step artist text
            if step_artist.is_empty() {
                step_artist.push_str(chunk);
            } else {
                step_artist.push(' ');
                step_artist.push_str(chunk);
            }
        }
    }

    (step_artist, tech_notations)
}

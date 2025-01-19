#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TechNotation(pub String);

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
    "XMOD", "XMOD+", "XMOD-",
    "XO", "XO+", "XO-",
];

/// Attempts to parse `chunk` into a list of known tech notations with *no leftover*.
/// Returns `Some(Vec<String>)` if the entire chunk can be fully matched;
/// otherwise returns `None`.
fn parse_chunk_as_tech(chunk: &str, known_list: &[&str]) -> Option<Vec<String>> {
    let mut remainder = chunk;
    let mut results = Vec::new();

    while !remainder.is_empty() {
        // Find all known tech patterns that match *the start* of `remainder`.
        let prefix_matches: Vec<&str> = known_list
            .iter()
            .copied()
            .filter(|pat| remainder.starts_with(*pat))
            .collect();

        if prefix_matches.is_empty() {
            // We can't match the front => fail
            return None;
        }

        // If multiple matches are possible, pick the *longest* one
        // to ensure e.g. "FS+" is used instead of "FS" if both exist.
        let best = prefix_matches
            .iter()
            .max_by_key(|p| p.len())
            .unwrap(); // safe since prefix_matches is non-empty

        // Add this notation to the results
        results.push((*best).to_string());

        // Remove that prefix from remainder
        remainder = &remainder[best.len()..];
    }

    // If we consumed the entire chunk exactly, success:
    Some(results)
}

pub fn parse_step_artist_and_tech(input: &str) -> (String, Vec<TechNotation>) {
    let mut step_artist = String::new();
    let mut tech_notations = Vec::new();

    // Split into chunks by whitespace
    for chunk in input.split_whitespace() {
        // Attempt to parse the entire chunk as a sequence of known tech notations
        if let Some(parsed_list) = parse_chunk_as_tech(chunk, KNOWN_TECH_LIST) {
            // The entire chunk was recognized as one or more tech notations
            for p in parsed_list {
                tech_notations.push(TechNotation(p));
            }
        } else {
            // Could not parse it fully => treat as step artist text
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

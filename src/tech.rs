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

/// Parse the entire string (the second #NOTES: field) into:
/// (step_artist, Vec<TechNotation>)
///
/// This version handles multiple tech notations within a single chunk 
/// without requiring spaces, e.g. "FS+SKT+BU+BR" or "SC-DS-".
pub fn parse_step_artist_and_tech(input: &str) -> (String, Vec<TechNotation>) {
    let mut step_artist = String::new();
    let mut tech_notations = Vec::new();

    // Split on whitespace for initial tokens
    for whitespace_chunk in input.split_whitespace() {
        let mut remainder = whitespace_chunk;
        let mut matched_any = false;
        let mut local_techs = Vec::new();

        loop {
            if remainder.is_empty() {
                break;
            }
            // Find all known patterns that match the start of `remainder`
            let prefix_matches: Vec<&str> = KNOWN_TECH_LIST
                .iter()
                .copied()
                .filter(|pat| remainder.starts_with(*pat))
                .collect();

            if prefix_matches.is_empty() {
                // No further prefix match => stop scanning for more tech notations
                break;
            }

            // Pick the longest match to handle e.g. "FS+" vs "FS"
            let best = prefix_matches
                .iter()
                .max_by_key(|p| p.len())
                .unwrap();

            matched_any = true;
            local_techs.push(TechNotation(best.to_string()));

            // Remove the matched prefix from the front of `remainder`
            remainder = &remainder[best.len()..];
        }

        if matched_any {
            // If there is leftover in remainder after matching all possible tech notations,
            // treat that leftover as part of the step artist name (if non-empty).
            if !remainder.is_empty() {
                if step_artist.is_empty() {
                    step_artist.push_str(remainder);
                } else {
                    step_artist.push(' ');
                    step_artist.push_str(remainder);
                }
            }
            // Add the recognized notations to the main list
            tech_notations.extend(local_techs);
        } else {
            // We didn't match any known tech => this chunk is purely step artist text
            if step_artist.is_empty() {
                step_artist.push_str(whitespace_chunk);
            } else {
                step_artist.push(' ');
                step_artist.push_str(whitespace_chunk);
            }
        }
    }

    (step_artist, tech_notations)
}

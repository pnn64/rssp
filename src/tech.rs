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
///   (step_artist, Vec<TechNotation>)
pub fn parse_step_artist_and_tech(input: &str) -> (String, Vec<TechNotation>) {
    let mut step_artist = String::new();
    let mut tech_notations = Vec::new();

    for token in input.split_whitespace() {
        if KNOWN_TECH_LIST.contains(&token) {
            tech_notations.push(TechNotation(token.to_owned()));
        } else {
            if step_artist.is_empty() {
                step_artist.push_str(token);
            } else {
                step_artist.push(' ');
                step_artist.push_str(token);
            }
        }
    }

    (step_artist, tech_notations)
}

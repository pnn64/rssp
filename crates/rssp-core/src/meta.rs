/// Normalizes common difficulty labels to a canonical form (e.g. Expert -> Challenge).
pub fn normalize_difficulty_label(raw: &str) -> String {
    old_style_difficulty_label(raw).map_or_else(|| raw.trim().to_string(), str::to_string)
}

fn canonical_difficulty_label(raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("beginner") {
        Some("Beginner")
    } else if raw.eq_ignore_ascii_case("easy") {
        Some("Easy")
    } else if raw.eq_ignore_ascii_case("medium") {
        Some("Medium")
    } else if raw.eq_ignore_ascii_case("hard") {
        Some("Hard")
    } else if raw.eq_ignore_ascii_case("challenge") {
        Some("Challenge")
    } else if raw.eq_ignore_ascii_case("edit") {
        Some("Edit")
    } else {
        None
    }
}

fn old_style_difficulty_label(raw: &str) -> Option<&'static str> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("beginner") {
        Some("Beginner")
    } else if raw.eq_ignore_ascii_case("easy")
        || raw.eq_ignore_ascii_case("basic")
        || raw.eq_ignore_ascii_case("light")
    {
        Some("Easy")
    } else if raw.eq_ignore_ascii_case("medium")
        || raw.eq_ignore_ascii_case("another")
        || raw.eq_ignore_ascii_case("trick")
        || raw.eq_ignore_ascii_case("standard")
        || raw.eq_ignore_ascii_case("difficult")
    {
        Some("Medium")
    } else if raw.eq_ignore_ascii_case("hard")
        || raw.eq_ignore_ascii_case("ssr")
        || raw.eq_ignore_ascii_case("maniac")
        || raw.eq_ignore_ascii_case("heavy")
    {
        Some("Hard")
    } else if raw.eq_ignore_ascii_case("challenge")
        || raw.eq_ignore_ascii_case("expert")
        || raw.eq_ignore_ascii_case("oni")
        || raw.eq_ignore_ascii_case("smaniac")
    {
        Some("Challenge")
    } else if raw.eq_ignore_ascii_case("edit") {
        Some("Edit")
    } else {
        None
    }
}

fn parse_meter_for_difficulty(meter_str: &str, is_sm: bool) -> i32 {
    let trimmed = meter_str.trim();
    if is_sm && trimmed.is_empty() {
        return 1;
    }
    trimmed.parse::<i32>().unwrap_or(0)
}

#[must_use]
pub fn resolve_difficulty_label(
    raw_difficulty: &str,
    description: &str,
    meter_str: &str,
    extension: &str,
) -> String {
    // Match ITGmania Steps::TidyUpData fallback when difficulty is invalid.
    let is_sm = extension.eq_ignore_ascii_case("sm");
    let mut difficulty = if is_sm {
        old_style_difficulty_label(raw_difficulty)
    } else {
        canonical_difficulty_label(raw_difficulty)
    };

    if is_sm && difficulty == Some("Hard") {
        let desc = description.trim();
        if desc.eq_ignore_ascii_case("smaniac") || desc.eq_ignore_ascii_case("challenge") {
            difficulty = Some("Challenge");
        }
    }

    if difficulty.is_none() {
        difficulty = canonical_difficulty_label(description);
    }

    if let Some(label) = difficulty {
        return label.to_string();
    }

    let meter = parse_meter_for_difficulty(meter_str, is_sm);
    if meter == 1 {
        "Beginner".to_string()
    } else if meter <= 3 {
        "Easy".to_string()
    } else if meter <= 6 {
        "Medium".to_string()
    } else {
        "Hard".to_string()
    }
}

#[must_use]
pub fn step_type_lanes(step_type: &str) -> usize {
    let s = step_type.trim().as_bytes();
    if s.eq_ignore_ascii_case(b"dance-double") || s.eq_ignore_ascii_case(b"dance_double") {
        8
    } else {
        4
    }
}

#[inline(always)]
const fn trim_ascii_ws(mut s: &[u8]) -> &[u8] {
    while let Some((&b, rest)) = s.split_first() {
        if b.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let Some((&b, rest)) = s.split_last() {
        if b.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

#[inline(always)]
pub const fn supported_stepstype_lanes_bytes(raw: &[u8]) -> Option<usize> {
    let s = trim_ascii_ws(raw);
    if s.eq_ignore_ascii_case(b"dance-single") || s.eq_ignore_ascii_case(b"dance_single") {
        Some(4)
    } else if s.eq_ignore_ascii_case(b"dance-double") || s.eq_ignore_ascii_case(b"dance_double") {
        Some(8)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_difficulty_label, resolve_difficulty_label};

    #[test]
    fn difficulty_labels_keep_aliases() {
        assert_eq!(normalize_difficulty_label(" Expert "), "Challenge");
        assert_eq!(normalize_difficulty_label("LIGHT"), "Easy");
        assert_eq!(normalize_difficulty_label("difficult"), "Medium");
    }

    #[test]
    fn ssc_labels_use_canonical_names() {
        assert_eq!(
            resolve_difficulty_label("challenge", "", "12", "ssc"),
            "Challenge"
        );
        assert_eq!(resolve_difficulty_label("expert", "", "12", "ssc"), "Hard");
    }
}

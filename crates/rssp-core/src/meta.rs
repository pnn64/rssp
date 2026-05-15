/// Normalizes common difficulty labels to a canonical form (e.g. Expert -> Challenge).
pub fn normalize_difficulty_label(raw: &str) -> String {
    old_style_difficulty_label(raw).map_or_else(|| raw.trim().to_string(), str::to_string)
}

fn canonical_difficulty_label(raw: &str) -> Option<&'static str> {
    let lowered = raw.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "beginner" => Some("Beginner"),
        "easy" => Some("Easy"),
        "medium" => Some("Medium"),
        "hard" => Some("Hard"),
        "challenge" => Some("Challenge"),
        "edit" => Some("Edit"),
        _ => None,
    }
}

fn old_style_difficulty_label(raw: &str) -> Option<&'static str> {
    let lowered = raw.trim().to_ascii_lowercase();
    match lowered.as_str() {
        "beginner" => Some("Beginner"),
        "easy" | "basic" | "light" => Some("Easy"),
        "medium" | "another" | "trick" | "standard" | "difficult" => Some("Medium"),
        "hard" | "ssr" | "maniac" | "heavy" => Some("Hard"),
        "challenge" | "expert" | "oni" | "smaniac" => Some("Challenge"),
        "edit" => Some("Edit"),
        _ => None,
    }
}

fn parse_meter_for_difficulty(meter_str: &str, extension: &str) -> i32 {
    let trimmed = meter_str.trim();
    if extension.eq_ignore_ascii_case("sm") && trimmed.is_empty() {
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
    let mut difficulty = if extension.eq_ignore_ascii_case("sm") {
        old_style_difficulty_label(raw_difficulty)
    } else {
        canonical_difficulty_label(raw_difficulty)
    };

    if extension.eq_ignore_ascii_case("sm") && difficulty == Some("Hard") {
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

    let meter = parse_meter_for_difficulty(meter_str, extension);
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

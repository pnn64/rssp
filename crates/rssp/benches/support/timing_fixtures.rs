use std::fmt::Write as _;

pub fn labels(count: usize, kind: &str) -> String {
    let mut text = String::new();
    for index in 0..count {
        if index != 0 {
            text.push(',');
        }
        let beat = if kind == "replace" { 0 } else { index * 4 };
        match kind {
            "repeat" => write!(text, "{beat}=Verse"),
            "long" => write!(text, "{beat}={}", "a".repeat(1024)),
            "replace" | "unique" => write!(text, "{beat}=Label {index}"),
            "mixed" => write!(text, "{beat}=Part {}", index / 8),
            "invalid" => write!(text, "bad=Label {index}"),
            _ => unreachable!("known fixture kind"),
        }
        .expect("String write");
    }
    text
}

pub fn bpms(count: usize) -> Vec<(f32, f32)> {
    (0..count)
        .map(|index| (index as f32 * 4.0, 90.0 + (index % 211) as f32))
        .collect()
}

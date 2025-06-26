use std::io;

pub fn strip_title_tags(mut title: &str) -> String {
    loop {
        let original = title;

        // Trim leading spaces at the start of each iteration
        title = title.trim_start();

        // Remove any leading bracketed tags like `[...]` regardless of content
        if let Some(rest) = title.strip_prefix('[').and_then(|s| s.split_once(']')) {
            title = rest.1.trim_start();
            continue;
        }

        // Remove numerical prefixes like `123- `
        if let Some(pos) = title.find("- ") {
            if title[..pos].chars().all(|c| c.is_ascii_digit() || c == '.') {
                title = &title[pos + 2..].trim_start();
                continue;
            }
        }

        // Exit if no changes were made
        if title == original {
            break;
        }
    }
    title.to_string()
}

pub fn clean_tag(tag: &str) -> String {
    tag.chars()
        .filter(|c| !c.is_control() && *c != '\u{200b}')
        .collect()
}

pub fn unescape_tag(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut chars = tag.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next_c) = chars.next() {
                match next_c {
                    ':' | ';' | '#' | '\\' => out.push(next_c),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn extract_sections<'a>(
    data: &'a [u8],
    file_extension: &str,
) -> io::Result<(
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Vec<(Vec<u8>, Option<Vec<u8>>)>,
)> {
    if !matches!(file_extension.to_lowercase().as_str(), "sm" | "ssc") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    }

    let tags = [
        b"#TITLE:".as_slice(),
        b"#SUBTITLE:".as_slice(),
        b"#ARTIST:".as_slice(),
        b"#TITLETRANSLIT:".as_slice(),
        b"#SUBTITLETRANSLIT:".as_slice(),
        b"#ARTISTTRANSLIT:".as_slice(),
        b"#OFFSET:".as_slice(),
        b"#BPMS:".as_slice(),
    ];

    let mut sections = [None; 8];
    let mut notes_list = Vec::new();
    let mut i = 0;

    let is_ssc = file_extension.eq_ignore_ascii_case("ssc");

    while i < data.len() {
        if let Some(pos) = data[i..].iter().position(|&b| b == b'#') {
            i += pos;
            if let Some((idx, tag)) = tags.iter().enumerate().find(|(_, &tag)| data[i..].starts_with(tag)) {
                sections[idx] = parse_tag(&data[i..], tag.len());
                i += 1;
            } else if is_ssc && data[i..].starts_with(b"#NOTEDATA:") {
                // --- SSC-specific logic ---
                let notedata_start = i;
                let mut notedata_end = notedata_start + 1;
                
                // Find the end of the current #NOTEDATA block
                while notedata_end < data.len() {
                    if data[notedata_end..].starts_with(b"#NOTEDATA:") {
                        break;
                    }
                    notedata_end += 1;
                }
                
                let notedata_slice = &data[notedata_start..notedata_end];
                
                // Parse subtags within the #NOTEDATA block
                let step_type   = parse_subtag(notedata_slice, b"#STEPSTYPE:").unwrap_or_default();
                let description = parse_subtag(notedata_slice, b"#DESCRIPTION:").unwrap_or_default();
                let credit      = parse_subtag(notedata_slice, b"#CREDIT:").unwrap_or_default();
                let difficulty  = parse_subtag(notedata_slice, b"#DIFFICULTY:").unwrap_or_default();
                let meter       = parse_subtag(notedata_slice, b"#METER:").unwrap_or_default();
                let notes       = parse_subtag(notedata_slice, b"#NOTES:").unwrap_or_default();
                let chart_bpms  = parse_subtag(notedata_slice, b"#BPMS:");

                let concatenated = [step_type, description, difficulty, meter, credit, notes].join(&b':');
                notes_list.push((concatenated, chart_bpms));

                i = notedata_end;
            } else if !is_ssc && data[i..].starts_with(b"#NOTES:") {
                // --- SM-specific logic ---
                let notes_start = i + b"#NOTES:".len();
                let notes_end = data[notes_start..]
                    .iter()
                    .position(|&b| b == b';')
                    .map(|e| notes_start + e)
                    .unwrap_or(data.len());
                let notes_data = data[notes_start..notes_end].to_vec();
                notes_list.push((notes_data, None)); // No chart-specific BPMs for .sm
                i = notes_end + 1;
            } else {
                i += 1; // Skip unrecognized tag
            }
        } else {
            break;
        }
    }

    Ok((
        sections[0], sections[1], sections[2], sections[3],
        sections[4], sections[5], sections[6], sections[7],
        notes_list,
    ))
}

fn parse_tag(data: &[u8], tag_len: usize) -> Option<&[u8]> {
    let slice = data.get(tag_len..)?;
    let mut i = 0;
    while i < slice.len() {
        if slice[i] == b';' {
            // Count preceding backslashes to determine if this semicolon is escaped
            let mut bs_count = 0;
            let mut j = i;
            while j > 0 && slice[j - 1] == b'\\' {
                bs_count += 1;
                j -= 1;
            }
            if bs_count % 2 == 0 {
                return Some(&slice[..i]);
            }
        }
        i += 1;
    }
    None
}

fn parse_subtag(data: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
    data.windows(tag.len())
        .position(|w| w == tag)
        .and_then(|pos| parse_tag(&data[pos + tag.len()..], 0))
        .map(|content| content.to_vec())
}

pub fn split_notes_fields(notes_block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < notes_block.len() && fields.len() < 5 {
        if notes_block[i] == b':' {
            let mut bs_count = 0;
            let mut j = i;
            while j > 0 && notes_block[j - 1] == b'\\' {
                bs_count += 1;
                j -= 1;
            }
            if bs_count % 2 == 0 {
                fields.push(&notes_block[start..i]);
                start = i + 1;
            }
        }
        i += 1;
    }
    let rest = if start <= notes_block.len() { &notes_block[start..] } else { &[] };
    (fields, rest)
}

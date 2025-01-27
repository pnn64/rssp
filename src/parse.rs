use std::io;

pub fn strip_title_tags(title: &str) -> String {
    let mut s = title.trim_start();

    loop {
        // If it starts with [something], check if "something" is purely digits or '.' 
        // If so, remove that bracketed portion
        if s.starts_with('[') {
            if let Some(end_bracket) = s.find(']') {
                let tag_content = &s[1..end_bracket];
                if tag_content.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    s = s[end_bracket + 1..].trim_start();
                    continue;
                }
            }
        } else {
            // Also strip leading numeric prefixes like "8. - "
            let chars = s.char_indices();
            let mut pos = 0;
            for (i, c) in chars {
                if c.is_ascii_digit() || c == '.' {
                    pos = i + c.len_utf8();
                } else {
                    break;
                }
            }
            if pos > 0 && s[pos..].starts_with("- ") {
                s = s[pos + 2..].trim_start();
                continue;
            }
        }
        break;
    }

    s.to_string()
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
    Option<Vec<u8>>,
)> {
    match file_extension.to_lowercase().as_str() {
        "ssc" => parse_ssc_sections(data),
        "sm"  => parse_sm_sections(data),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported file extension. Please provide a '.ssc' or '.sm' file.",
            ))
        }
    }
}

fn parse_sm_sections<'a>(
    data: &'a [u8],
) -> io::Result<(
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<Vec<u8>>,
)> {
    let mut title = None;
    let mut subtitle = None;
    let mut artist = None;
    let mut titletranslit = None;
    let mut subtitletranslit = None;
    let mut artisttranslit = None;
    let mut offset = None;
    let mut bpms = None;

    let mut notes_data: Option<Vec<u8>> = None;

    let mut i = 0;
    while i < data.len() {
        if title.is_some()
            && subtitle.is_some()
            && artist.is_some()
            && bpms.is_some()
            && notes_data.is_some()
        {
            break;
        }

        let slice = &data[i..];
        fn parse_tag<'b>(data: &'b [u8], idx: &mut usize, tag_len: usize) -> Option<&'b [u8]> {
            let start_idx = *idx + tag_len;
            if start_idx > data.len() {
                return None;
            }
            if let Some(end_off) = data[start_idx..].iter().position(|&b| b == b';') {
                let result = &data[start_idx..start_idx + end_off];
                *idx = start_idx + end_off + 1; // move past the semicolon
                Some(result)
            } else {
                None
            }
        }

        if slice.starts_with(b"#TITLE:") && title.is_none() {
            title = parse_tag(data, &mut i, b"#TITLE:".len());
            continue;
        } else if slice.starts_with(b"#SUBTITLE:") && subtitle.is_none() {
            subtitle = parse_tag(data, &mut i, b"#SUBTITLE:".len());
            continue;
        } else if slice.starts_with(b"#ARTIST:") && artist.is_none() {
            artist = parse_tag(data, &mut i, b"#ARTIST:".len());
            continue;
        } else if slice.starts_with(b"#TITLETRANSLIT:") && titletranslit.is_none() {
            titletranslit = parse_tag(data, &mut i, b"#TITLETRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#SUBTITLETRANSLIT:") && subtitletranslit.is_none() {
            subtitletranslit = parse_tag(data, &mut i, b"#SUBTITLETRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#ARTISTTRANSLIT:") && artisttranslit.is_none() {
            artisttranslit = parse_tag(data, &mut i, b"#ARTISTTRANSLIT:".len());
            continue;
        } else if slice.starts_with(b"#OFFSET:") && offset.is_none() {
            offset = parse_tag(data, &mut i, b"#OFFSET:".len());
            continue;
        } else if slice.starts_with(b"#BPMS:") && bpms.is_none() {
            bpms = parse_tag(data, &mut i, b"#BPMS:".len());
            continue;
        } else if slice.starts_with(b"#NOTES:") && notes_data.is_none() {
            let start_idx = i + b"#NOTES:".len();
            if start_idx < data.len() {
                notes_data = Some(data[start_idx..].to_vec());
            }
            break;
        }

        i += 1;
    }

    Ok((
        title,
        subtitle,
        artist,
        titletranslit,
        subtitletranslit,
        artisttranslit,
        offset,
        bpms,
        notes_data,
    ))
}

fn parse_ssc_sections<'a>(
    data: &'a [u8],
) -> io::Result<(
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<&'a [u8]>,
    Option<Vec<u8>>,
)> {
    if !data.windows(b"#NOTEDATA:".len()).any(|w| w == b"#NOTEDATA:") {
        return Err(io::Error::new(io::ErrorKind::Other, "No #NOTEDATA: found in .ssc"));
    }

    fn find_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        let needle_lower = needle.to_ascii_lowercase();
        haystack
            .windows(needle.len())
            .position(|window| window.to_ascii_lowercase() == needle_lower)
    }

    fn parse_tag_value<'b>(haystack: &'b [u8], tag: &[u8]) -> Option<&'b [u8]> {
        if let Some(pos) = find_ci(haystack, tag) {
            let start = pos + tag.len();
            if start < haystack.len() {
                if let Some(end) = haystack[start..].iter().position(|&b| b == b';') {
                    return Some(&haystack[start..start + end]);
                }
            }
        }
        None
    }

    let title            = parse_tag_value(data, b"#TITLE:");
    let subtitle         = parse_tag_value(data, b"#SUBTITLE:");
    let artist           = parse_tag_value(data, b"#ARTIST:");
    let titletranslit    = parse_tag_value(data, b"#TITLETRANSLIT:");
    let subtitletranslit = parse_tag_value(data, b"#SUBTITLETRANSLIT:");
    let artisttranslit   = parse_tag_value(data, b"#ARTISTTRANSLIT:");
    let offset           = parse_tag_value(data, b"#OFFSET:");
    let bpms             = parse_tag_value(data, b"#BPMS:");

    let nd_pos = find_ci(data, b"#NOTEDATA:").ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "Could not find #NOTEDATA: block")
    })?;
    let notedata_slice = &data[nd_pos..];

    let step_type   = parse_tag_value(notedata_slice, b"#STEPSTYPE:").unwrap_or(b"");
    let description = parse_tag_value(notedata_slice, b"#DESCRIPTION:").unwrap_or(b"");
    let difficulty  = parse_tag_value(notedata_slice, b"#DIFFICULTY:").unwrap_or(b"");
    let meter       = parse_tag_value(notedata_slice, b"#METER:").unwrap_or(b"");
    let radarvals   = parse_tag_value(notedata_slice, b"#RADARVALUES:").unwrap_or(b"");
    let notes       = parse_tag_value(notedata_slice, b"#NOTES:")
        .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "No #NOTES: in #NOTEDATA block"))?;

    let mut combined = Vec::new();
    combined.extend_from_slice(step_type);
    combined.push(b':');
    combined.extend_from_slice(description);
    combined.push(b':');
    combined.extend_from_slice(difficulty);
    combined.push(b':');
    combined.extend_from_slice(meter);
    combined.push(b':');
    combined.extend_from_slice(radarvals);
    combined.push(b':');
    combined.extend_from_slice(notes);

    Ok((
        title,
        subtitle,
        artist,
        titletranslit,
        subtitletranslit,
        artisttranslit,
        offset,
        bpms,
        Some(combined),
    ))
}

pub fn split_notes_fields(notes_block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut fields = Vec::with_capacity(5);
    let mut colon_count = 0;
    let mut start = 0;
    for (i, &b) in notes_block.iter().enumerate() {
        if b == b':' {
            fields.push(&notes_block[start..i]);
            start = i + 1;
            colon_count += 1;
            if colon_count == 5 {
                let remainder = &notes_block[start..];
                return (fields, remainder);
            }
        }
    }

    (fields, &notes_block[notes_block.len()..])
}

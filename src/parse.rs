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

#[inline]
fn parse_tag<'a>(data: &'a [u8], idx: &mut usize, tag_len: usize) -> Option<&'a [u8]> {
    let start_idx = *idx + tag_len;
    if start_idx >= data.len() {
        return None;
    }
    // We read until the next semicolon.
    if let Some(end_off) = data[start_idx..].iter().position(|&b| b == b';') {
        let result = &data[start_idx..start_idx + end_off];
        // Move `i` past that semicolon
        *idx = start_idx + end_off + 1;
        Some(result)
    } else {
        None
    }
}

pub fn extract_sections<'a>(
    data: &'a [u8],
    file_extension: &str,
) -> io::Result<(
    Option<&'a [u8]>,  // title
    Option<&'a [u8]>,  // subtitle
    Option<&'a [u8]>,  // artist
    Option<&'a [u8]>,  // titletranslit
    Option<&'a [u8]>,  // subtitletranslit
    Option<&'a [u8]>,  // artisttranslit
    Option<&'a [u8]>,  // offset
    Option<&'a [u8]>,  // bpms
    Option<Vec<u8>>,   // the final "notes data" block
)> {
    let mut title            = None;
    let mut subtitle         = None;
    let mut artist           = None;
    let mut titletranslit    = None;
    let mut subtitletranslit = None;
    let mut artisttranslit   = None;
    let mut offset           = None;
    let mut bpms             = None;

    let mut notes_data       = None; // for .sm or for the final combined block in .ssc

    let is_ssc = matches!(file_extension.to_lowercase().as_str(), "ssc");
    let is_sm  = matches!(file_extension.to_lowercase().as_str(), "sm");

    if !is_sm && !is_ssc {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    }

    let mut i = 0;
    while i < data.len() {
        let slice = &data[i..];

        // --- Parse global tags ---
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
        }

        // --- If it's an .sm file, we look for `#NOTES:` directly ---
        if is_sm && slice.starts_with(b"#NOTES:") && notes_data.is_none() {
            let start_idx = i + b"#NOTES:".len();
            if start_idx < data.len() {
                // Just store the remainder in a Vec<u8>
                notes_data = Some(data[start_idx..].to_vec());
            }
            break; // done scanning
        }

        // --- If it's an .ssc file, we look for `#NOTEDATA:` blocks ---
        if is_ssc && slice.starts_with(b"#NOTEDATA:") && notes_data.is_none() {
            i += b"#NOTEDATA:".len(); // move past "#NOTEDATA:"
            
            let step_type    = parse_subtag(data, &mut i, b"#STEPSTYPE:");
            let description  = parse_subtag(data, &mut i, b"#DESCRIPTION:");
            let difficulty   = parse_subtag(data, &mut i, b"#DIFFICULTY:");
            let meter        = parse_subtag(data, &mut i, b"#METER:");
            let radar_values = parse_subtag(data, &mut i, b"#RADARVALUES:");
            let notes_raw    = parse_subtag(data, &mut i, b"#NOTES:"); 

            let mut combined = Vec::new();
            combined.extend_from_slice(step_type);
            combined.push(b':');
            combined.extend_from_slice(description);
            combined.push(b':');
            combined.extend_from_slice(difficulty);
            combined.push(b':');
            combined.extend_from_slice(meter);
            combined.push(b':');
            combined.extend_from_slice(radar_values);
            combined.push(b':');
            combined.extend_from_slice(notes_raw);

            notes_data = Some(combined);
            break;
        }

        i += 1; // advance and continue searching
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

fn parse_subtag<'a>(data: &'a [u8], idx: &mut usize, tag: &[u8]) -> &'a [u8] {
    let upper_bound = data.len().saturating_sub(tag.len());
    let mut search_pos = *idx;

    while search_pos < upper_bound {
        let slice = &data[search_pos..];
        if slice.starts_with(tag) {
            let val_start = search_pos + tag.len();
            if val_start < data.len() {
                if let Some(end_off) = data[val_start..].iter().position(|&b| b == b';') {
                    let result = &data[val_start..val_start + end_off];
                    *idx = val_start + end_off + 1;
                    return result;
                }
            }
            *idx = data.len();
            return &data[val_start..];
        }
        search_pos += 1;
    }

    &[]
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

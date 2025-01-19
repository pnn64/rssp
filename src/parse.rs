use std::io;

pub fn strip_title_tags(title: &str) -> String {
    let mut s = title.trim_start();

    loop {
        if s.starts_with('[') {
            if let Some(end_bracket) = s.find(']') {
                let tag_content = &s[1..end_bracket];
                // Only strip if the bracket contents are digits or periods (e.g. [16], [300], [2.5])
                if tag_content.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    // Advance past the bracket and trim again
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

pub fn extract_sections(
    data: &[u8],
) -> io::Result<(
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
    Option<&[u8]>,
)> {
    let mut title = None;
    let mut subtitle = None;
    let mut artist = None;
    let mut titletranslit = None;
    let mut subtitletranslit = None;
    let mut artisttranslit = None;
    let mut bpms = None;
    let mut notes = None;

    let mut i = 0;
    while i < data.len() {
        if title.is_some()
            && subtitle.is_some()
            && artist.is_some()
            && bpms.is_some()
            && notes.is_some()
        {
            break;
        }

        #[inline]
        fn parse_tag<'a>(data: &'a [u8], idx: &mut usize, tag_len: usize) -> Option<&'a [u8]> {
            let start_idx = *idx + tag_len;
            if start_idx > data.len() {
                return None;
            }
            if let Some(end_off) = data[start_idx..].iter().position(|&b| b == b';') {
                let result = &data[start_idx..start_idx + end_off];
                *idx = start_idx + end_off + 1;
                Some(result)
            } else {
                None
            }
        }

        let slice = &data[i..];
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
        } else if slice.starts_with(b"#BPMS:") && bpms.is_none() {
            bpms = parse_tag(data, &mut i, b"#BPMS:".len());
            continue;
        } else if slice.starts_with(b"#NOTES:") && notes.is_none() {
            let start_idx = i + b"#NOTES:".len();
            if start_idx < data.len() {
                notes = Some(&data[start_idx..]);
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
        bpms,
        notes,
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

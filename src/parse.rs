use std::io;

pub fn strip_title_tags(title: &str) -> String {
    let mut s = title.trim_start();

    loop {
        let mut modified = false;

        // Check for bracketed numerical tags
        if let Some(start) = s.strip_prefix('[') {
            if let Some(end) = start.find(']') {
                let tag_content = &start[..end];
                if tag_content.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    s = start[end + 1..].trim_start();
                    modified = true;
                }
            }
        }

        // Check for numerical prefix with "- " suffix
        if !modified {
            let bytes = s.as_bytes();
            let pos = bytes
                .iter()
                .take_while(|&&b| b.is_ascii_digit() || b == b'.')
                .count();

            if pos > 0 && bytes.get(pos..).map_or(false, |s| s.starts_with(b"- ")) {
                s = &s[pos + 2..].trim_start();
                modified = true;
            }
        }

        if !modified {
            break;
        }
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
    let is_ssc = file_extension.eq_ignore_ascii_case("ssc");
    let is_sm = file_extension.eq_ignore_ascii_case("sm");

    if !is_sm && !is_ssc {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    }

    let tag_list = [
        (b"#TITLE:".as_slice(), 0),
        (b"#SUBTITLE:".as_slice(), 1),
        (b"#ARTIST:".as_slice(), 2),
        (b"#TITLETRANSLIT:".as_slice(), 3),
        (b"#SUBTITLETRANSLIT:".as_slice(), 4),
        (b"#ARTISTTRANSLIT:".as_slice(), 5),
        (b"#OFFSET:".as_slice(), 6),
        (b"#BPMS:".as_slice(), 7),
    ];

    let mut sections: [Option<&[u8]>; 8] = [None; 8];
    let mut notes_data = None;
    let mut i = 0;

    while i < data.len() {
        if let Some(pos) = data[i..].iter().position(|&b| b == b'#') {
            i += pos;
            
            // Check tags
            for (tag, idx) in &tag_list {
                if data.len() - i >= tag.len() && &data[i..i+tag.len()] == *tag {
                    if let Some(content) = parse_tag(data, &mut i, tag.len()) {
                        sections[*idx] = Some(content);
                    }
                    break;
                }
            }

            // Handle notes section
            if notes_data.is_none() {
                if is_sm && data[i..].starts_with(b"#NOTES:") {
                    let start = i + b"#NOTES:".len();
                    notes_data = Some(data[start..].to_vec());
                    break;
                } else if is_ssc && data[i..].starts_with(b"#NOTEDATA:") {
                    notes_data = Some(process_ssc_notedata(data, &mut i));
                    break;
                }
            }
        }
        i += 1;
    }

    Ok((
        sections[0], sections[1], sections[2], sections[3],
        sections[4], sections[5], sections[6], sections[7],
        notes_data,
    ))
}

fn process_ssc_notedata(data: &[u8], idx: &mut usize) -> Vec<u8> {
    *idx += b"#NOTEDATA:".len();
    let tags = [
        b"#STEPSTYPE:".as_slice(),
        b"#DESCRIPTION:".as_slice(),
        b"#DIFFICULTY:".as_slice(),
        b"#METER:".as_slice(),
        b"#RADARVALUES:".as_slice(),
        b"#NOTES:".as_slice(),
    ];

    let parts: Vec<_> = tags.iter().map(|tag| {
        let mut pos = *idx;
        let val = parse_subtag(data, &mut pos, tag);
        *idx = pos;
        val
    }).collect();

    parts.join(&b':')
}

fn parse_tag<'a>(data: &'a [u8], idx: &mut usize, tag_len: usize) -> Option<&'a [u8]> {
    let start = *idx + tag_len;
    data.get(start..).and_then(|d| {
        d.iter().position(|&b| b == b';').map(|end| {
            *idx = start + end + 1;
            &data[start..start+end]
        })
    })
}

fn parse_subtag<'a>(data: &'a [u8], idx: &mut usize, tag: &[u8]) -> &'a [u8] {
    data[*idx..].windows(tag.len()).position(|w| w == tag)
        .and_then(|pos| {
            *idx += pos + tag.len();
            parse_tag(data, idx, 0)
        })
        .unwrap_or_default()
}

pub fn split_notes_fields(notes_block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut parts = notes_block.splitn(6, |&b| b == b':');
    let fields: Vec<_> = (0..5).filter_map(|_| parts.next()).collect();
    (fields, parts.next().unwrap_or_default())
}

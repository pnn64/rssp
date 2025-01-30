use std::io;

pub fn strip_title_tags(mut title: &str) -> String {
    loop {
        let original = title;
        // Remove bracketed numerical tags like `[123]`
        if let Some(rest) = title.strip_prefix('[').and_then(|s| s.split_once(']')) {
            if rest.0.chars().all(|c| c.is_ascii_digit() || c == '.') {
                title = rest.1.trim_start();
                continue;
            }
        }
        // Remove numerical prefixes like `123- `
        if let Some(pos) = title.find("- ") {
            if title[..pos].chars().all(|c| c.is_ascii_digit() || c == '.') {
                title = &title[pos + 2..].trim_start();
                continue;
            }
        }
        if title == original {
            break;
        }
    }
    title.to_string()
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
    let mut notes_data = None;
    let mut i = 0;

    while i < data.len() {
        if let Some(pos) = data[i..].iter().position(|&b| b == b'#') {
            i += pos;
            if let Some((idx, tag)) = tags.iter().enumerate().find(|(_, &tag)| data[i..].starts_with(tag)) {
                sections[idx] = parse_tag(&data[i..], tag.len());
            } else if data[i..].starts_with(b"#NOTES:") {
                notes_data = Some(data[i + b"#NOTES:".len()..].to_vec());
                break;
            } else if data[i..].starts_with(b"#NOTEDATA:") {
                notes_data = Some(process_ssc_notedata(&data[i..]));
                break;
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

fn process_ssc_notedata(data: &[u8]) -> Vec<u8> {
    let tags = [
        b"#STEPSTYPE:".as_slice(),
        b"#DESCRIPTION:".as_slice(),
        b"#DIFFICULTY:".as_slice(),
        b"#METER:".as_slice(),
        b"#RADARVALUES:".as_slice(),
        b"#NOTES:".as_slice(),
    ];

    tags.iter()
        .filter_map(|&tag| parse_subtag(data, tag))
        .collect::<Vec<_>>()
        .join(&b':')
}

fn parse_tag(data: &[u8], tag_len: usize) -> Option<&[u8]> {
    data.get(tag_len..)
        .and_then(|d| d.iter().position(|&b| b == b';').map(|end| &d[..end]))
}

fn parse_subtag(data: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
    data.windows(tag.len())
        .position(|w| w == tag)
        .and_then(|pos| parse_tag(&data[pos + tag.len()..], 0))
        .map(|content| content.to_vec())
}

pub fn split_notes_fields(notes_block: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut parts = notes_block.splitn(6, |&b| b == b':');
    let fields: Vec<_> = parts.by_ref().take(5).collect();
    (fields, parts.next().unwrap_or(&[]))
}

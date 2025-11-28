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

/// Parsed note data for a single chart found in the simfile.
#[derive(Default)]
pub struct ParsedChartEntry {
    pub notes: Vec<u8>,
    pub chart_bpms: Option<Vec<u8>>,
    pub chart_stops: Option<Vec<u8>>,
    pub chart_delays: Option<Vec<u8>>,
    pub chart_warps: Option<Vec<u8>>,
    pub chart_speeds: Option<Vec<u8>>,
    pub chart_scrolls: Option<Vec<u8>>,
    pub chart_fakes: Option<Vec<u8>>,
}

/// A struct to hold the raw data parsed from a simfile's header tags.
#[derive(Default)]
pub struct ParsedSimfileData<'a> {
    pub title: Option<&'a [u8]>,
    pub subtitle: Option<&'a [u8]>,
    pub artist: Option<&'a [u8]>,
    pub title_translit: Option<&'a [u8]>,
    pub subtitle_translit: Option<&'a [u8]>,
    pub artist_translit: Option<&'a [u8]>,
    pub offset: Option<&'a [u8]>,
    pub bpms: Option<&'a [u8]>,
    pub stops: Option<&'a [u8]>,
    pub delays: Option<&'a [u8]>,
    pub warps: Option<&'a [u8]>,
    pub speeds: Option<&'a [u8]>,
    pub scrolls: Option<&'a [u8]>,
    pub fakes: Option<&'a [u8]>,
    pub banner: Option<&'a [u8]>,
    pub background: Option<&'a [u8]>,
    pub music: Option<&'a [u8]>,
    pub sample_start: Option<&'a [u8]>,
    pub sample_length: Option<&'a [u8]>,
    pub display_bpm: Option<&'a [u8]>,
    pub notes_list: Vec<ParsedChartEntry>,
}

pub fn extract_sections<'a>(
    data: &'a [u8],
    file_extension: &str,
) -> io::Result<ParsedSimfileData<'a>> {
    if !matches!(file_extension.to_lowercase().as_str(), "sm" | "ssc") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unsupported file extension (must be .sm or .ssc)",
        ));
    }

    let mut result = ParsedSimfileData::default();
    let mut i = 0;
    let is_ssc = file_extension.eq_ignore_ascii_case("ssc");

    while i < data.len() {
        if let Some(pos) = data[i..].iter().position(|&b| b == b'#') {
            i += pos;
            let current_slice = &data[i..];

            if current_slice.starts_with(b"#TITLE:") {
                result.title = parse_tag(current_slice, b"#TITLE:".len());
            } else if current_slice.starts_with(b"#SUBTITLE:") {
                result.subtitle = parse_tag(current_slice, b"#SUBTITLE:".len());
            } else if current_slice.starts_with(b"#ARTIST:") {
                result.artist = parse_tag(current_slice, b"#ARTIST:".len());
            } else if current_slice.starts_with(b"#TITLETRANSLIT:") {
                result.title_translit = parse_tag(current_slice, b"#TITLETRANSLIT:".len());
            } else if current_slice.starts_with(b"#SUBTITLETRANSLIT:") {
                result.subtitle_translit = parse_tag(current_slice, b"#SUBTITLETRANSLIT:".len());
            } else if current_slice.starts_with(b"#ARTISTTRANSLIT:") {
                result.artist_translit = parse_tag(current_slice, b"#ARTISTTRANSLIT:".len());
            } else if current_slice.starts_with(b"#OFFSET:") {
                result.offset = parse_tag(current_slice, b"#OFFSET:".len());
            } else if current_slice.starts_with(b"#BPMS:") {
                result.bpms = parse_tag(current_slice, b"#BPMS:".len());
            } else if current_slice.starts_with(b"#STOPS:") {
                result.stops = parse_tag(current_slice, b"#STOPS:".len());
            } else if current_slice.starts_with(b"#FREEZES:") {
                // Older charts sometimes use #FREEZES instead of #STOPS.
                result.stops = parse_tag(current_slice, b"#FREEZES:".len());
            } else if current_slice.starts_with(b"#FAKES:") {
                result.fakes = parse_tag(current_slice, b"#FAKES:".len());
            } else if current_slice.starts_with(b"#DELAYS:") {
                result.delays = parse_tag(current_slice, b"#DELAYS:".len());
            } else if current_slice.starts_with(b"#WARPS:") {
                result.warps = parse_tag(current_slice, b"#WARPS:".len());
            } else if current_slice.starts_with(b"#SPEEDS:") {
                result.speeds = parse_tag(current_slice, b"#SPEEDS:".len());
            } else if current_slice.starts_with(b"#SCROLLS:") {
                result.scrolls = parse_tag(current_slice, b"#SCROLLS:".len());
            } else if current_slice.starts_with(b"#BANNER:") {
                result.banner = parse_tag(current_slice, b"#BANNER:".len());
            } else if current_slice.starts_with(b"#BACKGROUND:") {
                result.background = parse_tag(current_slice, b"#BACKGROUND:".len());
            } else if current_slice.starts_with(b"#MUSIC:") {
                result.music = parse_tag(current_slice, b"#MUSIC:".len());
            } else if current_slice.starts_with(b"#SAMPLESTART:") {
                result.sample_start = parse_tag(current_slice, b"#SAMPLESTART:".len());
            } else if current_slice.starts_with(b"#SAMPLELENGTH:") {
                result.sample_length = parse_tag(current_slice, b"#SAMPLELENGTH:".len());
            } else if current_slice.starts_with(b"#DISPLAYBPM:") {
                result.display_bpm = parse_tag(current_slice, b"#DISPLAYBPM:".len());    
            } else if is_ssc && current_slice.starts_with(b"#NOTEDATA:") {
                let notedata_start = i;
                let mut notedata_end = notedata_start + 1;
                while notedata_end < data.len() && !data[notedata_end..].starts_with(b"#NOTEDATA:")
                {
                    notedata_end += 1;
                }

                let notedata_slice = &data[notedata_start..notedata_end];
                let step_type = parse_subtag(notedata_slice, b"#STEPSTYPE:").unwrap_or_default();
                let description =
                    parse_subtag(notedata_slice, b"#DESCRIPTION:").unwrap_or_default();
                let credit = parse_subtag(notedata_slice, b"#CREDIT:").unwrap_or_default();
                let difficulty = parse_subtag(notedata_slice, b"#DIFFICULTY:").unwrap_or_default();
                let meter = parse_subtag(notedata_slice, b"#METER:").unwrap_or_default();
                let notes = parse_subtag(notedata_slice, b"#NOTES:").unwrap_or_default();
                let chart_bpms = parse_subtag(notedata_slice, b"#BPMS:");
                let chart_stops = parse_subtag(notedata_slice, b"#STOPS:")
                    .or_else(|| parse_subtag(notedata_slice, b"#FREEZES:"));
                let chart_delays = parse_subtag(notedata_slice, b"#DELAYS:");
                let chart_warps = parse_subtag(notedata_slice, b"#WARPS:");
                let chart_speeds = parse_subtag(notedata_slice, b"#SPEEDS:");
                let chart_scrolls = parse_subtag(notedata_slice, b"#SCROLLS:");
                let chart_fakes = parse_subtag(notedata_slice, b"#FAKES:");

                let concatenated =
                    [step_type, description, difficulty, meter, credit, notes].join(&b':');
                result.notes_list.push(ParsedChartEntry {
                    notes: concatenated,
                    chart_bpms,
                    chart_stops,
                    chart_delays,
                    chart_warps,
                    chart_speeds,
                    chart_scrolls,
                    chart_fakes,
                });

                i = notedata_end;
                continue; // Skip the i += 1 at the end
            } else if !is_ssc && current_slice.starts_with(b"#NOTES:") {
                let notes_start = i + b"#NOTES:".len();
                let notes_end = data[notes_start..]
                    .iter()
                    .position(|&b| b == b';')
                    .map(|e| notes_start + e)
                    .unwrap_or(data.len());
                let block = data[notes_start..notes_end].to_vec();
                let chart_fakes = parse_subtag(&block, b"#FAKES:");
                result.notes_list.push(ParsedChartEntry {
                    notes: block,
                    chart_bpms: None,
                    chart_stops: None,
                    chart_delays: None,
                    chart_warps: None,
                    chart_speeds: None,
                    chart_scrolls: None,
                    chart_fakes,
                });
                i = notes_end + 1;
                continue; // Skip the i += 1 at the end
            }
            i += 1; // Move past the '#'
        } else {
            break; // No more '#' found
        }
    }

    Ok(result)
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
    let rest = if start <= notes_block.len() {
        &notes_block[start..]
    } else {
        &[]
    };
    (fields, rest)
}

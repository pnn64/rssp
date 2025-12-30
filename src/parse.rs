use std::borrow::Cow;
use std::io;

use crate::timing::{TimingFormat, STEPFILE_VERSION_NUMBER};

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
        .filter(|c| !c.is_control())
        .collect()
}

pub fn unescape_tag(tag: &str) -> String {
    let mut out = String::with_capacity(tag.len());
    let mut chars = tag.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next_c) = chars.next() {
                out.push(next_c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn unescape_trim(tag: &str) -> String {
    let mut out = unescape_tag(tag);
    let trimmed = out.trim();
    if trimmed.len() != out.len() {
        out = trimmed.to_string();
    }
    out
}

const CP1252_MAP: [u16; 32] = [
    0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021,
    0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD,
    0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
];

fn decode_cp1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        let ch = match b {
            0x00..=0x7F => b as char,
            0x80..=0x9F => char::from_u32(CP1252_MAP[(b - 0x80) as usize] as u32)
                .unwrap_or('\u{FFFD}'),
            _ => char::from_u32(b as u32).unwrap_or('\u{FFFD}'),
        };
        out.push(ch);
    }
    out
}

pub fn decode_bytes(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => Cow::Owned(decode_cp1252(bytes)),
    }
}

pub fn parse_offset_seconds(parsed_offset: Option<&[u8]>) -> f64 {
    parsed_offset
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as f32 as f64)
        .unwrap_or(0.0)
}

pub fn parse_version(parsed_version: Option<&[u8]>, timing_format: TimingFormat) -> f32 {
    parsed_version
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or_else(|| {
            if timing_format == TimingFormat::Ssc {
                0.0
            } else {
                STEPFILE_VERSION_NUMBER
            }
        })
}

pub const SSC_VERSION_CHART_NAME_TAG: f32 = 0.74;

pub fn normalize_chart_desc(desc: String, timing_format: TimingFormat, ssc_version: f32) -> String {
    if timing_format == TimingFormat::Ssc && ssc_version < SSC_VERSION_CHART_NAME_TAG {
        String::new()
    } else {
        desc
    }
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
    pub chart_offset: Option<Vec<u8>>,
    pub chart_time_signatures: Option<Vec<u8>>,
    pub chart_labels: Option<Vec<u8>>,
    pub chart_tickcounts: Option<Vec<u8>>,
    pub chart_combos: Option<Vec<u8>>,
    pub chart_radar_values: Option<Vec<u8>>,
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
    pub version: Option<&'a [u8]>,
    pub offset: Option<&'a [u8]>,
    pub bpms: Option<&'a [u8]>,
    pub stops: Option<&'a [u8]>,
    pub delays: Option<&'a [u8]>,
    pub warps: Option<&'a [u8]>,
    pub speeds: Option<&'a [u8]>,
    pub scrolls: Option<&'a [u8]>,
    pub fakes: Option<&'a [u8]>,
    pub time_signatures: Option<&'a [u8]>,
    pub labels: Option<&'a [u8]>,
    pub tickcounts: Option<&'a [u8]>,
    pub combos: Option<&'a [u8]>,
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
            } else if current_slice.starts_with(b"#VERSION:") {
                result.version = parse_tag(current_slice, b"#VERSION:".len());
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
            } else if current_slice.starts_with(b"#TIMESIGNATURES:") {
                result.time_signatures = parse_tag(current_slice, b"#TIMESIGNATURES:".len());
            } else if current_slice.starts_with(b"#LABELS:") {
                result.labels = parse_tag(current_slice, b"#LABELS:".len());
            } else if current_slice.starts_with(b"#TICKCOUNTS:") {
                result.tickcounts = parse_tag(current_slice, b"#TICKCOUNTS:".len());
            } else if current_slice.starts_with(b"#COMBOS:") {
                result.combos = parse_tag(current_slice, b"#COMBOS:".len());
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
                let step_type =
                    parse_subtag(notedata_slice, b"#STEPSTYPE:", false).unwrap_or_default();
                let description =
                    parse_subtag(notedata_slice, b"#DESCRIPTION:", false).unwrap_or_default();
                let credit = parse_subtag(notedata_slice, b"#CREDIT:", false).unwrap_or_default();
                let difficulty =
                    parse_subtag(notedata_slice, b"#DIFFICULTY:", false).unwrap_or_default();
                let meter = parse_subtag(notedata_slice, b"#METER:", false).unwrap_or_default();
                let notes = parse_subtag(notedata_slice, b"#NOTES:", true)
                    .or_else(|| parse_subtag(notedata_slice, b"#NOTES2:", true))
                    .unwrap_or_default();
                let chart_bpms = parse_subtag(notedata_slice, b"#BPMS:", true);
                let chart_stops = parse_subtag(notedata_slice, b"#STOPS:", true)
                    .or_else(|| parse_subtag(notedata_slice, b"#FREEZES:", true));
                let chart_delays = parse_subtag(notedata_slice, b"#DELAYS:", true);
                let chart_warps = parse_subtag(notedata_slice, b"#WARPS:", true);
                let chart_speeds = parse_subtag(notedata_slice, b"#SPEEDS:", true);
                let chart_scrolls = parse_subtag(notedata_slice, b"#SCROLLS:", true);
                let chart_fakes = parse_subtag(notedata_slice, b"#FAKES:", true);
                let chart_offset = parse_subtag(notedata_slice, b"#OFFSET:", true);
                let chart_time_signatures =
                    parse_subtag(notedata_slice, b"#TIMESIGNATURES:", true);
                let chart_labels = parse_subtag(notedata_slice, b"#LABELS:", true);
                let chart_tickcounts = parse_subtag(notedata_slice, b"#TICKCOUNTS:", true);
                let chart_combos = parse_subtag(notedata_slice, b"#COMBOS:", true);
                let chart_radar_values = parse_subtag(notedata_slice, b"#RADARVALUES:", true);

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
                    chart_offset,
                    chart_time_signatures,
                    chart_labels,
                    chart_tickcounts,
                    chart_combos,
                    chart_radar_values,
                });

                i = notedata_end;
                continue; // Skip the i += 1 at the end
            } else if !is_ssc
                && (current_slice.starts_with(b"#NOTES:") || current_slice.starts_with(b"#NOTES2:"))
            {
                let notes_tag_len = if current_slice.starts_with(b"#NOTES2:") {
                    b"#NOTES2:".len()
                } else {
                    b"#NOTES:".len()
                };
                let notes_start = i + notes_tag_len;
                let notes_end = data[notes_start..]
                    .iter()
                    .position(|&b| b == b';')
                    .map(|e| notes_start + e)
                    .unwrap_or(data.len());
                let block = data[notes_start..notes_end].to_vec();
                let chart_fakes = parse_subtag(&block, b"#FAKES:", true);
                result.notes_list.push(ParsedChartEntry {
                    notes: block,
                    chart_bpms: None,
                    chart_stops: None,
                    chart_delays: None,
                    chart_warps: None,
                    chart_speeds: None,
                    chart_scrolls: None,
                    chart_fakes,
                    chart_offset: None,
                    chart_time_signatures: None,
                    chart_labels: None,
                    chart_tickcounts: None,
                    chart_combos: None,
                    chart_radar_values: None,
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
        match slice[i] {
            b';' => {
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
            // Fallback for malformed tags missing a terminating semicolon: if the next
            // line starts a new tag (`#...:`), stop at this line break.
            b'\n' | b'\r' => {
                let mut j = i + 1;
                // Handle CRLF.
                if slice[i] == b'\r' && j < slice.len() && slice[j] == b'\n' {
                    j += 1;
                }

                // Skip horizontal whitespace at the start of the next line.
                while j < slice.len() && slice[j].is_ascii_whitespace()
                    && slice[j] != b'\n'
                    && slice[j] != b'\r'
                {
                    j += 1;
                }

                if j < slice.len() && slice[j] == b'#' {
                    return Some(&slice[..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn parse_subtag(data: &[u8], tag: &[u8], allow_newlines: bool) -> Option<Vec<u8>> {
    data.windows(tag.len())
        .position(|w| w == tag)
        .and_then(|pos| {
            let slice = &data[pos + tag.len()..];
            let mut i = 0;
            while i < slice.len() {
                match slice[i] {
                    b';' => {
                        // Count preceding backslashes to determine if this semicolon is escaped
                        let mut bs_count = 0;
                        let mut j = i;
                        while j > 0 && slice[j - 1] == b'\\' {
                            bs_count += 1;
                            j -= 1;
                        }
                        if bs_count % 2 == 0 {
                            return Some(slice[..i].to_vec());
                        }
                    }
                    // Fallback for malformed subtags missing a terminating semicolon: if the next
                    // line starts a new tag (`#...:`), stop at this line break.
                    b'\n' | b'\r' => {
                        let mut j = i + 1;
                        // Handle CRLF.
                        if slice[i] == b'\r' && j < slice.len() && slice[j] == b'\n' {
                            j += 1;
                        }

                        // Skip horizontal whitespace at the start of the next line.
                        while j < slice.len()
                            && slice[j].is_ascii_whitespace()
                            && slice[j] != b'\n'
                            && slice[j] != b'\r'
                        {
                            j += 1;
                        }

                        if j < slice.len() && slice[j] == b'#' {
                            return Some(slice[..i].to_vec());
                        }

                        if !allow_newlines {
                            return None;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            None
        })
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

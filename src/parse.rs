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
    let parsed = parsed_version
        .and_then(|b| std::str::from_utf8(b).ok())
        .and_then(|s| s.parse::<f32>().ok());
    match (parsed, timing_format) {
        (Some(version), _) => version,
        // Missing SSC version disables split timing; NaN keeps < checks false.
        (None, TimingFormat::Ssc) => f32::NAN,
        (None, TimingFormat::Sm) => STEPFILE_VERSION_NUMBER,
    }
}

pub const SSC_VERSION_CHART_NAME_TAG: f32 = 0.74;

pub fn normalize_chart_desc(desc: String, timing_format: TimingFormat, ssc_version: f32) -> String {
    if timing_format == TimingFormat::Ssc && ssc_version < SSC_VERSION_CHART_NAME_TAG {
        String::new()
    } else {
        desc
    }
}

type TagBytes<'a> = Cow<'a, [u8]>;

/// Parsed note data for a single chart found in the simfile.
#[derive(Default)]
pub struct ParsedChartEntry<'a> {
    pub notes: Vec<u8>,
    pub chart_bpms: Option<TagBytes<'a>>,
    pub chart_stops: Option<TagBytes<'a>>,
    pub chart_delays: Option<TagBytes<'a>>,
    pub chart_warps: Option<TagBytes<'a>>,
    pub chart_speeds: Option<TagBytes<'a>>,
    pub chart_scrolls: Option<TagBytes<'a>>,
    pub chart_fakes: Option<TagBytes<'a>>,
    pub chart_offset: Option<TagBytes<'a>>,
    pub chart_display_bpm: Option<TagBytes<'a>>,
    pub chart_time_signatures: Option<TagBytes<'a>>,
    pub chart_labels: Option<TagBytes<'a>>,
    pub chart_tickcounts: Option<TagBytes<'a>>,
    pub chart_combos: Option<TagBytes<'a>>,
    pub chart_radar_values: Option<TagBytes<'a>>,
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
    pub notes_list: Vec<ParsedChartEntry<'a>>,
}

#[derive(Default)]
struct NotedataFields<'a> {
    step_type: Option<&'a [u8]>,
    description: Option<&'a [u8]>,
    credit: Option<&'a [u8]>,
    difficulty: Option<&'a [u8]>,
    meter: Option<&'a [u8]>,
    notes: Option<&'a [u8]>,
    notes2: Option<&'a [u8]>,
    chart_bpms: Option<&'a [u8]>,
    chart_stops: Option<&'a [u8]>,
    chart_freezes: Option<&'a [u8]>,
    chart_delays: Option<&'a [u8]>,
    chart_warps: Option<&'a [u8]>,
    chart_speeds: Option<&'a [u8]>,
    chart_scrolls: Option<&'a [u8]>,
    chart_fakes: Option<&'a [u8]>,
    chart_offset: Option<&'a [u8]>,
    chart_display_bpm: Option<&'a [u8]>,
    chart_time_signatures: Option<&'a [u8]>,
    chart_labels: Option<&'a [u8]>,
    chart_tickcounts: Option<&'a [u8]>,
    chart_combos: Option<&'a [u8]>,
    chart_radar_values: Option<&'a [u8]>,
}

#[inline(always)]
fn starts_with_tag_ci(slice: &[u8], tag: &[u8]) -> bool {
    if slice.len() < tag.len() {
        return false;
    }
    let mut i = 0;
    while i < tag.len() {
        if slice[i].to_ascii_lowercase() != tag[i].to_ascii_lowercase() {
            return false;
        }
        i += 1;
    }
    true
}

const TAG_NOTEDATA: &[u8] = b"#NOTEDATA:";
const TAG_STEPSTYPE: &[u8] = b"#STEPSTYPE:";
const TAG_DESCRIPTION: &[u8] = b"#DESCRIPTION:";
const TAG_CREDIT: &[u8] = b"#CREDIT:";
const TAG_DIFFICULTY: &[u8] = b"#DIFFICULTY:";
const TAG_METER: &[u8] = b"#METER:";
const TAG_NOTES: &[u8] = b"#NOTES:";
const TAG_NOTES2: &[u8] = b"#NOTES2:";
const TAG_BPMS: &[u8] = b"#BPMS:";
const TAG_STOPS: &[u8] = b"#STOPS:";
const TAG_FREEZES: &[u8] = b"#FREEZES:";
const TAG_DELAYS: &[u8] = b"#DELAYS:";
const TAG_WARPS: &[u8] = b"#WARPS:";
const TAG_SPEEDS: &[u8] = b"#SPEEDS:";
const TAG_SCROLLS: &[u8] = b"#SCROLLS:";
const TAG_FAKES: &[u8] = b"#FAKES:";
const TAG_OFFSET: &[u8] = b"#OFFSET:";
const TAG_DISPLAYBPM: &[u8] = b"#DISPLAYBPM:";
const TAG_TIMESIGNATURES: &[u8] = b"#TIMESIGNATURES:";
const TAG_LABELS: &[u8] = b"#LABELS:";
const TAG_TICKCOUNTS: &[u8] = b"#TICKCOUNTS:";
const TAG_COMBOS: &[u8] = b"#COMBOS:";
const TAG_RADARVALUES: &[u8] = b"#RADARVALUES:";

#[inline(always)]
fn scan_tag_val(slice: &[u8], allow_newlines: bool) -> Option<(usize, usize)> {
    let mut i = 0usize;
    let mut bs_run = 0usize;
    while i < slice.len() {
        let b = slice[i];
        match b {
            b';' => {
                if bs_run & 1 == 0 {
                    return Some((i, i + 1));
                }
            }
            b':' if !allow_newlines => {
                if bs_run & 1 == 0 {
                    return Some((i, i + 1));
                }
            }
            b'\n' | b'\r' => {
                bs_run = 0;
                let mut j = i + 1;
                if b == b'\r' && j < slice.len() && slice[j] == b'\n' {
                    j += 1;
                }
                while j < slice.len()
                    && slice[j].is_ascii_whitespace()
                    && slice[j] != b'\n'
                    && slice[j] != b'\r'
                {
                    j += 1;
                }
                if j < slice.len() && slice[j] == b'#' {
                    return Some((i, j));
                }
                if !allow_newlines {
                    if j < slice.len() && slice[j] == b';' {
                        // Allow tags that put the terminator on the next line.
                    } else {
                        return None;
                    }
                }
            }
            _ => {}
        }
        if b == b'\\' {
            bs_run += 1;
        } else {
            bs_run = 0;
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn parse_tag_into<'a>(
    slice: &'a [u8],
    tag: &[u8],
    allow_newlines: bool,
    out: &mut Option<&'a [u8]>,
) -> Option<usize> {
    if !starts_with_tag_ci(slice, tag) {
        return None;
    }
    let tag_len = tag.len();
    let Some((end, next)) = scan_tag_val(&slice[tag_len..], allow_newlines) else {
        return None;
    };
    if out.is_none() {
        *out = Some(&slice[tag_len..tag_len + end]);
    }
    Some(tag_len + next)
}

#[inline(always)]
fn join_notes_fields(parts: [&[u8]; 6]) -> Vec<u8> {
    let mut total = 5usize;
    for part in parts.iter() {
        total += part.len();
    }
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(parts[0]);
    for part in &parts[1..] {
        out.push(b':');
        out.extend_from_slice(part);
    }
    out
}

fn parse_notedata_fields<'a>(data: &'a [u8]) -> NotedataFields<'a> {
    let mut out = NotedataFields::default();
    let mut i = 0usize;

    while i < data.len() {
        let Some(pos) = data[i..].iter().position(|&b| b == b'#') else {
            break;
        };
        i += pos;
        let slice = &data[i..];

        if starts_with_tag_ci(slice, TAG_NOTEDATA) {
            let tag_len = TAG_NOTEDATA.len();
            if let Some((_, next)) = scan_tag_val(&slice[tag_len..], true) {
                i += tag_len + next;
                continue;
            }
        }
        if let Some(adv) = parse_tag_into(slice, TAG_STEPSTYPE, false, &mut out.step_type) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_DESCRIPTION, false, &mut out.description) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_CREDIT, false, &mut out.credit) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_DIFFICULTY, false, &mut out.difficulty) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_METER, false, &mut out.meter) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_NOTES, true, &mut out.notes) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_NOTES2, true, &mut out.notes2) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_BPMS, true, &mut out.chart_bpms) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_STOPS, true, &mut out.chart_stops) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_FREEZES, true, &mut out.chart_freezes) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_DELAYS, true, &mut out.chart_delays) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_WARPS, true, &mut out.chart_warps) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_SPEEDS, true, &mut out.chart_speeds) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_SCROLLS, true, &mut out.chart_scrolls) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_FAKES, true, &mut out.chart_fakes) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_OFFSET, true, &mut out.chart_offset) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_DISPLAYBPM, true, &mut out.chart_display_bpm) {
            i += adv;
            continue;
        }
        if let Some(adv) =
            parse_tag_into(slice, TAG_TIMESIGNATURES, true, &mut out.chart_time_signatures)
        {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_LABELS, true, &mut out.chart_labels) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_TICKCOUNTS, true, &mut out.chart_tickcounts) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(slice, TAG_COMBOS, true, &mut out.chart_combos) {
            i += adv;
            continue;
        }
        if let Some(adv) = parse_tag_into(
            slice,
            TAG_RADARVALUES,
            true,
            &mut out.chart_radar_values,
        ) {
            i += adv;
            continue;
        }

        i += 1;
    }

    out
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

            if starts_with_tag_ci(current_slice, b"#TITLE:") {
                result.title = parse_tag(current_slice, b"#TITLE:".len());
            } else if starts_with_tag_ci(current_slice, b"#SUBTITLE:") {
                result.subtitle = parse_tag(current_slice, b"#SUBTITLE:".len());
            } else if starts_with_tag_ci(current_slice, b"#ARTIST:") {
                result.artist = parse_tag(current_slice, b"#ARTIST:".len());
            } else if starts_with_tag_ci(current_slice, b"#TITLETRANSLIT:") {
                result.title_translit = parse_tag(current_slice, b"#TITLETRANSLIT:".len());
            } else if starts_with_tag_ci(current_slice, b"#SUBTITLETRANSLIT:") {
                result.subtitle_translit = parse_tag(current_slice, b"#SUBTITLETRANSLIT:".len());
            } else if starts_with_tag_ci(current_slice, b"#ARTISTTRANSLIT:") {
                result.artist_translit = parse_tag(current_slice, b"#ARTISTTRANSLIT:".len());
            } else if starts_with_tag_ci(current_slice, b"#VERSION:") {
                result.version = parse_tag(current_slice, b"#VERSION:".len());
            } else if starts_with_tag_ci(current_slice, b"#OFFSET:") {
                result.offset = parse_tag(current_slice, b"#OFFSET:".len());
            } else if starts_with_tag_ci(current_slice, b"#BPMS:") {
                result.bpms = parse_tag(current_slice, b"#BPMS:".len());
            } else if starts_with_tag_ci(current_slice, b"#STOPS:") {
                result.stops = parse_tag(current_slice, b"#STOPS:".len());
            } else if starts_with_tag_ci(current_slice, b"#FREEZES:") {
                // Older charts sometimes use #FREEZES instead of #STOPS.
                result.stops = parse_tag(current_slice, b"#FREEZES:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#FAKES:") {
                result.fakes = parse_tag(current_slice, b"#FAKES:".len());
            } else if starts_with_tag_ci(current_slice, b"#DELAYS:") {
                result.delays = parse_tag(current_slice, b"#DELAYS:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#WARPS:") {
                result.warps = parse_tag(current_slice, b"#WARPS:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#SPEEDS:") {
                result.speeds = parse_tag(current_slice, b"#SPEEDS:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#SCROLLS:") {
                result.scrolls = parse_tag(current_slice, b"#SCROLLS:".len());
            } else if starts_with_tag_ci(current_slice, b"#TIMESIGNATURES:") {
                result.time_signatures = parse_tag(current_slice, b"#TIMESIGNATURES:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#LABELS:") {
                result.labels = parse_tag(current_slice, b"#LABELS:".len());
            } else if starts_with_tag_ci(current_slice, b"#TICKCOUNTS:") {
                result.tickcounts = parse_tag(current_slice, b"#TICKCOUNTS:".len());
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#COMBOS:") {
                result.combos = parse_tag(current_slice, b"#COMBOS:".len());
            } else if starts_with_tag_ci(current_slice, b"#BANNER:") {
                result.banner = parse_tag(current_slice, b"#BANNER:".len());
            } else if starts_with_tag_ci(current_slice, b"#BACKGROUND:") {
                result.background = parse_tag(current_slice, b"#BACKGROUND:".len());
            } else if starts_with_tag_ci(current_slice, b"#MUSIC:") {
                result.music = parse_tag(current_slice, b"#MUSIC:".len());
            } else if starts_with_tag_ci(current_slice, b"#SAMPLESTART:") {
                result.sample_start = parse_tag(current_slice, b"#SAMPLESTART:".len());
            } else if starts_with_tag_ci(current_slice, b"#SAMPLELENGTH:") {
                result.sample_length = parse_tag(current_slice, b"#SAMPLELENGTH:".len());
            } else if starts_with_tag_ci(current_slice, b"#DISPLAYBPM:") {
                result.display_bpm = parse_tag(current_slice, b"#DISPLAYBPM:".len());    
            } else if is_ssc && starts_with_tag_ci(current_slice, b"#NOTEDATA:") {
                let notedata_start = i;
                let mut notedata_end = notedata_start + 1;
                while notedata_end < data.len()
                    && !starts_with_tag_ci(&data[notedata_end..], b"#NOTEDATA:")
                {
                    notedata_end += 1;
                }

                let notedata_slice = &data[notedata_start..notedata_end];
                let fields = parse_notedata_fields(notedata_slice);
                let notes = join_notes_fields([
                    fields.step_type.unwrap_or_default(),
                    fields.description.unwrap_or_default(),
                    fields.difficulty.unwrap_or_default(),
                    fields.meter.unwrap_or_default(),
                    fields.credit.unwrap_or_default(),
                    fields.notes.or(fields.notes2).unwrap_or_default(),
                ]);
                let chart_stops = fields.chart_stops.or(fields.chart_freezes);
                result.notes_list.push(ParsedChartEntry {
                    notes,
                    chart_bpms: fields.chart_bpms.map(Cow::Borrowed),
                    chart_stops: chart_stops.map(Cow::Borrowed),
                    chart_delays: fields.chart_delays.map(Cow::Borrowed),
                    chart_warps: fields.chart_warps.map(Cow::Borrowed),
                    chart_speeds: fields.chart_speeds.map(Cow::Borrowed),
                    chart_scrolls: fields.chart_scrolls.map(Cow::Borrowed),
                    chart_fakes: fields.chart_fakes.map(Cow::Borrowed),
                    chart_offset: fields.chart_offset.map(Cow::Borrowed),
                    chart_display_bpm: fields.chart_display_bpm.map(Cow::Borrowed),
                    chart_time_signatures: fields.chart_time_signatures.map(Cow::Borrowed),
                    chart_labels: fields.chart_labels.map(Cow::Borrowed),
                    chart_tickcounts: fields.chart_tickcounts.map(Cow::Borrowed),
                    chart_combos: fields.chart_combos.map(Cow::Borrowed),
                    chart_radar_values: fields.chart_radar_values.map(Cow::Borrowed),
                });

                i = notedata_end;
                continue; // Skip the i += 1 at the end
            } else if !is_ssc
                && (starts_with_tag_ci(current_slice, b"#NOTES:")
                    || starts_with_tag_ci(current_slice, b"#NOTES2:"))
            {
                let notes_tag_len = if starts_with_tag_ci(current_slice, b"#NOTES2:") {
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
                result.notes_list.push(ParsedChartEntry {
                    notes: block,
                    chart_bpms: None,
                    chart_stops: None,
                    chart_delays: None,
                    chart_warps: None,
                    chart_speeds: None,
                    chart_scrolls: None,
                    chart_fakes: None,
                    chart_offset: None,
                    chart_display_bpm: None,
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
    let mut bs_run = 0usize;
    while i < slice.len() {
        match slice[i] {
            b';' => {
                if bs_run % 2 == 0 {
                    return Some(&slice[..i]);
                }
                bs_run = 0;
            }
            // Fallback for malformed tags missing a terminating semicolon: if the next
            // line starts a new tag (`#...:`), stop at this line break.
            b'\n' | b'\r' => {
                bs_run = 0;
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
            b'\\' => {
                bs_run += 1;
            }
            _ => {}
        }
        if slice[i] != b'\\' {
            bs_run = 0;
        }
        i += 1;
    }
    None
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
    let mut end = rest.len();
    let mut k = 0usize;
    while k < rest.len() {
        if rest[k] == b':' {
            let mut bs_count = 0;
            let mut j = k;
            while j > 0 && rest[j - 1] == b'\\' {
                bs_count += 1;
                j -= 1;
            }
            if bs_count % 2 == 0 {
                end = k;
                break;
            }
        }
        k += 1;
    }
    (fields, &rest[..end])
}
